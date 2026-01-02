//! Durable Streams Server
//!
//! A Rust implementation of the durable streams protocol - append-only logs with replay support.
//!
//! # Features
//!
//! - **Append-only streams**: Create streams and append data with guaranteed ordering
//! - **Replay support**: Read from any offset to catch up on missed messages
//! - **Long-polling**: Wait for new messages with configurable timeout
//! - **Server-Sent Events (SSE)**: Continuous streaming of new messages
//! - **TTL/Expiration**: Automatic stream cleanup based on time
//! - **JSON mode**: Special handling for JSON content with array wrapping
//! - **Writer coordination**: Sequence numbers to prevent duplicate writes
//!
//! # Example
//!
//! ```rust,no_run
//! use durable_stream::{server, types::ServerOptions};
//!
//! #[tokio::main]
//! async fn main() {
//!     let options = ServerOptions {
//!         port: 4437,
//!         host: "127.0.0.1".to_string(),
//!         ..Default::default()
//!     };
//!
//!     server::start_server(options).await.unwrap();
//! }
//! ```
//!
//! # Protocol
//!
//! ## Creating a stream
//!
//! ```text
//! PUT /stream/my-events HTTP/1.1
//! Content-Type: application/json
//! Stream-TTL: 3600
//!
//! Response: 201 Created
//! Stream-Next-Offset: 0000000000000000_0000000000000000
//! ```
//!
//! ## Appending data
//!
//! ```text
//! POST /stream/my-events HTTP/1.1
//! Content-Type: application/json
//!
//! {"event": "user_created", "id": 123}
//!
//! Response: 200 OK
//! Stream-Next-Offset: 0000000000000000_0000000000000042
//! ```
//!
//! ## Reading data
//!
//! ```text
//! GET /stream/my-events?offset=-1 HTTP/1.1
//!
//! Response: 200 OK
//! Stream-Next-Offset: 0000000000000000_0000000000000042
//! Content-Type: application/json
//!
//! [{"event": "user_created", "id": 123}]
//! ```
//!
//! ## Long-polling
//!
//! ```text
//! GET /stream/my-events?offset=0000000000000000_0000000000000042&live=long-poll HTTP/1.1
//!
//! (waits up to 30 seconds for new data)
//!
//! Response: 204 No Content (if no new data)
//! Stream-Up-To-Date: true
//! ```
//!
//! ## Server-Sent Events
//!
//! ```text
//! GET /stream/my-events?offset=-1&live=sse HTTP/1.1
//! Accept: text/event-stream
//!
//! event: data
//! data: {"event": "user_created", "id": 123}
//!
//! event: control
//! data: {"streamNextOffset": "0000000000000000_0000000000000042", "upToDate": true}
//! ```

pub mod cursor;
pub mod path_encoding;
pub mod server;
pub mod store;
pub mod types;

// Re-export commonly used items
pub use server::{create_router, start_server, AppState};
pub use store::{StoreError, StreamStore};
pub use types::{
    ReadResult, ServerOptions, Stream, StreamConfig, StreamLifecycleEvent, StreamMessage,
};
