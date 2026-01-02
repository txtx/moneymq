use std::{
    collections::VecDeque,
    convert::Infallible,
    sync::Arc,
    thread::{self, JoinHandle},
};

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use chrono::{DateTime, Utc};
use cloudevents::{AttributesReader, Event, EventBuilder, EventBuilderV10};
use crossbeam_channel::{Receiver, Sender, unbounded};
use futures::stream::Stream;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::api::payment::db::DbManager;

/// Default source for MoneyMQ events
pub const EVENT_SOURCE: &str = "moneymq";

/// Payment flow type - indicates how the payment was initiated
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentFlow {
    /// Payment via x402 protocol (agentic payments)
    X402,
    /// Payment via checkout flow (with payment intent)
    Checkout {
        /// The payment intent ID
        intent_id: String,
    },
}

/// Data payload for payment verification succeeded event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentVerificationSucceededData {
    pub payer: String,
    pub amount: String,
    pub network: String,
    pub product_id: Option<String>,
    pub payment_flow: PaymentFlow,
}

/// Data payload for payment verification failed event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentVerificationFailedData {
    pub payer: Option<String>,
    pub amount: String,
    pub network: String,
    pub reason: String,
    pub product_id: Option<String>,
    pub payment_flow: PaymentFlow,
}

/// Data payload for payment settlement succeeded event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSettlementSucceededData {
    pub payer: String,
    pub amount: String,
    pub network: String,
    pub transaction_signature: Option<String>,
    pub product_id: Option<String>,
    pub payment_flow: PaymentFlow,
}

/// Data payload for payment settlement failed event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSettlementFailedData {
    pub payer: Option<String>,
    pub amount: String,
    pub network: String,
    pub reason: String,
    pub product_id: Option<String>,
    pub payment_flow: PaymentFlow,
}

/// Enum of all possible CloudEvent types emitted by MoneyMQ
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CloudEvent {
    #[serde(rename = "mq.money.payment.verification.succeeded")]
    PaymentVerificationSucceeded(PaymentVerificationSucceededData),
    #[serde(rename = "mq.money.payment.verification.failed")]
    PaymentVerificationFailed(PaymentVerificationFailedData),
    #[serde(rename = "mq.money.payment.settlement.succeeded")]
    PaymentSettlementSucceeded(PaymentSettlementSucceededData),
    #[serde(rename = "mq.money.payment.settlement.failed")]
    PaymentSettlementFailed(PaymentSettlementFailedData),
}

impl CloudEvent {
    /// Get the CloudEvents type string for this event
    pub fn event_type(&self) -> &'static str {
        match self {
            CloudEvent::PaymentVerificationSucceeded(_) => {
                "mq.money.payment.verification.succeeded"
            }
            CloudEvent::PaymentVerificationFailed(_) => "mq.money.payment.verification.failed",
            CloudEvent::PaymentSettlementSucceeded(_) => "mq.money.payment.settlement.succeeded",
            CloudEvent::PaymentSettlementFailed(_) => "mq.money.payment.settlement.failed",
        }
    }

    /// Get the source path for this event
    pub fn source(&self) -> &'static str {
        match self {
            CloudEvent::PaymentVerificationSucceeded(_) => "moneymq/payment/verify",
            CloudEvent::PaymentVerificationFailed(_) => "moneymq/payment/verify",
            CloudEvent::PaymentSettlementSucceeded(_) => "moneymq/payment/settle",
            CloudEvent::PaymentSettlementFailed(_) => "moneymq/payment/settle",
        }
    }

    /// Get the SSE event name for this event
    pub fn sse_event_name(&self) -> &'static str {
        match self {
            CloudEvent::PaymentVerificationSucceeded(_) => "payment.verification.succeeded",
            CloudEvent::PaymentVerificationFailed(_) => "payment.verification.failed",
            CloudEvent::PaymentSettlementSucceeded(_) => "payment.settlement.succeeded",
            CloudEvent::PaymentSettlementFailed(_) => "payment.settlement.failed",
        }
    }
}

/// Serializable representation for SSE output
/// Follows the CloudEvents v1.0 JSON format specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEventEnvelope {
    pub specversion: String,
    pub id: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub source: String,
    pub time: DateTime<Utc>,
    pub datacontenttype: String,
    pub data: serde_json::Value,
}

impl CloudEventEnvelope {
    /// Create from a cloudevents::Event (SDK type)
    pub fn from_sdk_event(event: &cloudevents::Event) -> Option<Self> {
        let data_value = event.data().and_then(|d| match d {
            cloudevents::Data::Json(v) => Some(v.clone()),
            cloudevents::Data::String(s) => serde_json::from_str(s).ok(),
            cloudevents::Data::Binary(b) => serde_json::from_slice(b).ok(),
        })?;

        Some(Self {
            specversion: event.specversion().to_string(),
            id: event.id().to_string(),
            ty: event.ty().to_string(),
            source: event.source().to_string(),
            time: event.time().cloned().unwrap_or_else(chrono::Utc::now),
            datacontenttype: "application/json".to_string(),
            data: data_value,
        })
    }
}

/// Stored event for replay functionality
#[derive(Debug, Clone)]
pub struct StoredEvent {
    /// The event ID (UUID)
    pub id: String,
    /// Serialized JSON representation
    pub json: String,
}

/// Event store for cursor-based replay
/// Stores recent events in memory for clients to replay on reconnection
pub struct EventStore {
    events: RwLock<VecDeque<StoredEvent>>,
    max_events: usize,
}

impl EventStore {
    /// Create a new event store with the given capacity
    pub fn new(max_events: usize) -> Self {
        Self {
            events: RwLock::new(VecDeque::with_capacity(max_events)),
            max_events,
        }
    }

    /// Store an event for later replay
    pub fn store(&self, id: String, json: String) {
        let mut events = self.events.write();
        if events.len() >= self.max_events {
            events.pop_front();
        }
        events.push_back(StoredEvent { id, json });
    }

    /// Get events starting from the given cursor (exclusive)
    /// Returns events that occurred after the cursor
    pub fn get_from_cursor(&self, cursor: &str) -> Vec<StoredEvent> {
        let events = self.events.read();
        let mut found = false;
        events
            .iter()
            .filter(|e| {
                if found {
                    return true;
                }
                if e.id == cursor {
                    found = true;
                }
                false
            })
            .cloned()
            .collect()
    }

    /// Get the last N events
    pub fn get_last(&self, n: usize) -> Vec<StoredEvent> {
        let events = self.events.read();
        events.iter().rev().take(n).rev().cloned().collect()
    }

    /// Get the current cursor (ID of the most recent event)
    pub fn current_cursor(&self) -> Option<String> {
        self.events.read().back().map(|e| e.id.clone())
    }

    /// Get total number of stored events
    pub fn len(&self) -> usize {
        self.events.read().len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.events.read().is_empty()
    }
}

/// Query parameters for SSE event stream
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EventStreamQuery {
    /// Resume from this cursor (event ID) - exclusive, returns events after this ID
    pub cursor: Option<String>,
    /// Replay the last N events before switching to live
    pub last: Option<usize>,
    /// Stream ID for stateful streams - enables server-side cursor persistence
    /// When provided, the server tracks the last consumed event for this stream
    pub stream_id: Option<String>,
}

