//! Channel-based event streaming for real-time communication
//!
//! This module provides a pub/sub event system over Server-Sent Events (SSE):
//! - **Reader**: Subscribe-only client for frontend applications
//! - **Actor**: Subscribe + publish client for backend applications
//! - **Receiver**: Meta-listener that spawns actors for each new transaction
//!
//! ## Endpoints
//! - `GET /payment/v1/channels/{channelId}` - SSE stream for channel events
//! - `POST /payment/v1/channels/{channelId}/attachments` - Attach processor data to transaction
//! - `GET /payment/v1/channels/transactions` - SSE stream for new transactions

use std::{collections::HashMap, convert::Infallible, sync::Arc};

use axum::{
    Extension,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Json,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
};
use cloudevents::AttributesReader;
use futures::stream::Stream;
// Re-export types from moneymq-types
pub use moneymq_types::{
    BasketItem, ChannelEvent, PaymentFailedData, PaymentSettledData, PaymentVerifiedData,
    ProductFeature, TransactionCompletedData, defaults, event_types,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::events::{
    CloudEventEnvelope, TransactionCompletedData as CloudTransactionCompletedData,
    create_transaction_completed_event,
};

/// Maximum number of events to buffer per channel
const CHANNEL_BUFFER_SIZE: usize = 100;

/// Default broadcast capacity for the transactions stream
const TRANSACTIONS_BUFFER_SIZE: usize = 100;

// ==================== Types ====================

/// Payment details from x402 payment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDetails {
    /// Payer address/wallet
    pub payer: String,
    /// Transaction ID/signature
    pub transaction_id: String,
    /// Payment amount as string
    pub amount: String,
    /// Currency code (e.g., "USDC")
    pub currency: String,
    /// Network name (e.g., "solana")
    pub network: String,
}

/// Transaction notification sent to receivers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionNotification {
    /// Transaction ID
    pub id: String,
    /// Channel ID for this transaction (used to create actor)
    pub channel_id: String,
    /// Basket items (products being purchased with features)
    #[serde(default)]
    pub basket: Vec<BasketItem>,
    /// Payment details from x402
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment: Option<PaymentDetails>,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Query parameters for channel SSE endpoint
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelQuery {
    /// Stream ID for stateful cursor tracking
    pub stream_id: Option<String>,
    /// Replay last N events on connect
    pub replay: Option<usize>,
    /// Authentication token (for actors)
    pub token: Option<String>,
}

/// Query parameters for transactions SSE endpoint
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TransactionsQuery {
    /// Authentication token (required)
    pub token: Option<String>,
}

/// Request body for publishing an event (legacy, with type)
#[derive(Debug, Clone, Deserialize)]
pub struct PublishEventRequest {
    /// Event type
    #[serde(rename = "type")]
    pub ty: String,
    /// Event data payload
    pub data: serde_json::Value,
}

/// Request body for attaching data to a transaction
#[derive(Debug, Clone, Deserialize)]
pub struct AttachDataRequest {
    /// Actor ID (identifies the hook/processor attaching data)
    /// This becomes the outer key in the attachments structure
    pub actor_id: String,

    /// Attachment key (identifies which hook attachment this fulfills)
    /// e.g., "surfnet" matches the hook config's `attachments: [{key: surfnet}]`
    pub key: String,

