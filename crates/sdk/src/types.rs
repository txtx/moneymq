use std::collections::HashMap;

use chrono::Utc;
// Re-export shared types from moneymq-types
pub use moneymq_types::{BasketItem, ProductFeature, defaults};
use serde::{Deserialize, Serialize};

/// Event envelope following CloudEvents-like format
/// This matches the format used in the JavaScript SDK
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEvent<T = serde_json::Value> {
    /// Unique event ID (UUID)
    pub id: String,

    /// Event type (e.g., "payment:settled", "order:completed")
    #[serde(rename = "type")]
    pub event_type: String,

    /// Event payload
    pub data: T,

    /// ISO 8601 timestamp
    pub time: String,
}

impl<T> ChannelEvent<T> {
    /// Create a new channel event
    pub fn new(event_type: impl Into<String>, data: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            data,
            time: Utc::now().to_rfc3339(),
        }
    }

    /// Get the event type
    pub fn event_type(&self) -> &str {
        &self.event_type
    }
}

impl ChannelEvent<serde_json::Value> {
    /// Try to deserialize the data into a specific type
    pub fn data_as<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.data.clone())
    }
}

/// Payment details from the x402 payment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetails {
    /// Payer address/wallet
    pub payer: String,

    /// Transaction ID/signature
    #[serde(rename = "transactionId")]
    pub transaction_id: String,

    /// Payment amount as string
    pub amount: String,

    /// Currency code (e.g., "USDC")
    pub currency: String,

    /// Network name (e.g., "solana")
    pub network: String,
}

/// Transaction data received from the payment processor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction ID
    pub id: String,

    /// Channel ID for this transaction
    #[serde(rename = "channelId")]
    pub channel_id: String,

    /// Basket items (products being purchased with features)
    #[serde(default)]
    pub basket: Vec<BasketItem>,

    /// Payment details from x402
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment: Option<PaymentDetails>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Payment verification data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentVerified {
    /// Payer address
    pub payer: String,

    /// Payment amount as string
    pub amount: String,

    /// Network name
    pub network: String,

    /// Product ID (if applicable)
    #[serde(rename = "productId")]
    pub product_id: Option<String>,
}

/// Payment settlement data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSettled {
    /// Payer address
    pub payer: String,

    /// Payment amount as string
    pub amount: String,

    /// Network name
    pub network: String,

    /// Transaction signature (for blockchain payments)
    #[serde(rename = "transactionSignature")]
    pub transaction_signature: Option<String>,

    /// Product ID (if applicable)
    #[serde(rename = "productId")]
    pub product_id: Option<String>,
}

/// Payment failure data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentFailed {
    /// Payer address (if known)
    pub payer: Option<String>,

    /// Payment amount as string
    pub amount: String,

    /// Network name
    pub network: String,

    /// Failure reason
    pub reason: String,

    /// Product ID (if applicable)
    #[serde(rename = "productId")]
    pub product_id: Option<String>,
}

/// Connection state for channels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,

    /// Connecting to the server
    Connecting,

    /// Successfully connected
    Connected,

    /// Reconnecting after connection loss
    Reconnecting,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Disconnected => write!(f, "disconnected"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Connected => write!(f, "connected"),
            ConnectionState::Reconnecting => write!(f, "reconnecting"),
        }
    }
}

/// Configuration for channel connections
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// Base URL for the MoneyMQ API
    pub endpoint: String,

    /// Secret key for authentication (required for actors and processors)
    pub secret: Option<String>,

    /// Whether to automatically reconnect on connection loss
    pub auto_reconnect: bool,

    /// Delay between reconnection attempts (milliseconds)
    pub reconnect_delay_ms: u64,

    /// Maximum number of reconnection attempts (0 = infinite)
    pub max_reconnect_attempts: u32,

    /// Number of events to replay on initial connection
    pub replay: Option<u32>,

    /// Stream ID for stateful streams (enables server-side cursor tracking)
    pub stream_id: Option<String>,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:3000".to_string(),
            secret: None,
            auto_reconnect: true,
            reconnect_delay_ms: 1000,
            max_reconnect_attempts: 0, // infinite
            replay: None,
            stream_id: None,
        }
    }
}

impl ChannelConfig {
    /// Create a new configuration with the given endpoint
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            ..Default::default()
        }
    }

    /// Set the secret key for authentication
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        self.secret = Some(secret.into());
        self
    }

    /// Set the number of events to replay
    pub fn with_replay(mut self, count: u32) -> Self {
        self.replay = Some(count);
        self
    }

    /// Set the stream ID for stateful streams
    pub fn with_stream_id(mut self, stream_id: impl Into<String>) -> Self {
        self.stream_id = Some(stream_id.into());
        self
    }

    /// Disable auto-reconnection
    pub fn without_auto_reconnect(mut self) -> Self {
        self.auto_reconnect = false;
        self
    }

    /// Set the reconnection delay
    pub fn with_reconnect_delay(mut self, delay_ms: u64) -> Self {
        self.reconnect_delay_ms = delay_ms;
        self
    }

    /// Set maximum reconnection attempts
    pub fn with_max_reconnect_attempts(mut self, max: u32) -> Self {
        self.max_reconnect_attempts = max;
        self
    }
}

// Re-export event types from moneymq-types
pub use moneymq_types::event_types;