/// Creates a CloudEvent from event data using the official SDK
pub fn create_event(data: CloudEvent) -> Event {
    let event_type = data.event_type();
    let source = data.source();

    // Extract the inner data payload (without the enum tag wrapper)
    let data_value = match &data {
        CloudEvent::PaymentVerificationSucceeded(d) => {
            serde_json::to_value(d).expect("Failed to serialize event data")
        }
        CloudEvent::PaymentVerificationFailed(d) => {
            serde_json::to_value(d).expect("Failed to serialize event data")
        }
        CloudEvent::PaymentSettlementSucceeded(d) => {
            serde_json::to_value(d).expect("Failed to serialize event data")
        }
        CloudEvent::PaymentSettlementFailed(d) => {
            serde_json::to_value(d).expect("Failed to serialize event data")
        }
    };

    EventBuilderV10::new()
        .id(uuid::Uuid::new_v4().to_string())
        .ty(event_type)
        .source(source)
        .time(chrono::Utc::now())
        .data("application/json", data_value)
        .build()
        .expect("Failed to build CloudEvent")
}

/// Convenience function to create a payment verification succeeded event
pub fn create_payment_verification_succeeded_event(
    data: PaymentVerificationSucceededData,
) -> Event {
    create_event(CloudEvent::PaymentVerificationSucceeded(data))
}

/// Convenience function to create a payment verification failed event
pub fn create_payment_verification_failed_event(data: PaymentVerificationFailedData) -> Event {
    create_event(CloudEvent::PaymentVerificationFailed(data))
}

/// Convenience function to create a payment settlement succeeded event
pub fn create_payment_settlement_succeeded_event(data: PaymentSettlementSucceededData) -> Event {
    create_event(CloudEvent::PaymentSettlementSucceeded(data))
}

/// Convenience function to create a payment settlement failed event
pub fn create_payment_settlement_failed_event(data: PaymentSettlementFailedData) -> Event {
    create_event(CloudEvent::PaymentSettlementFailed(data))
}

/// Creates a new channel pair for CloudEvents (used by sync code to send events)
pub fn create_event_channel() -> (Sender<Event>, Receiver<Event>) {
    unbounded()
}

/// Default number of events to store for replay
pub const DEFAULT_EVENT_STORE_SIZE: usize = 1000;

/// Broadcaster that forwards CloudEvents to SSE clients and stores for replay
pub struct EventBroadcaster {
    tx: broadcast::Sender<(String, String)>, // (event_id, json)
    store: EventStore,
}

impl EventBroadcaster {
    /// Create a new broadcaster with the given broadcast capacity and store size
    pub fn new(broadcast_capacity: usize) -> Self {
        Self::with_store_size(broadcast_capacity, DEFAULT_EVENT_STORE_SIZE)
    }

    /// Create a new broadcaster with custom store size
    pub fn with_store_size(broadcast_capacity: usize, store_size: usize) -> Self {
        let (tx, _) = broadcast::channel(broadcast_capacity);
        Self {
            tx,
            store: EventStore::new(store_size),
        }
    }

    /// Subscribe to receive events (for SSE clients)
    pub fn subscribe(&self) -> broadcast::Receiver<(String, String)> {
        self.tx.subscribe()
    }

    /// Broadcast an event to all SSE clients and store for replay
    pub fn broadcast(&self, event: &Event) {
        if let Some(json_event) = CloudEventEnvelope::from_sdk_event(event)
            && let Ok(json_str) = serde_json::to_string(&json_event)
        {
            let event_id = json_event.id.clone();
            // Store for replay
            self.store.store(event_id.clone(), json_str.clone());
            // Broadcast to live subscribers (ignore send errors - no subscribers)
            let _ = self.tx.send((event_id, json_str));
        }
    }

    /// Get reference to the event store for replay queries
    pub fn store(&self) -> &EventStore {
        &self.store
    }

    /// Get events from cursor for replay
    pub fn get_replay_events(&self, query: &EventStreamQuery) -> Vec<StoredEvent> {
        match (&query.cursor, query.last) {
            (Some(cursor), _) => self.store.get_from_cursor(cursor),
            (None, Some(n)) => self.store.get_last(n),
            (None, None) => vec![],
        }
    }
}

/// Spawns a dedicated thread that consumes CloudEvents and broadcasts them to SSE clients
pub fn spawn_event_consumer(
    receiver: Receiver<Event>,
    broadcaster: Arc<EventBroadcaster>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        info!("CloudEvent consumer thread started");
        for event in receiver.iter() {
            info!(
                event_id = %event.id(),
                event_type = %event.ty(),
                event_source = %event.source(),
                event_data = ?event.data(),
                "Received CloudEvent"
            );

            // Broadcast to SSE clients
            broadcaster.broadcast(&event);
        }
        info!("CloudEvent consumer thread stopped");
    })
}

