use std::sync::Arc;

use futures::StreamExt;
use parking_lot::RwLock;
use reqwest_eventsource::{Event as SseEvent, EventSource};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::{
    error::{PaymentStreamError, Result},
    types::{ChannelConfig, ChannelEvent, ConnectionState},
};

/// Payment hook - bidirectional channel participant for fulfilling payments
///
/// A PaymentHook connects to a transaction's channel to receive events
/// and attach fulfillment data. It can both receive events via SSE and
/// publish attachments via HTTP POST.
///
/// Hooks require authentication (secret key) to attach data.
///
/// # Example
///
/// ```ignore
/// use moneymq_sdk::{PaymentHook, ChannelConfig};
///
/// let config = ChannelConfig::new("https://api.example.com")
///     .with_secret("your-secret-key")
///     .with_actor_id("my-processor")
///     .with_replay(10);
///
/// let mut hook = PaymentHook::new("order-123", config);
///
/// // Subscribe to events
/// let mut rx = hook.subscribe();
///
/// // Connect
/// hook.connect().await?;
///
/// // Attach data with a key - server creates JWT and emits transaction:completed
/// // The key identifies which hook attachment this fulfills
/// // Attachments are stored as: attachments[actor_id][key] = data
/// hook.attach("fulfillment", serde_json::json!({
///     "order_id": "order-123",
///     "status": "shipped"
/// })).await?;
/// ```
pub struct PaymentHook {
    /// Channel ID
    channel_id: String,

    /// Configuration
    config: ChannelConfig,

    /// Connection state
    state: Arc<RwLock<ConnectionState>>,

    /// Event broadcaster for distributing events to multiple subscribers
    event_tx: broadcast::Sender<ChannelEvent>,

    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,

    /// Reconnection attempt counter
    reconnect_attempts: Arc<RwLock<u32>>,

    /// HTTP client for publishing
    http_client: reqwest::Client,
}

