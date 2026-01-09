use std::sync::Arc;

use futures::StreamExt;
use parking_lot::RwLock;
use reqwest_eventsource::{Event as SseEvent, EventSource};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::{
    error::Result,
    types::{ChannelConfig, ChannelEvent, ConnectionState},
};

/// Event reader - read-only channel subscriber
///
/// This is the Rust equivalent of the JavaScript SDK's EventReader.
/// It connects to a channel via SSE and receives events.
///
/// # Example
///
/// ```ignore
/// use moneymq_processor::{EventReader, ChannelConfig};
///
/// let config = ChannelConfig::new("https://api.example.com")
///     .with_replay(10);
///
/// let reader = EventReader::new("order-123", config);
///
/// // Subscribe to events
/// let mut rx = reader.subscribe();
///
/// // Connect and start receiving
/// reader.connect().await?;
///
/// while let Some(event) = rx.recv().await {
///     println!("Received: {:?}", event);
/// }
/// ```
pub struct EventReader {
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
}

impl EventReader {
    /// Create a new event reader for the given channel
    pub fn new(channel_id: impl Into<String>, config: ChannelConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            channel_id: channel_id.into(),
            config,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            event_tx,
            shutdown_tx: None,
            reconnect_attempts: Arc::new(RwLock::new(0)),
        }
    }

    /// Get the current connection state
    pub fn state(&self) -> ConnectionState {
        *self.state.read()
    }

    /// Subscribe to events from this reader
    ///
    /// Returns a receiver that will receive all events.
    /// Multiple subscribers can be created.
    pub fn subscribe(&self) -> broadcast::Receiver<ChannelEvent> {
        self.event_tx.subscribe()
    }

    /// Build the SSE URL for this channel
    fn build_url(&self) -> String {
        let mut url = format!(
            "{}/payment/v1/channels/{}",
            self.config.endpoint.trim_end_matches('/'),
            self.channel_id
        );

        let mut params = Vec::new();

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

    /// Connect to the channel and start receiving events
    pub async fn connect(&mut self) -> Result<()> {
        if self.state() == ConnectionState::Connected {
            return Ok(());
        }

        self.set_state(ConnectionState::Connecting);

        let url = self.build_url();
        info!(channel_id = %self.channel_id, url = %url, "Connecting to channel");

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
        info!(channel_id = %self.channel_id, "Disconnected from channel");
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
            info!(channel_id = %channel_id, "Connected to channel");

            // Process events
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!(channel_id = %channel_id, "Shutdown signal received");
                        es.close();
                        return;
                    }
                    event = es.next() => {
                        match event {
                            Some(Ok(SseEvent::Open)) => {
                                debug!(channel_id = %channel_id, "SSE connection opened");
                            }
                            Some(Ok(SseEvent::Message(msg))) => {
                                // Parse the event
                                match serde_json::from_str::<ChannelEvent>(&msg.data) {
                                    Ok(channel_event) => {
                                        debug!(
                                            channel_id = %channel_id,
                                            event_id = %channel_event.id(),
                                            event_type = %channel_event.event_type(),
                                            "Received event"
                                        );
                                        // Broadcast to subscribers
                                        let _ = event_tx.send(channel_event);
                                    }
                                    Err(e) => {
                                        warn!(
                                            channel_id = %channel_id,
                                            error = %e,
                                            data = %msg.data,
                                            "Failed to parse event"
                                        );
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                error!(channel_id = %channel_id, error = %e, "SSE error");
                                break;
                            }
                            None => {
                                info!(channel_id = %channel_id, "SSE stream ended");
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
                        "Max reconnection attempts reached"
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
                "Scheduling reconnection"
            );

            tokio::time::sleep(tokio::time::Duration::from_millis(
                config.reconnect_delay_ms,
            ))
            .await;
        }
    }
}

impl Drop for EventReader {
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
    fn test_build_url_basic() {
        let config = ChannelConfig::new("https://api.example.com");
        let reader = EventReader::new("test-channel", config);
        assert_eq!(
            reader.build_url(),
            "https://api.example.com/payment/v1/channels/test-channel"
        );
    }

    #[test]
    fn test_build_url_with_replay() {
        let config = ChannelConfig::new("https://api.example.com").with_replay(10);
        let reader = EventReader::new("test-channel", config);
        assert_eq!(
            reader.build_url(),
            "https://api.example.com/payment/v1/channels/test-channel?replay=10"
        );
    }

    #[test]
    fn test_build_url_with_stream_id() {
        let config = ChannelConfig::new("https://api.example.com").with_stream_id("my-stream");
        let reader = EventReader::new("test-channel", config);
        assert_eq!(
            reader.build_url(),
            "https://api.example.com/payment/v1/channels/test-channel?stream_id=my-stream"
        );
    }

    #[test]
    fn test_build_url_with_all_params() {
        let config = ChannelConfig::new("https://api.example.com")
            .with_replay(5)
            .with_stream_id("my-stream");
        let reader = EventReader::new("test-channel", config);
        assert_eq!(
            reader.build_url(),
            "https://api.example.com/payment/v1/channels/test-channel?replay=5&stream_id=my-stream"
        );
    }
}