    /// Data payload to attach
    pub data: serde_json::Value,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

// ==================== Channel Manager ====================

/// Per-channel broadcaster
struct ChannelBroadcaster {
    tx: broadcast::Sender<ChannelEvent>,
    /// Recent events for replay
    recent_events: RwLock<Vec<ChannelEvent>>,
    max_events: usize,
}

impl ChannelBroadcaster {
    fn new(buffer_size: usize) -> Self {
        let (tx, _) = broadcast::channel(buffer_size);
        Self {
            tx,
            recent_events: RwLock::new(Vec::with_capacity(buffer_size)),
            max_events: buffer_size,
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<ChannelEvent> {
        self.tx.subscribe()
    }

    fn broadcast(&self, event: ChannelEvent) {
        // Store for replay
        {
            let mut events = self.recent_events.write();
            if events.len() >= self.max_events {
                events.remove(0);
            }
            events.push(event.clone());
        }

        // Broadcast to subscribers (ignore errors if no subscribers)
        let _ = self.tx.send(event);
    }

    fn get_replay_events(&self, count: usize) -> Vec<ChannelEvent> {
        let events = self.recent_events.read();
        events.iter().rev().take(count).rev().cloned().collect()
    }

    fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// Context for channel streams - includes payment stack context
#[derive(Debug, Clone)]
pub struct ChannelContext {
    pub payment_stack_id: String,
    pub is_sandbox: bool,
}

/// Inner attachments for a single actor (keyed by attachment key)
pub type ActorAttachments = HashMap<String, serde_json::Value>;

/// Pending attachments for a transaction (keyed by actor_id, then by key)
/// Structure: { actor_id: { key: data } }
pub type PendingAttachments = HashMap<String, ActorAttachments>;

/// Required attachment configuration from hook actors
#[derive(Debug, Clone, Default)]
pub struct RequiredAttachments {
    /// List of required attachment keys
    pub keys: Vec<String>,
}

/// Manages all channels and transaction notifications
pub struct ChannelManager {
    /// Per-channel broadcasters
    channels: RwLock<HashMap<String, Arc<ChannelBroadcaster>>>,
    /// Transaction stream for receivers
    transactions_tx: broadcast::Sender<TransactionNotification>,
    /// API secret for authentication
    secret: Option<String>,
    /// Database manager for cursor persistence
    db_manager: Option<Arc<crate::api::payment::db::DbManager>>,
    /// Context for channel streams
    context: Option<ChannelContext>,
    /// JWT key pair for signing payment receipts
    jwt_key_pair: Option<Arc<super::jwt::JwtKeyPair>>,
    /// Pending attachments per transaction (channel_id -> attachments)
    pending_attachments: RwLock<HashMap<String, PendingAttachments>>,
    /// Required attachments configuration (from hook actors)
    required_attachments: RequiredAttachments,
}

impl ChannelManager {
    /// Create a new channel manager (in-memory only)
    pub fn new(secret: Option<String>) -> Self {
        let (transactions_tx, _) = broadcast::channel(TRANSACTIONS_BUFFER_SIZE);
        Self {
            channels: RwLock::new(HashMap::new()),
            transactions_tx,
            secret,
            db_manager: None,
            context: None,
            jwt_key_pair: None,
            pending_attachments: RwLock::new(HashMap::new()),
            required_attachments: RequiredAttachments::default(),
        }
    }

    /// Create a new channel manager with DB persistence for cursors
    pub fn with_db(
        secret: Option<String>,
        db_manager: Arc<crate::api::payment::db::DbManager>,
        context: ChannelContext,
    ) -> Self {
        let (transactions_tx, _) = broadcast::channel(TRANSACTIONS_BUFFER_SIZE);
        Self {
            channels: RwLock::new(HashMap::new()),
            transactions_tx,
            secret,
            db_manager: Some(db_manager),
            context: Some(context),
            jwt_key_pair: None,
            pending_attachments: RwLock::new(HashMap::new()),
            required_attachments: RequiredAttachments::default(),
        }
    }

    /// Set the JWT key pair for signing payment receipts
    pub fn with_jwt_key_pair(mut self, key_pair: Arc<super::jwt::JwtKeyPair>) -> Self {
        self.jwt_key_pair = Some(key_pair);
        self
    }

    /// Set the required attachments configuration (from hook actors)
    pub fn with_required_attachments(mut self, keys: Vec<String>) -> Self {
        self.required_attachments = RequiredAttachments { keys };
        self
    }

    /// Check if all required attachments are present for a transaction
    /// Searches across all actors for the required keys
    pub fn has_all_required_attachments(&self, channel_id: &str) -> bool {
        if self.required_attachments.keys.is_empty() {
            // No required attachments configured - always ready
            return true;
        }

        let pending = self.pending_attachments.read();
        if let Some(actor_attachments) = pending.get(channel_id) {
            // Check if each required key exists in any actor's attachments
            self.required_attachments.keys.iter().all(|key| {
                actor_attachments
                    .values()
                    .any(|actor_data| actor_data.contains_key(key))
            })
        } else {
            false
        }
    }

    /// Store an attachment for a transaction
    /// Attachments are stored as: channel_id -> actor_id -> key -> data
    pub fn store_attachment(
        &self,
        channel_id: &str,
        actor_id: String,
        key: String,
        data: serde_json::Value,
    ) {
        let mut pending = self.pending_attachments.write();
        pending
            .entry(channel_id.to_string())
            .or_insert_with(HashMap::new)
            .entry(actor_id)
            .or_insert_with(HashMap::new)
            .insert(key, data);
    }

    /// Get all attachments for a transaction (consumes them)
    pub fn take_attachments(&self, channel_id: &str) -> Option<PendingAttachments> {
        let mut pending = self.pending_attachments.write();
        pending.remove(channel_id)
    }

    /// Get attachments without consuming them
    pub fn get_attachments(&self, channel_id: &str) -> Option<PendingAttachments> {
        let pending = self.pending_attachments.read();
        pending.get(channel_id).cloned()
    }

    /// Get cursor for a stream ID from DB
    pub fn get_stream_cursor(&self, stream_id: &str) -> Option<String> {
        let (db, ctx) = match (&self.db_manager, &self.context) {
            (Some(db), Some(ctx)) => (db, ctx),
            _ => return None,
        };

        match db.find_or_create_event_stream(stream_id, &ctx.payment_stack_id, ctx.is_sandbox) {
            Ok(stream) => stream.last_event_id,
            Err(e) => {
                error!("Failed to get stream cursor: {}", e);
                None
            }
        }
    }

    /// Update cursor for a stream ID in DB
    pub fn set_stream_cursor(&self, stream_id: &str, event_id: &str) {
        let (db, ctx) = match (&self.db_manager, &self.context) {
            (Some(db), Some(ctx)) => (db, ctx),
            _ => return,
        };

        let event_time = chrono::Utc::now().timestamp_millis();
        if let Err(e) = db.update_event_stream_cursor(
            stream_id,
            &ctx.payment_stack_id,
            ctx.is_sandbox,
            event_id,
            event_time,
        ) {
            error!("Failed to update stream cursor: {}", e);
        }
    }

    /// Get or create a channel broadcaster
    fn get_or_create_channel(&self, channel_id: &str) -> Arc<ChannelBroadcaster> {
        // Fast path: read lock
        {
            let channels = self.channels.read();
            if let Some(broadcaster) = channels.get(channel_id) {
                return Arc::clone(broadcaster);
            }
        }

        // Slow path: write lock to create
        let mut channels = self.channels.write();
        // Double-check after acquiring write lock
        if let Some(broadcaster) = channels.get(channel_id) {
            return Arc::clone(broadcaster);
        }

        let broadcaster = Arc::new(ChannelBroadcaster::new(CHANNEL_BUFFER_SIZE));
        channels.insert(channel_id.to_string(), Arc::clone(&broadcaster));
        info!(channel_id = %channel_id, "Created new channel");
        broadcaster
    }

    /// Subscribe to a channel
    pub fn subscribe(
        &self,
        channel_id: &str,
    ) -> (broadcast::Receiver<ChannelEvent>, Vec<ChannelEvent>, usize) {
        let broadcaster = self.get_or_create_channel(channel_id);
        let rx = broadcaster.subscribe();
        let replay = vec![]; // Replay is handled separately
        let subscriber_count = broadcaster.subscriber_count();
        (rx, replay, subscriber_count)
    }

    /// Get replay events for a channel
    pub fn get_replay_events(&self, channel_id: &str, count: usize) -> Vec<ChannelEvent> {
        let channels = self.channels.read();
        channels
            .get(channel_id)
            .map(|b| b.get_replay_events(count))
            .unwrap_or_default()
    }

    /// Get events after a specific cursor (for stateful streams)
    pub fn get_events_after_cursor(&self, channel_id: &str, cursor: &str) -> Vec<ChannelEvent> {
        let channels = self.channels.read();
        let Some(broadcaster) = channels.get(channel_id) else {
            return vec![];
        };

        let events = broadcaster.recent_events.read();
        // Find cursor position and return all events after it
        if let Some(pos) = events.iter().position(|e| e.id() == cursor) {
            events[pos + 1..].to_vec()
        } else {
            // Cursor not found, return all events
            events.clone()
        }
    }

    /// Publish an event to a channel
    pub fn publish(&self, channel_id: &str, event: ChannelEvent) {
        let broadcaster = self.get_or_create_channel(channel_id);
        info!(
            channel_id = %channel_id,
            event_id = %event.id(),
            event_type = %event.event_type(),
            subscribers = %broadcaster.subscriber_count(),
            "Publishing event to channel"
        );
        broadcaster.broadcast(event);
    }

    /// Notify receivers about a new transaction
    pub fn notify_transaction(&self, notification: TransactionNotification) {
        let amount = notification
            .payment
            .as_ref()
            .map(|p| p.amount.as_str())
            .unwrap_or("0");
        info!(
            tx_id = %notification.id,
            channel_id = %notification.channel_id,
            basket_items = %notification.basket.len(),
            amount = %amount,
            "Notifying receivers of new transaction"
        );
        let _ = self.transactions_tx.send(notification);
    }

    /// Subscribe to transaction notifications
    pub fn subscribe_transactions(&self) -> broadcast::Receiver<TransactionNotification> {
        self.transactions_tx.subscribe()
    }

    /// Check if there are any hook subscribers listening for transactions
    pub fn has_hook_subscribers(&self) -> bool {
        self.transactions_tx.receiver_count() > 0
    }

    /// Get the number of hook subscribers
    pub fn hook_subscriber_count(&self) -> usize {
        self.transactions_tx.receiver_count()
    }

    /// Validate authentication token
    pub fn validate_token(&self, token: Option<&str>) -> bool {
        match (&self.secret, token) {
            (Some(secret), Some(token)) => secret == token,
            (None, _) => true,        // No secret configured = no auth required
            (Some(_), None) => false, // Secret configured but no token provided
        }
    }

    /// Extract token from Authorization header or query param
    pub fn extract_token(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
        // Try Authorization header first
        if let Some(auth) = headers.get("authorization")
            && let Ok(auth_str) = auth.to_str()
            && let Some(token) = auth_str.strip_prefix("Bearer ")
        {
            return Some(token.to_string());
        }
        // Fall back to query param
        query_token.map(|t| t.to_string())
    }
}

// ==================== Handlers ====================

/// GET /payment/v1/channels/{channelId} - SSE stream for channel events
pub async fn channel_sse_handler(
    Extension(manager): Extension<Arc<ChannelManager>>,
    Path(channel_id): Path<String>,
    Query(query): Query<ChannelQuery>,
) -> impl IntoResponse {
    info!(
        channel_id = %channel_id,
        stream_id = ?query.stream_id,
        replay = ?query.replay,
        "New SSE connection to channel"
    );

    // Subscribe FIRST, then get replay (to avoid missing events)
    let (rx, _, _) = manager.subscribe(&channel_id);

    // Determine replay events based on stateful vs stateless mode
    let replay_events = if let Some(ref stream_id) = query.stream_id {
        // Stateful mode: use cursor from DB
        if let Some(cursor) = manager.get_stream_cursor(stream_id) {
            debug!(stream_id = %stream_id, cursor = %cursor, "Resuming stateful stream from cursor");
            manager.get_events_after_cursor(&channel_id, &cursor)
        } else {
            debug!(stream_id = %stream_id, "New stateful stream, no cursor yet");
            vec![]
        }
    } else if let Some(count) = query.replay {
        // Stateless mode: replay last N events
        manager.get_replay_events(&channel_id, count)
    } else {
        vec![]
    };

    create_channel_sse_stream(
        Arc::clone(&manager),
        rx,
        replay_events,
        channel_id,
        query.stream_id,
    )
}

/// POST /payment/v1/channels/{channelId}/attachments - Attach processor data to transaction
///
/// Attachments are stored by key. Once all required attachments (configured via hooks)
/// are present, a `transaction:completed` event is emitted with a signed JWT receipt.
/// Until then, `transaction:attach` events are emitted to acknowledge each attachment.
pub async fn publish_attachment_handler(
    Extension(manager): Extension<Arc<ChannelManager>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Json(request): Json<AttachDataRequest>,
) -> impl IntoResponse {
    // Extract and validate token
    let token = ChannelManager::extract_token(&headers, None);
    if !manager.validate_token(token.as_deref()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                message: "Unauthorized".to_string(),
                code: Some("UNAUTHORIZED".to_string()),
            }),
        )
            .into_response();
    }