impl PaymentHook {
    /// Create a new event actor for the given channel
    pub fn new(channel_id: impl Into<String>, config: ChannelConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            channel_id: channel_id.into(),
            config,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            event_tx,
            shutdown_tx: None,
            reconnect_attempts: Arc::new(RwLock::new(0)),
            http_client: reqwest::Client::new(),
        }
    }

    /// Get the channel ID
    pub fn channel_id(&self) -> &str {
        &self.channel_id
    }

    /// Get the current connection state
    pub fn state(&self) -> ConnectionState {
        *self.state.read()
    }

    /// Subscribe to events from this actor
    ///
    /// Returns a receiver that will receive all events.
    /// Multiple subscribers can be created.
    pub fn subscribe(&self) -> broadcast::Receiver<ChannelEvent> {
        self.event_tx.subscribe()
    }

    /// Build the SSE URL for this channel (with token in query param for auth)
    fn build_sse_url(&self) -> String {
        let mut url = format!(
            "{}/payment/v1/channels/{}",
            self.config.endpoint.trim_end_matches('/'),
            self.channel_id
        );

        let mut params = Vec::new();

        // Token in query param because EventSource cannot set headers
        if let Some(ref secret) = self.config.secret {
            params.push(format!("token={}", secret));
        }

        if let Some(replay) = self.config.replay {
            params.push(format!("replay={}", replay));
        }

        if let Some(ref stream_id) = self.config.stream_id {
            params.push(format!("stream_id={}", stream_id));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        url
    }

    /// Build the attachments URL for this channel
    fn build_attachments_url(&self) -> String {
        format!(
            "{}/payment/v1/channels/{}/attachments",
            self.config.endpoint.trim_end_matches('/'),
            self.channel_id
        )
    }

    /// Connect to the channel and start receiving events
    pub async fn connect(&mut self) -> Result<()> {
        if self.state() == ConnectionState::Connected {
            return Ok(());
        }

        self.set_state(ConnectionState::Connecting);

        let url = self.build_sse_url();
        info!(channel_id = %self.channel_id, url = %url, "Connecting hook to channel");

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Clone what we need for the task
        let event_tx = self.event_tx.clone();
        let state = Arc::clone(&self.state);
        let reconnect_attempts = Arc::clone(&self.reconnect_attempts);
        let config = self.config.clone();
        let channel_id = self.channel_id.clone();

        // Spawn connection task
        tokio::spawn(async move {
            Self::run_connection(
                url,
                channel_id,
                event_tx,
                state,
                reconnect_attempts,
                config,
                shutdown_rx,
            )
            .await;
        });

        Ok(())
    }

    /// Disconnect from the channel
    pub async fn disconnect(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self.set_state(ConnectionState::Disconnected);
        info!(channel_id = %self.channel_id, "Hook disconnected from channel");
    }

    /// Attach data to the transaction channel
    ///
    /// This sends keyed data to the server. The key identifies which hook attachment
    /// this fulfills (e.g., "surfnet" matches a hook's `attachments: [{key: surfnet}]`).
    ///
    /// Once all required attachments are present, the server creates a signed JWT receipt
    /// and emits a `transaction:completed` event to all listeners. Until then, it emits
    /// `transaction:attach` to acknowledge each attachment.
    ///
    /// Attachments are stored as: attachments[actor_id][key] = data
    ///
    /// # Arguments
    /// * `key` - The attachment key (e.g., "surfnet", "billing")
    /// * `data` - The data payload to attach
    ///
    /// Returns the created event with its ID and timestamp.
    pub async fn attach<T: Serialize>(
        &self,
        key: impl Into<String>,
        data: T,
    ) -> Result<ChannelEvent> {
        let secret = self.config.secret.as_ref().ok_or_else(|| {
            PaymentStreamError::Authentication("Secret key required to attach data".to_string())
        })?;

        let actor_id = self.config.actor_id.as_ref().ok_or_else(|| {
            PaymentStreamError::Authentication("Actor ID required to attach data".to_string())
        })?;

        let url = self.build_attachments_url();
        let key = key.into();
        debug!(channel_id = %self.channel_id, url = %url, actor_id = %actor_id, key = %key, "Attaching data to transaction");

        // Construct the request body with actor_id, key and data
        let request_body = serde_json::json!({
            "actor_id": actor_id,
            "key": key,
            "data": serde_json::to_value(&data).unwrap_or(serde_json::Value::Null)
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", secret))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PaymentStreamError::Send(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        // Parse the response as ChannelEvent
        let event: ChannelEvent = response.json().await?;

        info!(
            channel_id = %self.channel_id,
            event_id = %event.id(),
            event_type = %event.event_type(),
            actor_id = %actor_id,
            key = %key,
            "Data attached successfully"
        );

        Ok(event)
    }

    /// Set the connection state
    fn set_state(&self, new_state: ConnectionState) {
        let mut state = self.state.write();
        *state = new_state;
    }

    /// Run the SSE connection loop
    async fn run_connection(
        url: String,
        channel_id: String,
        event_tx: broadcast::Sender<ChannelEvent>,
        state: Arc<RwLock<ConnectionState>>,
        reconnect_attempts: Arc<RwLock<u32>>,
        config: ChannelConfig,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        loop {
            // Build request
            let client = reqwest::Client::new();
            let request = client.get(&url);

            // Create EventSource
            let mut es = EventSource::new(request).expect("Failed to create EventSource");

            // Update state to connected once we start receiving
            {
                let mut s = state.write();
                *s = ConnectionState::Connected;
            }
            {
                let mut attempts = reconnect_attempts.write();
                *attempts = 0;
            }
            info!(channel_id = %channel_id, "Hook connected to channel");

            // Process events
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!(channel_id = %channel_id, "Hook shutdown signal received");
                        es.close();
                        return;
                    }
                    event = es.next() => {
                        match event {
                            Some(Ok(SseEvent::Open)) => {
                                debug!(channel_id = %channel_id, "Hook SSE connection opened");
                            }
                            Some(Ok(SseEvent::Message(msg))) => {
                                // Parse the event
                                match serde_json::from_str::<ChannelEvent>(&msg.data) {
                                    Ok(channel_event) => {
                                        debug!(
                                            channel_id = %channel_id,
                                            event_id = %channel_event.id(),
                                            event_type = %channel_event.event_type(),
                                            "Hook received event"
                                        );
                                        // Broadcast to subscribers
                                        let _ = event_tx.send(channel_event);
                                    }
                                    Err(e) => {
                                        warn!(
                                            channel_id = %channel_id,
                                            error = %e,
                                            data = %msg.data,
                                            "Hook failed to parse event"
                                        );
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                error!(channel_id = %channel_id, error = %e, "Hook SSE error");
                                break;
                            }
                            None => {
                                info!(channel_id = %channel_id, "Hook SSE stream ended");
                                break;
                            }
                        }
                    }
                }
            }

            // Connection lost - attempt reconnect if enabled
            es.close();

            if !config.auto_reconnect {
                {
                    let mut s = state.write();
                    *s = ConnectionState::Disconnected;
                }
                return;
            }

            // Check max attempts
            {
                let mut attempts = reconnect_attempts.write();
                *attempts += 1;
                if config.max_reconnect_attempts > 0 && *attempts >= config.max_reconnect_attempts {
                    error!(
                        channel_id = %channel_id,
                        attempts = *attempts,
                        "Hook max reconnection attempts reached"
                    );
                    let mut s = state.write();
                    *s = ConnectionState::Disconnected;
                    return;
                }
            }

            {
                let mut s = state.write();
                *s = ConnectionState::Reconnecting;
            }

            info!(
                channel_id = %channel_id,
                delay_ms = config.reconnect_delay_ms,
                "Hook scheduling reconnection"
            );

            tokio::time::sleep(tokio::time::Duration::from_millis(
                config.reconnect_delay_ms,
            ))
            .await;
        }
    }
}

impl Drop for PaymentHook {
    fn drop(&mut self) {
        // Signal shutdown synchronously
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_sse_url_with_token() {
        let config = ChannelConfig::new("https://api.example.com").with_secret("my-secret");
        let hook = PaymentHook::new("test-channel", config);
        assert_eq!(
            hook.build_sse_url(),
            "https://api.example.com/payment/v1/channels/test-channel?token=my-secret"
        );
    }

    #[test]
    fn test_build_sse_url_with_all_params() {
        let config = ChannelConfig::new("https://api.example.com")
            .with_secret("my-secret")
            .with_replay(5)
            .with_stream_id("my-stream");
        let hook = PaymentHook::new("test-channel", config);
        assert_eq!(
            hook.build_sse_url(),
            "https://api.example.com/payment/v1/channels/test-channel?token=my-secret&replay=5&stream_id=my-stream"
        );
    }

    #[test]
    fn test_build_attachments_url() {
        let config = ChannelConfig::new("https://api.example.com").with_secret("my-secret");
        let hook = PaymentHook::new("test-channel", config);
        assert_eq!(
            hook.build_attachments_url(),
            "https://api.example.com/payment/v1/channels/test-channel/attachments"
        );
    }
}
