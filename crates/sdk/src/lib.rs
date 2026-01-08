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
//! - [`EventActor`] - Bidirectional channel participant (receive + publish)
//! - [`Processor`] - Transaction processor that spawns handlers for each payment
//!
//! # Quick Start
//!
//! ## Processing Payments
//!
//! ```ignore
//! use moneymq_sdk::{Processor, ProcessorConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Configure the processor
//!     let config = ProcessorConfig::new(
//!         "https://api.money.mq",
//!         "your-secret-key"
//!     )
//!     .with_stream_id("my-payment-processor");
//!
//!     // Create and connect the processor
//!     let mut processor = Processor::new(config);
//!     let mut rx = processor.subscribe().expect("Already subscribed");
//!     processor.connect().await?;
//!
//!     // Handle incoming transactions
//!     while let Some(tx_ctx) = rx.recv().await {
//!         println!("Payment received: {} {} from {:?}",
//!             tx_ctx.amount(),
//!             tx_ctx.currency(),
//!             tx_ctx.payer()
//!         );
//!
//!         // Create an actor to respond on this transaction's channel
//!         let mut actor = tx_ctx.actor();
//!         actor.connect().await?;
//!
//!         // Do your business logic here...
//!         // e.g., fulfill an order, grant access, etc.
//!
//!         // Attach completion data - server creates JWT receipt and emits transaction:completed
//!         actor.attach(serde_json::json!({
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
pub use actor::EventActor;
pub use error::{ProcessorError, Result};
pub use processor::{Processor, ProcessorConfig, TransactionContext};
pub use reader::EventReader;
pub use types::{
    ChannelConfig, ChannelEvent, ConnectionState, PaymentFailed, PaymentSettled, PaymentVerified,
    Transaction, event_types,
};
