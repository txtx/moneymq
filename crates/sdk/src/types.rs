use std::collections::HashMap;

// Re-export shared types from moneymq-types
pub use moneymq_types::{
    BasketItem, ChannelEvent, PaymentFailedData, PaymentSettledData, PaymentVerifiedData,
    ProductFeature, TransactionCompletedData, defaults, event_types,
};
use serde::{Deserialize, Serialize};

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

// Type aliases for backwards compatibility
pub type PaymentVerified = PaymentVerifiedData;
pub type PaymentSettled = PaymentSettledData;
pub type PaymentFailed = PaymentFailedData;

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

    /// Actor ID (identifies this hook/processor in attachments)
    /// Used as the outer key in attachments: attachments[actor_id][key] = data
    pub actor_id: Option<String>,

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
            actor_id: None,
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

    /// Set the actor ID for attachments
    /// This becomes the outer key in attachments: attachments[actor_id][key] = data
    pub fn with_actor_id(mut self, actor_id: impl Into<String>) -> Self {
        self.actor_id = Some(actor_id.into());
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
