use thiserror::Error;

/// Errors that can occur in the payment stream SDK
#[derive(Error, Debug)]
pub enum PaymentStreamError {
    /// Connection error (failed to connect to SSE endpoint)
    #[error("Connection error: {0}")]
    Connection(String),

    /// Authentication error (invalid or missing credentials)
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Parse error (failed to parse event data)
    #[error("Parse error: {0}")]
    Parse(String),

    /// Send error (failed to publish event)
    #[error("Send error: {0}")]
    Send(String),

    /// HTTP error from reqwest
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Channel closed
    #[error("Channel closed")]
    ChannelClosed,

    /// Connection lost
    #[error("Connection lost")]
    ConnectionLost,

    /// Timeout
    #[error("Timeout")]
    Timeout,
}

/// Result type alias for payment stream operations
pub type Result<T> = std::result::Result<T, PaymentStreamError>;