/// Create an SSE stream for a client with optional cursor-based replay
///
/// # Query Parameters
/// - `cursor`: Resume from this event ID (exclusive - returns events after this ID)
/// - `last`: Replay the last N events before switching to live
///
/// The stream uses the SSE `id:` field for each event, enabling automatic
/// reconnection with the `Last-Event-ID` header.
pub fn create_sse_stream(
    broadcaster: Arc<EventBroadcaster>,
    query: EventStreamQuery,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    // Get replay events before subscribing to avoid missing events
    let replay_events = broadcaster.get_replay_events(&query);
    let rx = broadcaster.subscribe();

    let stream = async_stream::stream! {
        // First, replay any historical events
        for stored in replay_events {
            yield Ok(SseEvent::default()
                .id(stored.id)
                .event("payment")
                .data(stored.json));
        }

        // Then switch to live events
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok((event_id, data)) => {
                    yield Ok(SseEvent::default()
                        .id(event_id)
                        .event("payment")
                        .data(data));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    error!("SSE client lagged behind by {} messages", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ==================== Stateful Event Stream Support ====================

/// Context for stateful event streams - includes payment stack context
#[derive(Debug, Clone)]
pub struct EventStreamContext {
    pub payment_stack_id: String,
    pub is_sandbox: bool,
}

/// Stateful broadcaster that persists events to the database
/// In addition to in-memory broadcasting, it stores events for DB-backed replay
pub struct StatefulEventBroadcaster {
    inner: EventBroadcaster,
    db_manager: Arc<DbManager>,
    context: EventStreamContext,
}

impl StatefulEventBroadcaster {
    /// Create a new stateful broadcaster with database persistence
    pub fn new(
        broadcast_capacity: usize,
        db_manager: Arc<DbManager>,
        context: EventStreamContext,
    ) -> Self {
        Self {
            inner: EventBroadcaster::new(broadcast_capacity),
            db_manager,
            context,
        }
    }

    /// Subscribe to receive events (for SSE clients)
    pub fn subscribe(&self) -> broadcast::Receiver<(String, String)> {
        self.inner.subscribe()
    }

    /// Broadcast an event to all SSE clients, store in memory AND persist to database
    pub fn broadcast(&self, event: &Event) {
        if let Some(json_event) = CloudEventEnvelope::from_sdk_event(event)
            && let Ok(json_str) = serde_json::to_string(&json_event)
        {
            let event_id = json_event.id.clone();
            let event_type = json_event.ty.clone();
            let event_source = json_event.source.clone();
            let event_time = json_event.time.timestamp_millis();

            info!(
                event_id = %event_id,
                event_type = %event_type,
                "Broadcasting event to SSE clients"
            );

            // Store in memory for stateless clients
            self.inner.store.store(event_id.clone(), json_str.clone());

            // Persist to database for stateful clients
            if let Err(e) = self.db_manager.insert_cloud_event(
                event_id.clone(),
                event_type.clone(),
                event_source,
                event_time,
                json_str.clone(),
                &self.context.payment_stack_id,
                self.context.is_sandbox,
            ) {
                error!("Failed to persist CloudEvent to database: {}", e);
            }

            // Broadcast to live subscribers
            let subscriber_count = self.inner.tx.receiver_count();
            info!(
                event_id = %event_id,
                subscriber_count = %subscriber_count,
                "Sending to broadcast channel"
            );
            match self.inner.tx.send((event_id.clone(), json_str)) {
                Ok(sent_to) => {
                    info!(
                        event_id = %event_id,
                        sent_to = %sent_to,
                        "Event sent to subscribers"
                    );
                }
                Err(e) => {
                    error!(
                        event_id = %event_id,
                        error = %e,
                        "Failed to send to broadcast channel (no subscribers?)"
                    );
                }
            }
        }
    }

    /// Get reference to the inner event store for stateless replay queries
    pub fn store(&self) -> &EventStore {
        &self.inner.store
    }

    /// Get events from cursor for replay (stateless mode - from memory)
    pub fn get_replay_events(&self, query: &EventStreamQuery) -> Vec<StoredEvent> {
        self.inner.get_replay_events(query)
    }

    /// Get events from DB for stateful stream replay
    /// Uses the stream's persisted cursor if no explicit cursor provided
    pub fn get_stateful_replay_events(
        &self,
        stream_id: &str,
        explicit_cursor: Option<&str>,
        limit: i64,
    ) -> Vec<StoredEvent> {
        info!(
            stream_id = %stream_id,
            explicit_cursor = ?explicit_cursor,
            limit = %limit,
            payment_stack_id = %self.context.payment_stack_id,
            is_sandbox = %self.context.is_sandbox,
            "Getting stateful replay events"
        );

        // First, find or create the stream to get its cursor
        let stream_cursor = match self.db_manager.find_or_create_event_stream(
            stream_id,
            &self.context.payment_stack_id,
            self.context.is_sandbox,
        ) {
            Ok(stream) => {
                info!(
                    stream_id = %stream_id,
                    stored_cursor = ?stream.last_event_id,
                    "Found/created stream"
                );
                stream.last_event_id
            }
            Err(e) => {
                error!("Failed to find/create event stream: {}", e);
                None
            }
        };

        // Determine cursor and whether to apply limit:
        // - If explicit_cursor provided: use it with limit (manual replay request)
        // - If stream has stored cursor: use it WITHOUT limit (returning user - get ALL missed events)
        // - If no cursor at all: use limit to get last N events (new stream)
        let (cursor, apply_limit) = if let Some(explicit) = explicit_cursor {
            info!(stream_id = %stream_id, "Using explicit cursor");
            (Some(explicit.to_string()), true)
        } else if let Some(stored) = stream_cursor {
            info!(stream_id = %stream_id, stored_cursor = %stored, "Using stored cursor (returning stream)");
            (Some(stored), false) // Returning stream: get ALL events after cursor
        } else {
            info!(stream_id = %stream_id, "No cursor - new stream, will get last N events");
            (None, true)
        };

        let events = match cursor {
            Some(cursor_id) => {
                // Get events after the cursor from DB
                // For returning streams (apply_limit=false), use a large limit to get all missed events
                let effective_limit = if apply_limit { limit } else { 10000 };
                match self.db_manager.get_events_after_cursor(
                    &cursor_id,
                    &self.context.payment_stack_id,
                    self.context.is_sandbox,
                    effective_limit,
                ) {
                    Ok(events) => events
                        .into_iter()
                        .map(|e| StoredEvent {
                            id: e.event_id,
                            json: e.data_json,
                        })
                        .collect(),
                    Err(e) => {
                        error!("Failed to get events from DB: {}", e);
                        vec![]
                    }
                }
            }
            None => {
                // No cursor - get last N events from DB (new stream with `last` parameter)
                match self.db_manager.get_last_events(
                    &self.context.payment_stack_id,
                    self.context.is_sandbox,
                    limit,
                ) {
                    Ok(events) => events
                        .into_iter()
                        .map(|e| StoredEvent {
                            id: e.event_id,
                            json: e.data_json,
                        })
                        .collect(),
                    Err(e) => {
                        error!("Failed to get last events from DB: {}", e);
                        vec![]
                    }
                }
            }
        };

        info!(
            stream_id = %stream_id,
            event_count = %events.len(),
            "Returning replay events"
        );
        events
    }

    /// Update the cursor for a stateful stream after consuming an event
    pub fn update_stream_cursor(&self, stream_id: &str, event_id: &str, event_time: i64) {
        info!(
            stream_id = %stream_id,
            event_id = %event_id,
            payment_stack_id = %self.context.payment_stack_id,
            is_sandbox = %self.context.is_sandbox,
            "Updating stream cursor"
        );
        match self.db_manager.update_event_stream_cursor(
            stream_id,
            &self.context.payment_stack_id,
            self.context.is_sandbox,
            event_id,
            event_time,
        ) {
            Ok(rows_updated) => {
                if rows_updated == 0 {
                    error!(
                        stream_id = %stream_id,
                        event_id = %event_id,
                        "Cursor update affected 0 rows - stream may not exist"
                    );
                } else {
                    info!(
                        stream_id = %stream_id,
                        event_id = %event_id,
                        rows_updated = %rows_updated,
                        "Stream cursor updated successfully"
                    );
                }
            }
            Err(e) => {
                error!("Failed to update stream cursor: {}", e);
            }
        }
    }

    /// Get the database manager for advanced queries
    pub fn db_manager(&self) -> &Arc<DbManager> {
        &self.db_manager
    }

    /// Get the event stream context
    pub fn context(&self) -> &EventStreamContext {
        &self.context
    }
}

/// Spawns a dedicated thread that consumes CloudEvents and broadcasts them via stateful broadcaster
pub fn spawn_stateful_event_consumer(
    receiver: Receiver<Event>,
    broadcaster: Arc<StatefulEventBroadcaster>,
) -> JoinHandle<()> {
    // Log the broadcaster pointer address for debugging
    let broadcaster_ptr = Arc::as_ptr(&broadcaster) as usize;
    info!(
        broadcaster_ptr = %format!("{:#x}", broadcaster_ptr),
        "Stateful CloudEvent consumer thread starting with broadcaster"
    );

    thread::spawn(move || {
        info!(
            broadcaster_ptr = %format!("{:#x}", broadcaster_ptr),
            "Stateful CloudEvent consumer thread started"
        );
        for event in receiver.iter() {
            info!(
                event_id = %event.id(),
                event_type = %event.ty(),
                broadcaster_ptr = %format!("{:#x}", broadcaster_ptr),
                "Received CloudEvent, broadcasting to SSE clients"
            );

            // Broadcast to SSE clients and persist to DB
            broadcaster.broadcast(&event);
        }
        info!("Stateful CloudEvent consumer thread stopped");
    })
}

/// Create a stateful SSE stream for a client
///
/// # Parameters
/// - `broadcaster`: The stateful event broadcaster
/// - `query`: Stream query parameters
///
/// # Stateful Stream Behavior
/// When `stream_id` is provided:
/// 1. Server tracks this stream's cursor in the database
/// 2. On reconnection, missed events are replayed from the persisted cursor
/// 3. Cursor is updated as events are consumed by the client
///
/// When `stream_id` is not provided:
/// - Falls back to stateless behavior (in-memory only)
pub fn create_stateful_sse_stream(
    broadcaster: Arc<StatefulEventBroadcaster>,
    query: EventStreamQuery,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let is_stateful = query.stream_id.is_some();
    let stream_id = query.stream_id.clone();

    // Log broadcaster pointer for debugging - should match consumer thread
    let broadcaster_ptr = Arc::as_ptr(&broadcaster) as usize;
    info!(
        stream_id = ?stream_id,
        is_stateful = %is_stateful,
        last = ?query.last,
        cursor = ?query.cursor,
        broadcaster_ptr = %format!("{:#x}", broadcaster_ptr),
        "Creating SSE stream"
    );

    // IMPORTANT: Subscribe FIRST, then get replay events.
    // This ensures we don't miss events that arrive between fetching replays and subscribing.
    let rx = broadcaster.subscribe();
    info!("Subscribed to broadcast channel");

    // Now get replay events - any new events that arrive will be queued in rx
    let replay_events = if let Some(ref sid) = stream_id {
        // Stateful: get from DB with cursor tracking
        let limit = query.last.unwrap_or(100) as i64;
        let events = broadcaster.get_stateful_replay_events(sid, query.cursor.as_deref(), limit);
        info!(
            stream_id = %sid,
            replay_count = %events.len(),
            "Got stateful replay events"
        );
        events
    } else {
        // Stateless: get from memory
        broadcaster.get_replay_events(&query)
    };

    let broadcaster_for_cursor = if is_stateful {
        Some(Arc::clone(&broadcaster))
    } else {
        None
    };

    let stream_id_for_log = stream_id.clone();
    let stream = async_stream::stream! {
        info!(stream_id = ?stream_id_for_log, "SSE async stream started");

        // Track the last event ID for cursor update after all replays
        let mut last_replay_event_id: Option<String> = None;

        // First, replay any historical events
        for stored in replay_events {
            let event_id = stored.id.clone();

            // Update cursor for PREVIOUS event (if any) before yielding current
            // This ensures "at-least-once" delivery - cursor only updated after event sent
            if let (Some(sid), Some(b), Some(prev_id)) = (&stream_id, &broadcaster_for_cursor, &last_replay_event_id) {
                info!(
                    stream_id = %sid,
                    prev_event_id = %prev_id,
                    "Updating cursor for previous replay event"
                );
                let event_time = chrono::Utc::now().timestamp_millis();
                b.update_stream_cursor(sid, prev_id, event_time);
            }

            last_replay_event_id = Some(event_id.clone());

            info!(
                stream_id = ?stream_id,
                event_id = %event_id,
                "Yielding replay event"
            );
            yield Ok(SseEvent::default()
                .id(event_id)
                .event("payment")
                .data(stored.json));
        }

        // Update cursor for the LAST replay event before entering live loop
        // This runs on the poll after the last replay event is sent
        if let (Some(sid), Some(b), Some(last_id)) = (&stream_id, &broadcaster_for_cursor, &last_replay_event_id) {
            info!(
                stream_id = %sid,
                last_event_id = %last_id,
                "Updating cursor for last replay event before entering live loop"
            );
            let event_time = chrono::Utc::now().timestamp_millis();
            b.update_stream_cursor(sid, last_id, event_time);
        }

        info!(stream_id = ?stream_id, "Entering live event loop");

        // Then switch to live events
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok((event_id, data)) => {
                    info!(
                        stream_id = ?stream_id,
                        event_id = %event_id,
                        "Received LIVE event from broadcast channel, yielding"
                    );
                    yield Ok(SseEvent::default()
                        .id(event_id.clone())
                        .event("payment")
                        .data(data));

                    // Update cursor AFTER yielding for at-least-once delivery
                    if let (Some(sid), Some(b)) = (&stream_id, &broadcaster_for_cursor) {
                        info!(
                            stream_id = %sid,
                            event_id = %event_id,
                            "Updating cursor for live event"
                        );
                        let event_time = chrono::Utc::now().timestamp_millis();
                        b.update_stream_cursor(sid, &event_id, event_time);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    error!("SSE client lagged behind by {} messages", n);
                    // For stateful streams, we could replay from DB here
                    // For now, just continue
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!(stream_id = ?stream_id, "Broadcast channel closed, ending stream");
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// Create an in-memory database for testing
    fn create_test_db() -> Arc<DbManager> {
        // Use in-memory SQLite for tests
        Arc::new(DbManager::local(":memory:").expect("Failed to create in-memory database"))
    }

    /// Create a test broadcaster with in-memory database
    fn create_test_broadcaster() -> Arc<StatefulEventBroadcaster> {
        let db = create_test_db();
        let context = EventStreamContext {
            payment_stack_id: "test_stack".to_string(),
            is_sandbox: true,
        };
        Arc::new(StatefulEventBroadcaster::new(100, db, context))
    }

    /// Helper to insert test events directly into the database
    /// Uses a prefix to ensure unique event IDs across tests
    fn insert_test_events_with_prefix(
        db: &DbManager,
        prefix: &str,
        count: usize,
        payment_stack_id: &str,
        is_sandbox: bool,
    ) -> Vec<String> {
        let mut event_ids = Vec::new();
        for i in 0..count {
            let event_id = format!("{}_{}", prefix, i);
            let event_time = chrono::Utc::now().timestamp_millis() + (i as i64 * 100);
            db.insert_cloud_event(
                event_id.clone(),
                "payment.settlement.succeeded".to_string(),
                "moneymq".to_string(),
                event_time,
                format!(r#"{{"index": {}}}"#, i),
                payment_stack_id,
                is_sandbox,
            )
            .expect("Failed to insert test event");
            event_ids.push(event_id);
            // Small delay to ensure different created_at timestamps
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        event_ids
    }

    // ==================== Test: New Stream with `last` Parameter ====================
    // A new stream (no stored cursor) should get the last N events from history

    #[test]
    fn test_new_stream_with_last_gets_history() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 20 events with unique prefix
        let event_ids = insert_test_events_with_prefix(db, "new_last_hist", 20, "test_stack", true);

        // Request a new stream with last=5
        let events = broadcaster.get_stateful_replay_events("new_stream_1", None, 5);

        // Should get exactly 5 events (the last 5)
        assert_eq!(
            events.len(),
            5,
            "New stream with last=5 should return 5 events"
        );

        // Events should be the last 5 in chronological order
        assert_eq!(
            events[0].id, event_ids[15],
            "First event should be event_15"
        );
        assert_eq!(events[4].id, event_ids[19], "Last event should be event_19");
    }

    #[test]
    fn test_new_stream_with_last_more_than_available() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert only 3 events with unique prefix
        let event_ids = insert_test_events_with_prefix(db, "new_more", 3, "test_stack", true);

        // Request last=10
        let events = broadcaster.get_stateful_replay_events("new_stream_2", None, 10);

        // Should get all 3 events
        assert_eq!(
            events.len(),
            3,
            "Should return all available events when less than requested"
        );
        assert_eq!(events[0].id, event_ids[0]);
        assert_eq!(events[2].id, event_ids[2]);
    }

    #[test]
    fn test_new_stream_with_zero_events() {
        let broadcaster = create_test_broadcaster();

        // No events in database, request last=10
        let events = broadcaster.get_stateful_replay_events("new_stream_3", None, 10);

        // Should get 0 events
        assert_eq!(
            events.len(),
            0,
            "New stream with no events should return empty"
        );
    }

    // ==================== Test: Returning Stream Behavior ====================
    // A returning stream (has stored cursor) should ONLY get missed events, ignoring `last`

    #[test]
    fn test_returning_stream_no_new_events() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 10 events with unique prefix
        let event_ids = insert_test_events_with_prefix(db, "ret_no_new", 10, "test_stack", true);

        // Simulate first connection - create stream by calling get_stateful_replay_events
        let stream_id = "returning_stream_1";
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 10);

        // Update cursor to last event (simulating client consumed all events)
        let last_event_id = &event_ids[9];
        let event_time = chrono::Utc::now().timestamp_millis();
        broadcaster.update_stream_cursor(stream_id, last_event_id, event_time);

        // Simulate reconnection with last=5 - should get 0 events since cursor is at the end
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 5);

        assert_eq!(
            events.len(),
            0,
            "Returning stream at end should get 0 events, not last 5"
        );
    }

    #[test]
    fn test_returning_stream_with_missed_events() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert first batch of 5 events with unique prefix
        let first_batch = insert_test_events_with_prefix(db, "ret_missed_1", 5, "test_stack", true);

        // Simulate first connection - create stream and consume all events
        let stream_id = "returning_stream_2";
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 5);

        // Set cursor to event_4 (last of first batch)
        let cursor_event = &first_batch[4];
        let event_time = chrono::Utc::now().timestamp_millis();
        broadcaster.update_stream_cursor(stream_id, cursor_event, event_time);

        // Insert more events while client is disconnected (with different prefix)
        let second_batch =
            insert_test_events_with_prefix(db, "ret_missed_2", 15, "test_stack", true);

        // Reconnect with last=3 - should get ALL 15 missed events, not just 3
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 3);

        assert_eq!(
            events.len(),
            15,
            "Returning stream should get ALL missed events, ignoring last param"
        );
        assert_eq!(
            events[0].id, second_batch[0],
            "Should start from first missed event"
        );
        assert_eq!(
            events[14].id, second_batch[14],
            "Should include all missed events"
        );
    }

    #[test]
    fn test_returning_stream_cursor_persists_across_calls() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 10 events with unique prefix
        let event_ids = insert_test_events_with_prefix(db, "ret_persist", 10, "test_stack", true);

        // First connection - get last 10
        let stream_id = "returning_stream_3";
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(events.len(), 10, "First connection should get 10 events");

        // Manually update cursor to event_4 (simulating client consumed events 0-4)
        broadcaster.update_stream_cursor(
            stream_id,
            &event_ids[4],
            chrono::Utc::now().timestamp_millis(),
        );

        // Reconnect - should get events 5-9 (5 events)
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(events.len(), 5, "Should get only events after cursor");
        assert_eq!(events[0].id, event_ids[5], "Should start after cursor");
        assert_eq!(events[4].id, event_ids[9], "Should end at last event");
    }

    // ==================== Test: Explicit Cursor Behavior ====================
    // Explicit cursor should override stored cursor and apply limit

    #[test]
    fn test_explicit_cursor_overrides_stored() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 20 events with unique prefix
        let event_ids = insert_test_events_with_prefix(db, "exp_override", 20, "test_stack", true);

        // Create stream first
        let stream_id = "explicit_cursor_stream";
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 20);

        // Set stream cursor to event_15
        broadcaster.update_stream_cursor(
            stream_id,
            &event_ids[15],
            chrono::Utc::now().timestamp_millis(),
        );

        // Request with explicit cursor at event_5 and limit=3
        let events = broadcaster.get_stateful_replay_events(stream_id, Some(&event_ids[5]), 3);

        // Should get 3 events starting after event_5, not after event_15
        assert_eq!(events.len(), 3, "Explicit cursor should respect limit");
        assert_eq!(
            events[0].id, event_ids[6],
            "Should start after explicit cursor"
        );
        assert_eq!(events[2].id, event_ids[8], "Should get 3 events");
    }

    #[test]
    fn test_explicit_cursor_with_limit() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 20 events with unique prefix
        let event_ids = insert_test_events_with_prefix(db, "exp_limit", 20, "test_stack", true);

        // Request with explicit cursor at event_10 and limit=5
        let events =
            broadcaster.get_stateful_replay_events("new_stream_explicit", Some(&event_ids[10]), 5);

        // Should get 5 events after event_10
        assert_eq!(events.len(), 5, "Explicit cursor should apply limit");
        assert_eq!(events[0].id, event_ids[11], "Should start after cursor");
        assert_eq!(events[4].id, event_ids[15], "Should stop at limit");
    }

    // ==================== Test: Cursor Update ====================

    #[test]
    fn test_cursor_update_persisted_to_db() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        let stream_id = "cursor_persist_stream";
        let event_id = "test_event_123";
        let event_time = chrono::Utc::now().timestamp_millis();

        // First create the stream by calling get_stateful_replay_events
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 0);

        // Update cursor
        broadcaster.update_stream_cursor(stream_id, event_id, event_time);

        // Verify cursor is persisted
        let stream = db
            .find_event_stream(stream_id, "test_stack", true)
            .expect("DB query failed")
            .expect("Stream should exist");

        assert_eq!(stream.last_event_id, Some(event_id.to_string()));
        assert_eq!(stream.last_event_time, Some(event_time));
    }

    #[test]
    fn test_new_stream_creates_db_record() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Query for a new stream (triggers find_or_create)
        let stream_id = "brand_new_stream";
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 10);

        // Verify stream was created in DB
        let stream = db
            .find_event_stream(stream_id, "test_stack", true)
            .expect("DB query failed");

        assert!(stream.is_some(), "New stream should be created in DB");
        let stream = stream.unwrap();
        assert_eq!(stream.stream_id, stream_id);
        assert_eq!(
            stream.last_event_id, None,
            "New stream should have no cursor"
        );
    }

    // ==================== Test: Isolation Between Streams ====================

    #[test]
    fn test_streams_isolated_by_id() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 10 events with unique prefix
        let event_ids = insert_test_events_with_prefix(db, "iso_id", 10, "test_stack", true);

        // Stream A: first consume all events, then update cursor
        let stream_a = "stream_a";
        let _ = broadcaster.get_stateful_replay_events(stream_a, None, 10);
        broadcaster.update_stream_cursor(
            stream_a,
            &event_ids[9],
            chrono::Utc::now().timestamp_millis(),
        );

        // Stream B is new
        let stream_b = "stream_b";

        // Stream A should get 0 events (cursor is at the end)
        let events_a = broadcaster.get_stateful_replay_events(stream_a, None, 10);
        assert_eq!(
            events_a.len(),
            0,
            "Stream A should have no events to replay"
        );

        // Stream B should get all 10 events
        let events_b = broadcaster.get_stateful_replay_events(stream_b, None, 10);
        assert_eq!(events_b.len(), 10, "Stream B should get all events");
    }

    #[test]
    fn test_streams_isolated_by_payment_stack() {
        let db = create_test_db();

        // Create two broadcasters with different payment_stack_ids
        let context_a = EventStreamContext {
            payment_stack_id: "stack_a".to_string(),
            is_sandbox: true,
        };
        let context_b = EventStreamContext {
            payment_stack_id: "stack_b".to_string(),
            is_sandbox: true,
        };
        let broadcaster_a = Arc::new(StatefulEventBroadcaster::new(
            100,
            Arc::clone(&db),
            context_a,
        ));
        let broadcaster_b = Arc::new(StatefulEventBroadcaster::new(
            100,
            Arc::clone(&db),
            context_b,
        ));

        // Insert events for stack_a with unique prefix
        insert_test_events_with_prefix(&db, "iso_stack_a", 5, "stack_a", true);

        // Insert events for stack_b with unique prefix
        insert_test_events_with_prefix(&db, "iso_stack_b", 10, "stack_b", true);

        // Stream in stack_a should only see 5 events
        let events_a = broadcaster_a.get_stateful_replay_events("shared_stream_id", None, 20);
        assert_eq!(events_a.len(), 5, "Stack A should only see its 5 events");

        // Stream in stack_b should only see 10 events
        let events_b = broadcaster_b.get_stateful_replay_events("shared_stream_id", None, 20);
        assert_eq!(events_b.len(), 10, "Stack B should only see its 10 events");
    }

    #[test]
    fn test_streams_isolated_by_sandbox_flag() {
        let db = create_test_db();

        // Create two broadcasters with different is_sandbox flags
        let context_sandbox = EventStreamContext {
            payment_stack_id: "shared_stack".to_string(),
            is_sandbox: true,
        };
        let context_prod = EventStreamContext {
            payment_stack_id: "shared_stack".to_string(),
            is_sandbox: false,
        };
        let broadcaster_sandbox = Arc::new(StatefulEventBroadcaster::new(
            100,
            Arc::clone(&db),
            context_sandbox,
        ));
        let broadcaster_prod = Arc::new(StatefulEventBroadcaster::new(
            100,
            Arc::clone(&db),
            context_prod,
        ));

        // Insert events for sandbox with unique prefix
        insert_test_events_with_prefix(&db, "iso_sandbox", 7, "shared_stack", true);

        // Insert events for prod with unique prefix
        insert_test_events_with_prefix(&db, "iso_prod", 3, "shared_stack", false);

        // Sandbox stream should see 7 events
        let events_sandbox = broadcaster_sandbox.get_stateful_replay_events("stream", None, 20);
        assert_eq!(events_sandbox.len(), 7, "Sandbox should see its 7 events");

        // Prod stream should see 3 events
        let events_prod = broadcaster_prod.get_stateful_replay_events("stream", None, 20);
        assert_eq!(events_prod.len(), 3, "Prod should see its 3 events");
    }

    // ==================== Test: Edge Cases ====================

    #[test]
    fn test_cursor_at_deleted_event() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert events with unique prefix
        insert_test_events_with_prefix(db, "orphan", 5, "test_stack", true);

        // Create stream first
        let stream_id = "orphan_cursor_stream";
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 0);

        // Set cursor to a fake event that doesn't exist
        broadcaster.update_stream_cursor(
            stream_id,
            "nonexistent_event",
            chrono::Utc::now().timestamp_millis(),
        );

        // Should return empty since cursor event doesn't exist
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(
            events.len(),
            0,
            "Should return empty when cursor event doesn't exist"
        );
    }

    #[test]
    fn test_large_missed_event_count() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert first batch with unique prefix
        let first_batch = insert_test_events_with_prefix(db, "large_1", 2, "test_stack", true);

        // Create stream and set cursor to last of first batch
        let stream_id = "large_batch_stream";
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 2);
        broadcaster.update_stream_cursor(
            stream_id,
            &first_batch[1],
            chrono::Utc::now().timestamp_millis(),
        );

        // Insert 100 more events while disconnected (with different prefix)
        let second_batch = insert_test_events_with_prefix(db, "large_2", 100, "test_stack", true);

        // Reconnect with last=5 - should get ALL 100 missed events
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 5);

        assert_eq!(events.len(), 100, "Should get all 100 missed events");
        assert_eq!(events[0].id, second_batch[0]);
        assert_eq!(events[99].id, second_batch[99]);
    }

    // ==================== Bug Fix Tests ====================
    // These tests verify the specific bugs we fixed

    #[test]
    fn test_cursor_update_returns_row_count() {
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // First create the stream
        let stream_id = "row_count_stream";
        let _ = broadcaster.get_stateful_replay_events(stream_id, None, 0);

        // Update cursor - should return 1 (one row updated)
        let result = db.update_event_stream_cursor(
            stream_id,
            "test_stack",
            true,
            "test_event",
            chrono::Utc::now().timestamp_millis(),
        );

        assert!(result.is_ok(), "Update should succeed");
        assert_eq!(result.unwrap(), 1, "Should update exactly 1 row");
    }

    #[test]
    fn test_cursor_update_on_nonexistent_stream_returns_zero() {
        let db = create_test_db();

        // Try to update cursor for a stream that doesn't exist
        let result = db.update_event_stream_cursor(
            "nonexistent_stream",
            "test_stack",
            true,
            "test_event",
            chrono::Utc::now().timestamp_millis(),
        );

        assert!(result.is_ok(), "Update should not error");
        assert_eq!(
            result.unwrap(),
            0,
            "Should update 0 rows for nonexistent stream"
        );
    }

    #[test]
    fn test_full_replay_consume_reconnect_cycle() {
        // This test simulates the full user flow:
        // 1. Payment is made (event stored in DB)
        // 2. User starts stream, gets replay
        // 3. Cursor is updated after consuming
        // 4. User refreshes, reconnects with same stream ID
        // 5. Should get 0 events (all consumed)

        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Step 1: Insert payment event
        let event_ids = insert_test_events_with_prefix(db, "full_cycle", 1, "test_stack", true);

        // Step 2: Start stream, get replay
        let stream_id = "user_dashboard";
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(events.len(), 1, "First connection should get 1 event");
        assert_eq!(events[0].id, event_ids[0]);

        // Step 3: Update cursor (simulating consuming the event)
        broadcaster.update_stream_cursor(
            stream_id,
            &event_ids[0],
            chrono::Utc::now().timestamp_millis(),
        );

        // Verify cursor was persisted
        let stream = db
            .find_event_stream(stream_id, "test_stack", true)
            .expect("DB query failed")
            .expect("Stream should exist");
        assert_eq!(
            stream.last_event_id,
            Some(event_ids[0].clone()),
            "Cursor should be set to consumed event"
        );

        // Step 4: User refreshes, reconnects
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 10);

        // Step 5: Should get 0 events
        assert_eq!(
            events.len(),
            0,
            "Reconnection should get 0 events - all consumed"
        );
    }

    #[test]
    fn test_consume_some_then_reconnect_gets_remaining() {
        // User consumes some events, disconnects, more events arrive, reconnects
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 5 events
        let batch1 = insert_test_events_with_prefix(db, "partial_1", 5, "test_stack", true);

        // Start stream, get all 5
        let stream_id = "partial_consumer";
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(events.len(), 5);

        // Consume only first 3 events (update cursor to event 2, 0-indexed)
        broadcaster.update_stream_cursor(
            stream_id,
            &batch1[2],
            chrono::Utc::now().timestamp_millis(),
        );

        // User disconnects, more events arrive
        let batch2 = insert_test_events_with_prefix(db, "partial_2", 3, "test_stack", true);

        // Reconnect - should get events 3, 4 from batch1 + all 3 from batch2 = 5 events
        let events = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(
            events.len(),
            5,
            "Should get remaining events: 2 from batch1 + 3 from batch2"
        );

        // Verify order: batch1[3], batch1[4], batch2[0], batch2[1], batch2[2]
        assert_eq!(events[0].id, batch1[3], "First should be batch1[3]");
        assert_eq!(events[1].id, batch1[4], "Second should be batch1[4]");
        assert_eq!(events[2].id, batch2[0], "Third should be batch2[0]");
    }

    #[test]
    fn test_broadcast_subscription_receives_live_events() {
        // Test that subscribing to broadcast channel receives events
        let broadcaster = create_test_broadcaster();

        // Subscribe before any events
        let mut rx = broadcaster.subscribe();

        // Broadcast an event using the CloudEvent API
        let event = cloudevents::EventBuilderV10::new()
            .id("live_test_event_1")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"test": true}"#)
            .build()
            .expect("Failed to build event");

        broadcaster.broadcast(&event);

        // Try to receive (non-blocking)
        match rx.try_recv() {
            Ok((event_id, data)) => {
                assert_eq!(event_id, "live_test_event_1");
                assert!(data.contains("test"));
            }
            Err(e) => {
                panic!("Should have received the broadcast event: {:?}", e);
            }
        }
    }

    #[test]
    fn test_subscription_after_broadcast_misses_event() {
        // Verify that subscribing AFTER broadcast misses the event
        // This is the bug we fixed by subscribing before fetching replays
        let broadcaster = create_test_broadcaster();

        // Broadcast an event BEFORE subscribing
        let event = cloudevents::EventBuilderV10::new()
            .id("missed_event_1")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"test": true}"#)
            .build()
            .expect("Failed to build event");

        broadcaster.broadcast(&event);

        // Now subscribe - should NOT receive the event that was already broadcast
        let mut rx = broadcaster.subscribe();

        // Try to receive (non-blocking) - should be empty
        match rx.try_recv() {
            Ok(_) => {
                panic!("Should NOT have received the event - it was broadcast before subscription");
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                // This is expected - the event was broadcast before we subscribed
            }
            Err(e) => {
                panic!("Unexpected error: {:?}", e);
            }
        }
    }

    #[test]
    fn test_multiple_streams_same_events_different_cursors() {
        // Two different stream IDs should have independent cursors
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert 10 events
        let events = insert_test_events_with_prefix(db, "multi_stream", 10, "test_stack", true);

        // Stream A: consumes all 10, cursor at event 9
        let stream_a = "stream_alpha";
        let _ = broadcaster.get_stateful_replay_events(stream_a, None, 10);
        broadcaster.update_stream_cursor(
            stream_a,
            &events[9],
            chrono::Utc::now().timestamp_millis(),
        );

        // Stream B: consumes only 5, cursor at event 4
        let stream_b = "stream_beta";
        let _ = broadcaster.get_stateful_replay_events(stream_b, None, 10);
        broadcaster.update_stream_cursor(
            stream_b,
            &events[4],
            chrono::Utc::now().timestamp_millis(),
        );

        // Insert 2 more events
        let more_events =
            insert_test_events_with_prefix(db, "multi_stream_2", 2, "test_stack", true);

        // Reconnect Stream A - should get only the 2 new events
        let replay_a = broadcaster.get_stateful_replay_events(stream_a, None, 10);
        assert_eq!(replay_a.len(), 2, "Stream A should get 2 new events");
        assert_eq!(replay_a[0].id, more_events[0]);

        // Reconnect Stream B - should get events 5-9 (5 events) + 2 new = 7 events
        let replay_b = broadcaster.get_stateful_replay_events(stream_b, None, 10);
        assert_eq!(
            replay_b.len(),
            7,
            "Stream B should get 5 missed + 2 new = 7 events"
        );
        assert_eq!(replay_b[0].id, events[5], "First should be event 5");
    }

    // ==================== Full SSE Flow Tests ====================
    // These tests simulate the actual SSE flow with subscribers

    #[test]
    fn test_subscribe_then_replay_then_live_event() {
        // This simulates the exact SSE flow:
        // 1. Subscribe to broadcast channel FIRST
        // 2. Get replay events from DB
        // 3. New event arrives via broadcast
        // 4. Subscriber should receive it

        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        // Insert some historical events
        insert_test_events_with_prefix(db, "sse_flow", 3, "test_stack", true);

        // Step 1: Subscribe FIRST (like SSE does)
        let mut rx = broadcaster.subscribe();

        // Step 2: Get replay events (like SSE does)
        let stream_id = "sse_flow_stream";
        let replay = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(replay.len(), 3, "Should get 3 replay events");

        // Step 3: New event arrives while we're "connected"
        let live_event = cloudevents::EventBuilderV10::new()
            .id("live_event_after_replay")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"live": true}"#)
            .build()
            .expect("Failed to build event");

        broadcaster.broadcast(&live_event);

        // Step 4: Subscriber should receive it
        match rx.try_recv() {
            Ok((event_id, _data)) => {
                assert_eq!(
                    event_id, "live_event_after_replay",
                    "Should receive the live event"
                );
            }
            Err(e) => {
                panic!(
                    "BUG: Subscriber did not receive live event after replay! Error: {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_multiple_subscribers_all_receive_live_events() {
        // Test that multiple subscribers all receive the same event
        let broadcaster = create_test_broadcaster();

        // Create 3 subscribers
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();
        let mut rx3 = broadcaster.subscribe();

        // Broadcast an event
        let event = cloudevents::EventBuilderV10::new()
            .id("multi_sub_event")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"test": true}"#)
            .build()
            .expect("Failed to build event");

        broadcaster.broadcast(&event);

        // All 3 subscribers should receive it
        for (i, rx) in [&mut rx1, &mut rx2, &mut rx3].iter_mut().enumerate() {
            match rx.try_recv() {
                Ok((event_id, _)) => {
                    assert_eq!(event_id, "multi_sub_event");
                }
                Err(e) => {
                    panic!("Subscriber {} did not receive event: {:?}", i + 1, e);
                }
            }
        }
    }

    #[test]
    fn test_subscriber_count_after_subscribe() {
        // Verify subscriber count is correctly tracked
        let broadcaster = create_test_broadcaster();

        // Initially no subscribers
        assert_eq!(
            broadcaster.inner.tx.receiver_count(),
            0,
            "Should have 0 subscribers initially"
        );

        // Subscribe
        let _rx1 = broadcaster.subscribe();
        assert_eq!(
            broadcaster.inner.tx.receiver_count(),
            1,
            "Should have 1 subscriber"
        );

        let _rx2 = broadcaster.subscribe();
        assert_eq!(
            broadcaster.inner.tx.receiver_count(),
            2,
            "Should have 2 subscribers"
        );

        // Drop one subscriber
        drop(_rx1);
        assert_eq!(
            broadcaster.inner.tx.receiver_count(),
            1,
            "Should have 1 subscriber after drop"
        );
    }

    #[test]
    fn test_live_event_persisted_to_db_and_broadcast() {
        // Verify that broadcast() both persists to DB and sends to subscribers
        let broadcaster = create_test_broadcaster();
        let db = broadcaster.db_manager();

        let mut rx = broadcaster.subscribe();

        let event = cloudevents::EventBuilderV10::new()
            .id("persist_and_broadcast_event")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"amount": 1000}"#)
            .build()
            .expect("Failed to build event");

        broadcaster.broadcast(&event);

        // Should be in broadcast channel
        let (event_id, _) = rx.try_recv().expect("Should receive broadcast");
        assert_eq!(event_id, "persist_and_broadcast_event");

        // Should also be persisted to DB - use get_last_events instead
        let db_events = db
            .get_last_events("test_stack", true, 10)
            .expect("DB query failed");

        assert_eq!(db_events.len(), 1, "Should have 1 event in DB");
        assert_eq!(db_events[0].event_id, "persist_and_broadcast_event");
    }

    #[tokio::test]
    async fn test_broadcast_receiver_in_async_context() {
        // Test that broadcast receiver works correctly in an async context
        // This simulates what happens inside the SSE stream
        let broadcaster = create_test_broadcaster();

        // Subscribe (like SSE stream does)
        let mut rx = broadcaster.subscribe();

        // Broadcast an event
        let event = cloudevents::EventBuilderV10::new()
            .id("async_context_event")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"async": true}"#)
            .build()
            .expect("Failed to build event");

        broadcaster.broadcast(&event);

        // Receive in async context using rx.recv().await
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;

        match result {
            Ok(Ok((event_id, _))) => {
                assert_eq!(event_id, "async_context_event");
            }
            Ok(Err(e)) => {
                panic!("Recv error: {:?}", e);
            }
            Err(_) => {
                panic!("Timeout waiting for event in async context");
            }
        }
    }

    #[test]
    fn test_sse_stream_simulated_flow() {
        // Simulate the SSE stream flow step by step
        // This is what happens inside create_stateful_sse_stream

        let broadcaster = Arc::new({
            let db = create_test_db();
            let context = EventStreamContext {
                payment_stack_id: "sim_stack".to_string(),
                is_sandbox: true,
            };
            StatefulEventBroadcaster::new(100, db, context)
        });

        let db = broadcaster.db_manager();

        // Step 1: Insert historical events (before stream starts)
        db.insert_cloud_event(
            "sim_hist_1".to_string(),
            "payment.settlement.succeeded".to_string(),
            "moneymq".to_string(),
            chrono::Utc::now().timestamp_millis(),
            r#"{"id":"sim_hist_1"}"#.to_string(),
            "sim_stack",
            true,
        )
        .expect("Failed to insert event");

        db.insert_cloud_event(
            "sim_hist_2".to_string(),
            "payment.settlement.succeeded".to_string(),
            "moneymq".to_string(),
            chrono::Utc::now().timestamp_millis() + 100,
            r#"{"id":"sim_hist_2"}"#.to_string(),
            "sim_stack",
            true,
        )
        .expect("Failed to insert event");

        // Step 2: Stream starts - SUBSCRIBE FIRST (critical!)
        let mut rx = broadcaster.subscribe();

        // Step 3: Get replay events
        let replay = broadcaster.get_stateful_replay_events("sim_stream", None, 10);
        assert_eq!(replay.len(), 2, "Should get 2 historical events");
        assert_eq!(replay[0].id, "sim_hist_1");
        assert_eq!(replay[1].id, "sim_hist_2");

        // Step 4: A new event arrives (payment happens)
        let live_event = cloudevents::EventBuilderV10::new()
            .id("sim_live_1")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"live": true}"#)
            .build()
            .expect("Failed to build event");

        broadcaster.broadcast(&live_event);

        // Step 5: SSE stream should receive the live event
        match rx.try_recv() {
            Ok((event_id, _)) => {
                assert_eq!(event_id, "sim_live_1", "Should receive live event");
            }
            Err(e) => {
                panic!(
                    "BUG: Live event not received! Error: {:?}\nThis is the exact bug we're trying to catch.",
                    e
                );
            }
        }

        // Step 6: Update cursor (as SSE stream would do after yielding)
        broadcaster.update_stream_cursor(
            "sim_stream",
            "sim_live_1",
            chrono::Utc::now().timestamp_millis(),
        );

        // Step 7: Verify cursor was updated
        let stream = db
            .find_event_stream("sim_stream", "sim_stack", true)
            .expect("DB query failed")
            .expect("Stream should exist");
        assert_eq!(
            stream.last_event_id,
            Some("sim_live_1".to_string()),
            "Cursor should be at live event"
        );

        // Step 8: Simulate reconnect - should get 0 events
        // Drop rx to simulate disconnect
        drop(rx);

        // New connection
        let _rx2 = broadcaster.subscribe();
        let replay2 = broadcaster.get_stateful_replay_events("sim_stream", None, 10);
        assert_eq!(
            replay2.len(),
            0,
            "After cursor update, should get 0 replay events"
        );
    }

    #[test]
    fn test_full_production_flow_with_event_channel() {
        // This test matches the EXACT production flow:
        // 1. Create broadcaster
        // 2. Create event channel
        // 3. Spawn event consumer thread (receives from channel, broadcasts to SSE)
        // 4. Make a payment (send to channel) - no SSE subscribers yet
        // 5. SSE client connects (subscribes to broadcaster)
        // 6. Make another payment (send to channel)
        // 7. SSE client should receive the live event

        use std::time::Duration;

        let broadcaster = Arc::new({
            let db = create_test_db();
            let context = EventStreamContext {
                payment_stack_id: "prod_flow_stack".to_string(),
                is_sandbox: true,
            };
            StatefulEventBroadcaster::new(100, db, context)
        });

        // Create event channel (like production)
        let (tx, rx) = create_event_channel();

        // Spawn consumer thread (like production)
        let _handle = spawn_stateful_event_consumer(rx, Arc::clone(&broadcaster));

        // Give thread time to start
        std::thread::sleep(Duration::from_millis(50));

        // Step 4: First payment (before SSE connects) - goes to channel
        let event1 = cloudevents::EventBuilderV10::new()
            .id("prod_flow_event_1")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"first": true}"#)
            .build()
            .expect("Failed to build event");

        tx.send(event1).expect("Failed to send event to channel");

        // Give consumer thread time to process
        std::thread::sleep(Duration::from_millis(50));

        // Step 5: SSE client connects
        let mut sse_rx = broadcaster.subscribe();

        // Get replay (should see event1 from DB)
        let replay = broadcaster.get_stateful_replay_events("prod_flow_stream", None, 10);
        assert_eq!(replay.len(), 1, "Should have 1 event from replay");
        assert_eq!(replay[0].id, "prod_flow_event_1");

        // Step 6: Second payment (after SSE connects) - should be received live
        let event2 = cloudevents::EventBuilderV10::new()
            .id("prod_flow_event_2")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"second": true}"#)
            .build()
            .expect("Failed to build event");

        tx.send(event2).expect("Failed to send event to channel");

        // Give consumer thread time to process and broadcast
        std::thread::sleep(Duration::from_millis(100));

        // Step 7: SSE should receive the live event
        match sse_rx.try_recv() {
            Ok((event_id, _)) => {
                assert_eq!(
                    event_id, "prod_flow_event_2",
                    "Should receive the second event live"
                );
            }
            Err(e) => {
                panic!(
                    "BUG: Live event NOT received via channel->consumer->broadcast flow!\n\
                     This is the EXACT bug the user is experiencing.\n\
                     Error: {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_cursor_update_in_production_flow() {
        // Test that cursor updates work in the full flow:
        // 1. Payment made, event stored
        // 2. Stream connects, gets replay, cursor updated
        // 3. Disconnect and reconnect - should get 0 events

        use std::time::Duration;

        let broadcaster = Arc::new({
            let db = create_test_db();
            let context = EventStreamContext {
                payment_stack_id: "cursor_prod_stack".to_string(),
                is_sandbox: true,
            };
            StatefulEventBroadcaster::new(100, db, context)
        });

        let (tx, rx) = create_event_channel();
        let _handle = spawn_stateful_event_consumer(rx, Arc::clone(&broadcaster));
        std::thread::sleep(Duration::from_millis(50));

        // Payment 1
        let event1 = cloudevents::EventBuilderV10::new()
            .id("cursor_prod_event_1")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"num": 1}"#)
            .build()
            .expect("Failed to build event");
        tx.send(event1).expect("Failed to send");
        std::thread::sleep(Duration::from_millis(50));

        // Payment 2
        let event2 = cloudevents::EventBuilderV10::new()
            .id("cursor_prod_event_2")
            .ty("payment.settlement.succeeded")
            .source("moneymq")
            .data("application/json", r#"{"num": 2}"#)
            .build()
            .expect("Failed to build event");
        tx.send(event2).expect("Failed to send");
        std::thread::sleep(Duration::from_millis(50));

        // First connection
        let stream_id = "cursor_prod_stream";
        let _rx1 = broadcaster.subscribe();
        let replay1 = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(replay1.len(), 2, "First connection should get 2 events");

        // Simulate consuming both events (update cursor to last event)
        broadcaster.update_stream_cursor(
            stream_id,
            "cursor_prod_event_2",
            chrono::Utc::now().timestamp_millis(),
        );

        // Verify cursor is set
        let db = broadcaster.db_manager();
        let stream = db
            .find_event_stream(stream_id, "cursor_prod_stack", true)
            .expect("DB query failed")
            .expect("Stream should exist");
        assert_eq!(
            stream.last_event_id,
            Some("cursor_prod_event_2".to_string())
        );

        // Reconnect - should get 0 events (all consumed)
        let _rx2 = broadcaster.subscribe();
        let replay2 = broadcaster.get_stateful_replay_events(stream_id, None, 10);
        assert_eq!(
            replay2.len(),
            0,
            "Second connection should get 0 events - all consumed"
        );
    }
}