    info!(
        channel_id = %channel_id,
        actor_id = %request.actor_id,
        attachment_key = %request.key,
        required_keys = ?manager.required_attachments.keys,
        "Received attachment"
    );

    // Store the attachment by actor_id and key
    manager.store_attachment(
        &channel_id,
        request.actor_id.clone(),
        request.key.clone(),
        request.data.clone(),
    );

    // Check if all required attachments are now present
    let all_attachments_present = manager.has_all_required_attachments(&channel_id);

    info!(
        channel_id = %channel_id,
        all_attachments_present = %all_attachments_present,
        "Checking attachment requirements"
    );

    let event = if all_attachments_present {
        // All required attachments are present - create JWT receipt and emit transaction:completed
        create_completion_event(&manager, &channel_id)
    } else {
        // Not all attachments present yet - emit transaction:attach to acknowledge
        info!(
            channel_id = %channel_id,
            actor_id = %request.actor_id,
            attachment_key = %request.key,
            "Attachment stored, waiting for remaining required attachments"
        );
        ChannelEvent::custom(
            event_types::TRANSACTION_ATTACH,
            serde_json::json!({
                "actor_id": request.actor_id,
                "key": request.key,
                "acknowledged": true,
                "pending": manager.required_attachments.keys.iter()
                    .filter(|k| {
                        // Check if key exists in any actor's attachments
                        manager.get_attachments(&channel_id)
                            .map(|actors| !actors.values().any(|actor_data| actor_data.contains_key(*k)))
                            .unwrap_or(true)
                    })
                    .collect::<Vec<_>>()
            }),
        )
    };

