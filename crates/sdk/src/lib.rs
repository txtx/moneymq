//! MoneyMQ SDK
//!
//! This crate provides server-side abstractions for processing payment events
//! from MoneyMQ. It is the Rust equivalent of the JavaScript SDK's processor
//! functionality.
//!
//! # Overview
//!
//! The SDK provides three main abstractions:
//!
//! - [`EventReader`] - Read-only channel subscriber (for receiving events)
//! - [`PaymentHook`] - Bidirectional channel participant (receive + attach data)
//! - [`PaymentStream`] - Transaction stream that spawns handlers for each payment
//!
//! # Quick Start
//!
//! ## Processing Payments
//!
//! ```ignore
//! use moneymq_sdk::{PaymentStream, PaymentStreamConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Configure the payment stream
//!     let config = PaymentStreamConfig::new(
//!         "https://api.money.mq",
//!         "your-secret-key"
//!     )
//!     .with_stream_id("my-payment-stream");
//!
//!     // Create and connect the payment stream
//!     let mut stream = PaymentStream::new(config);
//!     let mut rx = stream.subscribe().expect("Already subscribed");
//!     stream.connect().await?;
//!
//!     // Handle incoming transactions
//!     while let Some(tx_ctx) = rx.recv().await {
//!         println!("Payment received: {} {} from {:?}",
//!             tx_ctx.amount(),
//!             tx_ctx.currency(),
//!             tx_ctx.payer()
//!         );
//!
//!         // Create a hook to respond on this transaction's channel
//!         let mut hook = tx_ctx.hook();
//!         hook.connect().await?;
//!
//!         // Do your business logic here...
//!         // e.g., fulfill an order, grant access, etc.
//!
//!         // Attach completion data with a key - server creates JWT receipt and emits transaction:completed
//!         // The key identifies which hook attachment this fulfills (e.g., "surfnet", "billing")
//!         hook.attach("fulfillment", serde_json::json!({
//!             "order_id": tx_ctx.id(),
//!             "status": "fulfilled"
//!         })).await?;
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Subscribing to a Channel
//!
//! ```ignore
//! use moneymq_sdk::{EventReader, ChannelConfig};
//!
//! let config = ChannelConfig::new("https://api.money.mq")
//!     .with_replay(10); // Replay last 10 events
//!
//! let mut reader = EventReader::new("order-123", config);
//! let mut rx = reader.subscribe();
//! reader.connect().await?;
//!
//! while let Ok(event) = rx.recv().await {
//!     println!("Event: {} - {:?}", event.event_type, event.data);
//! }
//! ```
//!
//! # Event Types
//!
//! The SDK recognizes these standard payment event types:
//!
//! - `payment:verified` - Payment has been verified
//! - `payment:settled` - Payment has been settled
//! - `payment:verification_failed` - Payment verification failed
//! - `payment:settlement_failed` - Payment settlement failed
//!
//! Custom event types can also be used for application-specific events.

pub mod actor;
pub mod error;
pub mod processor;
pub mod reader;
pub mod types;

// Re-export main types at crate root
pub use actor::PaymentHook;
pub use error::{PaymentStreamError, Result};
pub use processor::{PaymentStream, PaymentStreamConfig, TransactionContext};
pub use reader::EventReader;
pub use types::{
    ChannelConfig, ChannelEvent, ConnectionState, PaymentFailed, PaymentFailedData, PaymentSettled,
    PaymentSettledData, PaymentVerified, PaymentVerifiedData, Transaction,
    TransactionCompletedData, event_types,
};
