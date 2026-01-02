//! Core types for the durable streams server.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A message stored in a stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessage {
    /// Raw message bytes
    pub data: Vec<u8>,
    /// Offset in format "readSeq_byteOffset" (e.g., "0000000000000000_0000000000001024")
    /// Lexicographically sortable
    pub offset: String,
    /// Timestamp when the message was appended (milliseconds since epoch)
    pub timestamp: i64,
}

/// Stream metadata and messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stream {
    /// URL path identifying the stream
    pub path: String,
    /// MIME type of the stream content
    pub content_type: Option<String>,
    /// Messages stored in the stream
    pub messages: Vec<StreamMessage>,
    /// Current tail offset (points to next write position)
    pub current_offset: String,
    /// Last sequence number for writer coordination
    pub last_seq: Option<String>,
    /// Time-to-live in seconds (relative TTL)
    pub ttl_seconds: Option<u64>,
    /// Absolute expiration timestamp (ISO 8601)
    pub expires_at: Option<DateTime<Utc>>,
    /// Creation timestamp (milliseconds since epoch)
    pub created_at: i64,
}

impl Stream {
    /// Create a new stream with the given path.
    pub fn new(path: String) -> Self {
        Self {
            path,
            content_type: None,
            messages: Vec::new(),
            current_offset: format_offset(0, 0),
            last_seq: None,
            ttl_seconds: None,
            expires_at: None,
            created_at: Utc::now().timestamp_millis(),
        }
    }

    /// Check if the stream has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            return Utc::now() >= expires_at;
        }
        if let Some(ttl_seconds) = self.ttl_seconds {
            let expiry = self.created_at + (ttl_seconds as i64 * 1000);
            return Utc::now().timestamp_millis() >= expiry;
        }
        false
    }

    /// Check if this stream uses JSON content type.
    pub fn is_json(&self) -> bool {
        self.content_type
            .as_ref()
            .map(|ct| normalize_content_type(ct) == "application/json")
            .unwrap_or(false)
    }
}

/// Stream lifecycle event for hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamLifecycleEvent {
    Created {
        path: String,
        content_type: Option<String>,
        timestamp: i64,
    },
    Deleted {
        path: String,
        timestamp: i64,
    },
}

/// Configuration for creating a stream.
#[derive(Debug, Clone, Default)]
pub struct StreamConfig {
    pub content_type: Option<String>,
    pub ttl_seconds: Option<u64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub initial_data: Option<Vec<u8>>,
}

/// Result of reading from a stream.
#[derive(Debug, Clone)]
pub struct ReadResult {
    /// Messages read from the stream
    pub messages: Vec<StreamMessage>,
    /// Next offset to use for subsequent reads
    pub next_offset: String,
    /// Whether we've caught up to the tail
    pub up_to_date: bool,
}

/// Server configuration options.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Port to listen on (0 for auto-assign)
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Long-poll timeout in milliseconds
    pub long_poll_timeout_ms: u64,
    /// Data directory for file-backed storage (None for in-memory)
    pub data_dir: Option<String>,
    /// Enable compression
    pub compression: bool,
    /// Cursor interval in seconds
    pub cursor_interval_seconds: u64,
    /// Cursor epoch for interval calculation
    pub cursor_epoch: DateTime<Utc>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            port: 4437,
            host: "127.0.0.1".to_string(),
            long_poll_timeout_ms: 30_000,
            data_dir: None,
            compression: true,
            cursor_interval_seconds: 20,
            // October 9, 2024 as default epoch
            cursor_epoch: DateTime::parse_from_rfc3339("2024-10-09T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        }
    }
}

/// Format an offset string from read sequence and byte offset.
/// Format: "readSeq_byteOffset" with 16-digit zero-padding each.
pub fn format_offset(read_seq: u64, byte_offset: u64) -> String {
    format!("{:016}_{:016}", read_seq, byte_offset)
}

/// Parse an offset string into (read_seq, byte_offset).
/// Returns None if the format is invalid.
pub fn parse_offset(offset: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = offset.split('_').collect();
    if parts.len() != 2 {
        return None;
    }
    let read_seq = parts[0].parse().ok()?;
    let byte_offset = parts[1].parse().ok()?;
    Some((read_seq, byte_offset))
}

/// Normalize a content type by stripping charset and parameters.
pub fn normalize_content_type(content_type: &str) -> &str {
    content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
}

/// Compare two offsets lexicographically.
/// Returns Ordering::Less if a < b, Equal if a == b, Greater if a > b.
pub fn compare_offsets(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_offset() {
        assert_eq!(format_offset(0, 0), "0000000000000000_0000000000000000");
        assert_eq!(format_offset(0, 1024), "0000000000000000_0000000000001024");
        assert_eq!(format_offset(1, 0), "0000000000000001_0000000000000000");
    }

    #[test]
    fn test_parse_offset() {
        assert_eq!(
            parse_offset("0000000000000000_0000000000001024"),
            Some((0, 1024))
        );
        assert_eq!(parse_offset("invalid"), None);
        assert_eq!(parse_offset("abc_def"), None);
    }

    #[test]
    fn test_normalize_content_type() {
        assert_eq!(
            normalize_content_type("application/json; charset=utf-8"),
            "application/json"
        );
        assert_eq!(normalize_content_type("text/plain"), "text/plain");
    }

    #[test]
    fn test_compare_offsets() {
        use std::cmp::Ordering;
        let a = format_offset(0, 100);
        let b = format_offset(0, 200);
        let c = format_offset(1, 0);

        assert_eq!(compare_offsets(&a, &b), Ordering::Less);
        assert_eq!(compare_offsets(&b, &c), Ordering::Less);
        assert_eq!(compare_offsets(&a, &a), Ordering::Equal);
    }
}