    // Clone for response
    let event_response = event.clone();

    info!(
        channel_id = %channel_id,
        event_id = %event.id(),
        event_type = %event.event_type(),
        "Publishing event via HTTP"
    );

    manager.publish(&channel_id, event);

    (StatusCode::CREATED, Json(event_response)).into_response()
}

/// Create the transaction:completed event with JWT receipt containing all attachments
fn create_completion_event(manager: &Arc<ChannelManager>, channel_id: &str) -> ChannelEvent {
    info!(
        channel_id = %channel_id,
        has_jwt_key_pair = manager.jwt_key_pair.is_some(),
        has_db_manager = manager.db_manager.is_some(),
        has_context = manager.context.is_some(),
        "All attachments present - creating receipt JWT"
    );

    // Take all attachments for inclusion in the JWT
    let all_attachments = manager.take_attachments(channel_id).unwrap_or_default();

    match (&manager.jwt_key_pair, &manager.db_manager, &manager.context) {
        (Some(key_pair), Some(db_manager), Some(ctx)) => {
            // Look up transaction by channel_id (which is the payment_hash)
            info!(channel_id = %channel_id, "Looking up transaction by payment_hash");
            match db_manager.find_transaction_by_payment_hash(channel_id) {
                Ok(Some(tx)) => {
                    // Create JWT with payment claims + all processor attachments
                    let payer = tx
                        .customer
                        .as_ref()
                        .map(|c| c.address.clone())
                        .unwrap_or_default();

                    let network = defaults::NETWORK.to_string();

                    // Extract features from x402_payment_requirement (base64-encoded)
                    let features_from_req = tx
                        .x402_payment_requirement
                        .as_ref()
                        .and_then(|b64| {
                            use base64::{Engine as _, engine::general_purpose::STANDARD};
                            let decoded = STANDARD.decode(b64).ok()?;
                            let payment_req: moneymq_types::x402::PaymentRequirements =
                                serde_json::from_slice(&decoded).ok()?;
                            payment_req
                                .extra
                                .and_then(|extra| extra.get("features").cloned())
                        })
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                    // Parse basket from tx.product JSON array: [{"productId", "experimentId", "quantity"}]
                    let basket: Vec<super::jwt::BasketItem> = tx
                        .product
                        .as_ref()
                        .and_then(|s| serde_json::from_str::<Vec<serde_json::Value>>(s).ok())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| {
                                    let product_id = item.get("productId")?.as_str()?.to_string();
                                    let quantity =
                                        item.get("quantity").and_then(|v| v.as_i64()).unwrap_or(1)
                                            as u32;
                                    Some(super::jwt::BasketItem {
                                        product_id,
                                        experiment_id: None,
                                        features: features_from_req.clone(),
                                        quantity,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    // Convert nested attachments to JSON: { actor_id: { key: data } }
                    let attachments_map: serde_json::Map<String, serde_json::Value> =
                        all_attachments
                            .into_iter()
                            .map(|(actor_id, actor_data)| {
                                let inner_map: serde_json::Map<String, serde_json::Value> =
                                    actor_data.into_iter().collect();
                                (actor_id, serde_json::Value::Object(inner_map))
                            })
                            .collect();

                    info!(
                        channel_id = %channel_id,
                        attachments = ?attachments_map.keys().collect::<Vec<_>>(),
                        "Adding attachments to JWT claims"
                    );

                    let claims = super::jwt::PaymentReceiptClaims::new_with_basket(
                        channel_id.to_string(), // transaction_id = channel_id = payment_hash
                        payer,
                        tx.amount.clone(),
                        tx.currency
                            .clone()
                            .unwrap_or_else(|| defaults::CURRENCY.to_string()),
                        network,
                        basket,
                        tx.signature.clone(),
                        ctx.payment_stack_id.clone(),
                        defaults::JWT_EXPIRATION_HOURS,
                    )
                    .with_attachments(attachments_map);

                    // Sign the JWT
                    let currency = tx
                        .currency
                        .clone()
                        .unwrap_or_else(|| defaults::CURRENCY.to_string());
                    let payer_for_event = tx
                        .customer
                        .as_ref()
                        .map(|c| c.address.clone())
                        .unwrap_or_default();
                    let product_id = tx.product.as_ref().and_then(|s| {
                        serde_json::from_str::<Vec<serde_json::Value>>(s)
                            .ok()
                            .and_then(|items| {
                                items.first().and_then(|item| {
                                    item.get("productId")?.as_str().map(String::from)
                                })
                            })
                    });

                    match key_pair.sign(&claims) {
                        Ok(jwt) => {
                            info!(
                                channel_id = %channel_id,
                                "Created payment receipt JWT, emitting transaction:completed"
                            );

                            // Also persist as CloudEvent to DB for clients using /payment/v1/events
                            let cloud_event_data = CloudTransactionCompletedData {
                                transaction_id: channel_id.to_string(),
                                receipt: jwt.clone(),
                                payer: payer_for_event,
                                amount: tx.amount.clone(),
                                currency,
                                network: defaults::NETWORK.to_string(),
                                transaction_signature: tx.signature.clone(),
                                product_id,
                            };
                            let cloud_event = create_transaction_completed_event(cloud_event_data);

                            if let Some(envelope) = CloudEventEnvelope::from_sdk_event(&cloud_event)
                            {
                                if let Ok(json_str) = serde_json::to_string(&envelope) {
                                    let event_time = cloud_event
                                        .time()
                                        .map(|t| t.timestamp_millis())
                                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                    if let Err(e) = db_manager.insert_cloud_event(
                                        envelope.id.clone(),
                                        envelope.ty.clone(),
                                        envelope.source.clone(),
                                        event_time,
                                        json_str,
                                        &ctx.payment_stack_id,
                                        ctx.is_sandbox,
                                    ) {
                                        error!(
                                            "Failed to persist transaction:completed CloudEvent to DB: {}",
                                            e
                                        );
                                    } else {
                                        info!(
                                            event_id = %envelope.id,
                                            channel_id = %channel_id,
                                            "Transaction completed CloudEvent persisted to DB (with attachments)"
                                        );
                                    }
                                }
                            }

                            // Emit transaction:completed with the receipt
                            ChannelEvent::transaction_completed(TransactionCompletedData {
                                receipt: jwt,
                            })
                        }
                        Err(e) => {
                            error!("Failed to sign JWT for channel {}: {}", channel_id, e);
                            ChannelEvent::custom(
                                event_types::TRANSACTION_ATTACH,
                                serde_json::json!({"error": "Failed to create receipt"}),
                            )
                        }
                    }
                }
                Ok(None) => {
                    warn!(channel_id = %channel_id, "No transaction found for channel");
                    ChannelEvent::custom(
                        event_types::TRANSACTION_ATTACH,
                        serde_json::json!({"error": "Transaction not found"}),
                    )
                }
                Err(e) => {
                    error!(
                        "Failed to look up transaction for channel {}: {}",
                        channel_id, e
                    );
                    ChannelEvent::custom(
                        event_types::TRANSACTION_ATTACH,
                        serde_json::json!({"error": "Database error"}),
                    )
                }
            }
        }
        _ => {
            warn!("Cannot create receipt JWT: missing key_pair, db_manager, or context");
            ChannelEvent::custom(
                event_types::TRANSACTION_ATTACH,
                serde_json::json!({"error": "Server configuration error"}),
            )
        }
    }
}

/// GET /payment/v1/channels/transactions - SSE stream for new transactions
pub async fn transactions_sse_handler(
    Extension(manager): Extension<Arc<ChannelManager>>,
    Query(query): Query<TransactionsQuery>,
) -> impl IntoResponse {
    // Validate token
    if !manager.validate_token(query.token.as_deref()) {
        // For SSE, we can't return JSON error easily, so we return empty stream
        // In practice, frontend should handle 401 before establishing SSE
        warn!("Unauthorized transactions SSE connection attempt");
        return create_empty_sse_stream().into_response();
    }

    info!("New SSE connection to transactions stream");
    let rx = manager.subscribe_transactions();
    create_transactions_sse_stream(rx).into_response()
}

// ==================== SSE Stream Helpers ====================

/// Create SSE stream for a channel
fn create_channel_sse_stream(
    manager: Arc<ChannelManager>,
    rx: broadcast::Receiver<ChannelEvent>,
    replay_events: Vec<ChannelEvent>,
    channel_id: String,
    stream_id: Option<String>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = async_stream::stream! {
        // First, replay historical events
        for event in replay_events {
            let event_id = event.id().to_string();
            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(SseEvent::default()
                .id(event_id.clone())
                .event("message")
                .data(json));

            // Update cursor for stateful streams
            if let Some(ref sid) = stream_id {
                manager.set_stream_cursor(sid, &event_id);
            }
        }

        // Then switch to live events
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let event_id = event.id().to_string();
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    yield Ok(SseEvent::default()
                        .id(event_id.clone())
                        .event("message")
                        .data(json));

                    // Update cursor for stateful streams
                    if let Some(ref sid) = stream_id {
                        manager.set_stream_cursor(sid, &event_id);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    error!(channel_id = %channel_id, lagged = %n, "SSE client lagged behind");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!(channel_id = %channel_id, "Channel closed, ending SSE stream");
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Create SSE stream for transactions
fn create_transactions_sse_stream(
    rx: broadcast::Receiver<TransactionNotification>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(notification) => {
                    let json = serde_json::to_string(&notification).unwrap_or_default();
                    debug!(
                        tx_id = %notification.id,
                        channel_id = %notification.channel_id,
                        "Sending transaction to SSE client"
                    );
                    yield Ok(SseEvent::default()
                        .id(notification.id.clone())
                        .event("transaction")
                        .data(json));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    error!(lagged = %n, "Transactions SSE client lagged behind");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Transactions stream closed, ending SSE");
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Create an empty SSE stream (for unauthorized requests)
fn create_empty_sse_stream() -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = async_stream::stream! {
        // Immediately end the stream
        yield Ok(SseEvent::default().event("error").data("Unauthorized"));
    };
    Sse::new(stream)
}

// ==================== Router ====================

/// Create the channels router
pub fn create_router(manager: Arc<ChannelManager>) -> axum::Router<()> {
    use axum::routing::{get, post};

    axum::Router::new()
        // SSE endpoint for channel events
        .route("/channels/{channel_id}", get(channel_sse_handler))
        // HTTP endpoint to publish events
        .route(
            "/channels/{channel_id}/attachments",
            post(publish_attachment_handler),
        )
        // SSE endpoint for transaction notifications
        .route("/channels/transactions", get(transactions_sse_handler))
        .layer(Extension(manager))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_event_creation() {
        // Test typed event
        let event = ChannelEvent::payment_settled(PaymentSettledData {
            payer: "wallet123".to_string(),
            amount: "1000".to_string(),
            currency: "USDC".to_string(),
            network: "solana".to_string(),
            transaction_signature: None,
            product_id: None,
        });
        assert!(!event.id().is_empty());
        assert_eq!(event.event_type(), "payment:settled");

        // Test custom event
        let custom = ChannelEvent::custom("test:custom", serde_json::json!({"amount": 1000}));
        assert_eq!(custom.event_type(), "test:custom");
    }

    #[test]
    fn test_channel_manager_publish_subscribe() {
        let manager = ChannelManager::new(None);
        let channel_id = "test-channel";

        // Subscribe first
        let (mut rx, _, _) = manager.subscribe(channel_id);

        // Publish event
        let event = ChannelEvent::custom("test", serde_json::json!({"hello": "world"}));
        let event_id = event.id().to_string();
        manager.publish(channel_id, event);

        // Should receive event
        let received = rx.try_recv().expect("Should receive event");
        assert_eq!(received.id(), event_id);
    }

    #[test]
    fn test_channel_manager_replay() {
        let manager = ChannelManager::new(None);
        let channel_id = "replay-channel";

        // Publish events before subscribing
        for i in 0..5 {
            let event = ChannelEvent::custom("test", serde_json::json!({"index": i}));
            manager.publish(channel_id, event);
        }

        // Get replay (last 3)
        let replay = manager.get_replay_events(channel_id, 3);
        assert_eq!(replay.len(), 3);

        // Verify using JSON serialization since we can't directly access data
        let json0 = serde_json::to_value(&replay[0]).unwrap();
        let json2 = serde_json::to_value(&replay[2]).unwrap();
        assert_eq!(json0["data"]["index"], 2);
        assert_eq!(json2["data"]["index"], 4);
    }

    #[test]
    fn test_token_validation() {
        let manager = ChannelManager::new(Some("secret123".to_string()));

        assert!(manager.validate_token(Some("secret123")));
        assert!(!manager.validate_token(Some("wrong")));
        assert!(!manager.validate_token(None));

        // No secret = always valid
        let manager_no_auth = ChannelManager::new(None);
        assert!(manager_no_auth.validate_token(None));
        assert!(manager_no_auth.validate_token(Some("anything")));
    }

    #[test]
    fn test_transaction_notifications() {
        let manager = ChannelManager::new(None);

        let mut rx = manager.subscribe_transactions();

        let notification = TransactionNotification {
            id: "tx_123".to_string(),
            channel_id: "order-123".to_string(),
            basket: vec![BasketItem {
                product_id: "prod_abc".to_string(),
                experiment_id: None,
                features: serde_json::Value::default(),
                quantity: 1,
            }],
            payment: Some(PaymentDetails {
                payer: "wallet123".to_string(),
                transaction_id: "tx_123".to_string(),
                amount: "1000".to_string(),
                currency: defaults::CURRENCY.to_string(),
                network: defaults::NETWORK.to_string(),
            }),
            metadata: None,
        };

        manager.notify_transaction(notification.clone());

        let received = rx.try_recv().expect("Should receive notification");
        assert_eq!(received.id, "tx_123");
        assert_eq!(received.channel_id, "order-123");
        assert_eq!(received.basket.len(), 1);
        assert_eq!(received.basket[0].product_id, "prod_abc");
        assert!(received.payment.is_some());
    }

    #[test]
    fn test_attachment_tracking_no_requirements() {
        // Without required attachments, has_all_required_attachments should return true
        let manager = ChannelManager::new(None);
        let channel_id = "test-channel";

        assert!(manager.has_all_required_attachments(channel_id));

        // Even with an attachment, it should still return true (no requirements)
        manager.store_attachment(
            channel_id,
            "processor-1".to_string(),
            "surfnet".to_string(),
            serde_json::json!({"rpc": "http://localhost:8899"}),
        );
        assert!(manager.has_all_required_attachments(channel_id));
    }

    #[test]
    fn test_attachment_tracking_with_requirements() {
        // With required attachments, should only be complete when all are present
        let manager = ChannelManager::new(None)
            .with_required_attachments(vec!["surfnet".to_string(), "billing".to_string()]);
        let channel_id = "test-channel";

        // Not complete initially
        assert!(!manager.has_all_required_attachments(channel_id));

        // Add one attachment - still not complete
        manager.store_attachment(
            channel_id,
            "processor-1".to_string(),
            "surfnet".to_string(),
            serde_json::json!({"rpc": "http://localhost:8899"}),
        );
        assert!(!manager.has_all_required_attachments(channel_id));

        // Verify attachment is stored (now nested under actor_id)
        let attachments = manager.get_attachments(channel_id);
        assert!(attachments.is_some());
        let actors = attachments.as_ref().unwrap();
        assert!(actors.contains_key("processor-1"));
        assert!(actors.get("processor-1").unwrap().contains_key("surfnet"));

        // Add the second required attachment from a different actor - now complete
        manager.store_attachment(
            channel_id,
            "processor-2".to_string(),
            "billing".to_string(),
            serde_json::json!({"customer_id": "cust_123"}),
        );
        assert!(manager.has_all_required_attachments(channel_id));

        // Take all attachments
        let taken = manager.take_attachments(channel_id);
        assert!(taken.is_some());
        let taken = taken.unwrap();
        // Now we have 2 actors, not 2 keys at the top level
        assert_eq!(taken.len(), 2);
        assert!(taken.contains_key("processor-1"));
        assert!(taken.contains_key("processor-2"));
        assert!(taken.get("processor-1").unwrap().contains_key("surfnet"));
        assert!(taken.get("processor-2").unwrap().contains_key("billing"));

        // After taking, attachments are gone
        assert!(manager.get_attachments(channel_id).is_none());
        assert!(!manager.has_all_required_attachments(channel_id));
    }

    #[test]
    fn test_attachment_tracking_extra_attachments() {
        // Extra attachments beyond requirements should not prevent completion
        let manager =
            ChannelManager::new(None).with_required_attachments(vec!["surfnet".to_string()]);
        let channel_id = "test-channel";

        // Add required attachment
        manager.store_attachment(
            channel_id,
            "processor-1".to_string(),
            "surfnet".to_string(),
            serde_json::json!({"rpc": "http://localhost:8899"}),
        );
        assert!(manager.has_all_required_attachments(channel_id));

        // Add extra attachment from same actor - should still be complete
        manager.store_attachment(
            channel_id,
            "processor-1".to_string(),
            "extra".to_string(),
            serde_json::json!({"foo": "bar"}),
        );
        assert!(manager.has_all_required_attachments(channel_id));

        // One actor with two attachments
        let attachments = manager.get_attachments(channel_id);
        assert!(attachments.is_some());
        let actors = attachments.as_ref().unwrap();
        assert_eq!(actors.len(), 1); // One actor
        assert_eq!(actors.get("processor-1").unwrap().len(), 2); // Two attachments for that actor
    }
}
