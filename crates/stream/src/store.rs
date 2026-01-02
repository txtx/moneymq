//! In-memory stream storage.
//!
//! Provides a thread-safe in-memory implementation of stream storage
//! with support for long-polling and SSE.

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing::{debug, info};

use crate::types::{
    compare_offsets, format_offset, normalize_content_type, parse_offset, ReadResult, Stream,
    StreamConfig, StreamLifecycleEvent, StreamMessage,
};

/// Error types for store operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Stream not found: {0}")]
    NotFound(String),

    #[error("Stream already exists with different configuration")]
    ConfigMismatch,

    #[error("Content-type mismatch: expected {expected}, got {actual}")]
    ContentTypeMismatch { expected: String, actual: String },

    #[error("Sequence conflict: {0}")]
    SequenceConflict(String),

    #[error("Invalid offset format: {0}")]
    InvalidOffset(String),

    #[error("Empty body not allowed")]
    EmptyBody,

    #[error("Cannot specify both TTL and Expires-At")]
    TtlConflict,

    #[error("Invalid TTL value")]
    InvalidTtl,

    #[error("Invalid Expires-At timestamp")]
    InvalidExpiresAt,

    #[error("Empty arrays not allowed on append")]
    EmptyArrayNotAllowed,
}

/// Notification sent when new data is appended to a stream.
#[derive(Debug, Clone)]
pub struct AppendNotification {
    pub path: String,
    pub offset: String,
}

/// In-memory stream store with long-poll support.
pub struct StreamStore {
    /// Streams indexed by path
    streams: RwLock<HashMap<String, Stream>>,
    /// Broadcast channel for append notifications
    notify_tx: broadcast::Sender<AppendNotification>,
    /// Lifecycle event callback
    on_lifecycle: Option<Box<dyn Fn(StreamLifecycleEvent) + Send + Sync>>,
}

impl StreamStore {
    /// Create a new empty stream store.
    pub fn new() -> Arc<Self> {
        let (notify_tx, _) = broadcast::channel(1024);
        Arc::new(Self {
            streams: RwLock::new(HashMap::new()),
            notify_tx,
            on_lifecycle: None,
        })
    }

    /// Create a new stream store with lifecycle callbacks.
    pub fn with_lifecycle<F>(on_lifecycle: F) -> Arc<Self>
    where
        F: Fn(StreamLifecycleEvent) + Send + Sync + 'static,
    {
        let (notify_tx, _) = broadcast::channel(1024);
        Arc::new(Self {
            streams: RwLock::new(HashMap::new()),
            notify_tx,
            on_lifecycle: Some(Box::new(on_lifecycle)),
        })
    }

    /// Subscribe to append notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<AppendNotification> {
        self.notify_tx.subscribe()
    }

    /// Check if a stream exists (and is not expired).
    pub fn has(&self, path: &str) -> bool {
        let streams = self.streams.read();
        if let Some(stream) = streams.get(path) {
            !stream.is_expired()
        } else {
            false
        }
    }

    /// Get a stream by path.
    pub fn get(&self, path: &str) -> Option<Stream> {
        let mut streams = self.streams.write();
        if let Some(stream) = streams.get(path) {
            if stream.is_expired() {
                // Auto-delete expired stream
                let removed = streams.remove(path);
                if removed.is_some() {
                    self.emit_lifecycle(StreamLifecycleEvent::Deleted {
                        path: path.to_string(),
                        timestamp: Utc::now().timestamp_millis(),
                    });
                }
                return None;
            }
            Some(stream.clone())
        } else {
            None
        }
    }

    /// Create a new stream.
    ///
    /// Returns Ok(true) if created, Ok(false) if already exists with same config.
    /// Returns Err if already exists with different config.
    pub fn create(&self, path: &str, config: StreamConfig) -> Result<bool, StoreError> {
        // Validate TTL/expires-at conflict
        if config.ttl_seconds.is_some() && config.expires_at.is_some() {
            return Err(StoreError::TtlConflict);
        }

        let mut streams = self.streams.write();

        // Check if already exists
        if let Some(existing) = streams.get(path) {
            if existing.is_expired() {
                // Remove expired stream and continue with creation
                streams.remove(path);
                self.emit_lifecycle(StreamLifecycleEvent::Deleted {
                    path: path.to_string(),
                    timestamp: Utc::now().timestamp_millis(),
                });
            } else {
                // Check config matches
                let existing_ct = existing
                    .content_type
                    .as_ref()
                    .map(|ct| normalize_content_type(ct));
                let new_ct = config
                    .content_type
                    .as_ref()
                    .map(|ct| normalize_content_type(ct));

                if existing_ct != new_ct
                    || existing.ttl_seconds != config.ttl_seconds
                    || existing.expires_at != config.expires_at
                {
                    return Err(StoreError::ConfigMismatch);
                }

                // Already exists with same config - idempotent
                return Ok(false);
            }
        }

        // Create new stream
        let mut stream = Stream::new(path.to_string());
        stream.content_type = config.content_type.clone();
        stream.ttl_seconds = config.ttl_seconds;
        stream.expires_at = config.expires_at;

        // Handle initial data
        if let Some(data) = config.initial_data {
            if !data.is_empty() {
                let message = StreamMessage {
                    data,
                    offset: stream.current_offset.clone(),
                    timestamp: Utc::now().timestamp_millis(),
                };

                // Update offset
                let (seq, byte_offset) = parse_offset(&stream.current_offset).unwrap();
                stream.current_offset = format_offset(seq, byte_offset + message.data.len() as u64);
                stream.messages.push(message);
            }
        }

        info!(path = %path, "Created stream");
        streams.insert(path.to_string(), stream.clone());

        self.emit_lifecycle(StreamLifecycleEvent::Created {
            path: path.to_string(),
            content_type: config.content_type,
            timestamp: Utc::now().timestamp_millis(),
        });

        Ok(true)
    }

    /// Delete a stream.
    pub fn delete(&self, path: &str) -> bool {
        let mut streams = self.streams.write();
        let removed = streams.remove(path).is_some();

        if removed {
            info!(path = %path, "Deleted stream");
            self.emit_lifecycle(StreamLifecycleEvent::Deleted {
                path: path.to_string(),
                timestamp: Utc::now().timestamp_millis(),
            });
        }

        removed
    }

    /// Append data to a stream.
    ///
    /// # Arguments
    /// * `path` - Stream path
    /// * `data` - Data to append
    /// * `content_type` - Content type of the data
    /// * `seq` - Optional sequence number for writer coordination
    ///
    /// # Returns
    /// The new offset after appending
    pub fn append(
        &self,
        path: &str,
        data: Vec<u8>,
        content_type: Option<&str>,
        seq: Option<&str>,
    ) -> Result<String, StoreError> {
        if data.is_empty() {
            return Err(StoreError::EmptyBody);
        }

        let mut streams = self.streams.write();

        let stream = streams
            .get_mut(path)
            .ok_or_else(|| StoreError::NotFound(path.to_string()))?;

        // Check if expired
        if stream.is_expired() {
            streams.remove(path);
            self.emit_lifecycle(StreamLifecycleEvent::Deleted {
                path: path.to_string(),
                timestamp: Utc::now().timestamp_millis(),
            });
            return Err(StoreError::NotFound(path.to_string()));
        }

        // Validate content type
        if let Some(ct) = content_type {
            if let Some(ref stream_ct) = stream.content_type {
                if normalize_content_type(ct) != normalize_content_type(stream_ct) {
                    return Err(StoreError::ContentTypeMismatch {
                        expected: stream_ct.clone(),
                        actual: ct.to_string(),
                    });
                }
            }
        }

        // Validate sequence (must be strictly increasing)
        if let Some(s) = seq {
            if let Some(ref last_seq) = stream.last_seq {
                if s <= last_seq.as_str() {
                    return Err(StoreError::SequenceConflict(format!(
                        "Sequence {} <= last sequence {}",
                        s, last_seq
                    )));
                }
            }
            stream.last_seq = Some(s.to_string());
        }

        // Check for empty JSON array
        if stream.is_json() {
            let trimmed = String::from_utf8_lossy(&data);
            let trimmed = trimmed.trim();
            if trimmed == "[]" {
                return Err(StoreError::EmptyArrayNotAllowed);
            }
        }

        // Prepare data for JSON mode (add trailing comma)
        let data = if stream.is_json() {
            let mut d = data;
            // Strip any existing trailing commas and add our own
            while d.last() == Some(&b',') {
                d.pop();
            }
            d.push(b',');
            d
        } else {
            data
        };

        let message = StreamMessage {
            data: data.clone(),
            offset: stream.current_offset.clone(),
            timestamp: Utc::now().timestamp_millis(),
        };

        // Update offset
        let (seq_num, byte_offset) = parse_offset(&stream.current_offset).unwrap();
        let new_offset = format_offset(seq_num, byte_offset + data.len() as u64);
        stream.current_offset = new_offset.clone();
        stream.messages.push(message);

        debug!(path = %path, offset = %new_offset, "Appended to stream");

        // Notify waiters
        let _ = self.notify_tx.send(AppendNotification {
            path: path.to_string(),
            offset: new_offset.clone(),
        });

        Ok(new_offset)
    }

    /// Read messages from a stream starting after the given offset.
    ///
    /// # Arguments
    /// * `path` - Stream path
    /// * `offset` - Read messages after this offset. Use "-1" for beginning.
    ///
    /// # Returns
    /// ReadResult with messages and next offset
    pub fn read(&self, path: &str, offset: &str) -> Result<ReadResult, StoreError> {
        let stream = self
            .get(path)
            .ok_or_else(|| StoreError::NotFound(path.to_string()))?;

        // Validate offset format
        if offset != "-1" && parse_offset(offset).is_none() {
            return Err(StoreError::InvalidOffset(offset.to_string()));
        }

        // Find messages starting from the given offset (inclusive)
        // Special case: "-1" means from the beginning (include all messages)
        let messages: Vec<StreamMessage> = if offset == "-1" {
            stream.messages.clone()
        } else {
            stream
                .messages
                .iter()
                .filter(|m| compare_offsets(&m.offset, offset) >= std::cmp::Ordering::Equal)
                .cloned()
                .collect()
        };

        let next_offset = messages
            .last()
            .map(|m| {
                let (seq, byte_offset) = parse_offset(&m.offset).unwrap();
                format_offset(seq, byte_offset + m.data.len() as u64)
            })
            .unwrap_or_else(|| stream.current_offset.clone());

        let up_to_date =
            compare_offsets(&next_offset, &stream.current_offset) >= std::cmp::Ordering::Equal;

        Ok(ReadResult {
            messages,
            next_offset,
            up_to_date,
        })
    }

    /// Get the current offset of a stream.
    pub fn get_current_offset(&self, path: &str) -> Option<String> {
        self.get(path).map(|s| s.current_offset)
    }

    /// Format a read response based on content type.
    ///
    /// For JSON streams, wraps the concatenated data in array brackets.
    pub fn format_response(&self, path: &str, messages: &[StreamMessage]) -> Vec<u8> {
        if let Some(stream) = self.get(path) {
            if stream.is_json() {
                // Concatenate all message data
                let mut data: Vec<u8> = messages.iter().flat_map(|m| m.data.clone()).collect();

                // Strip trailing comma if present
                while data.last() == Some(&b',') {
                    data.pop();
                }

                // Wrap in array brackets
                let mut result = vec![b'['];
                result.extend(data);
                result.push(b']');
                return result;
            }
        }

        // Non-JSON: just concatenate
        messages.iter().flat_map(|m| m.data.clone()).collect()
    }

    /// List all non-expired streams.
    pub fn list(&self) -> Vec<String> {
        let streams = self.streams.read();
        streams
            .iter()
            .filter(|(_, s)| !s.is_expired())
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Clear all streams.
    pub fn clear(&self) {
        let mut streams = self.streams.write();
        streams.clear();
    }

    fn emit_lifecycle(&self, event: StreamLifecycleEvent) {
        if let Some(ref callback) = self.on_lifecycle {
            callback(event);
        }
    }
}

impl Default for StreamStore {
    fn default() -> Self {
        let (notify_tx, _) = broadcast::channel(1024);
        Self {
            streams: RwLock::new(HashMap::new()),
            notify_tx,
            on_lifecycle: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_stream() {
        let store = StreamStore::new();

        let config = StreamConfig {
            content_type: Some("text/plain".to_string()),
            ..Default::default()
        };

        let created = store.create("/test/stream", config).unwrap();
        assert!(created);

        let stream = store.get("/test/stream").unwrap();
        assert_eq!(stream.path, "/test/stream");
        assert_eq!(stream.content_type, Some("text/plain".to_string()));
    }

    #[test]
    fn test_create_idempotent() {
        let store = StreamStore::new();

        let config = StreamConfig {
            content_type: Some("text/plain".to_string()),
            ..Default::default()
        };

        let created1 = store.create("/test/stream", config.clone()).unwrap();
        let created2 = store.create("/test/stream", config).unwrap();

        assert!(created1);
        assert!(!created2); // Second create returns false (already exists)
    }

    #[test]
    fn test_create_config_mismatch() {
        let store = StreamStore::new();

        let config1 = StreamConfig {
            content_type: Some("text/plain".to_string()),
            ..Default::default()
        };

        let config2 = StreamConfig {
            content_type: Some("application/json".to_string()),
            ..Default::default()
        };

        store.create("/test/stream", config1).unwrap();
        let result = store.create("/test/stream", config2);

        assert!(matches!(result, Err(StoreError::ConfigMismatch)));
    }

    #[test]
    fn test_append_and_read() {
        let store = StreamStore::new();

        store
            .create(
                "/test/stream",
                StreamConfig {
                    content_type: Some("text/plain".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        // Append some data
        store
            .append("/test/stream", b"hello".to_vec(), Some("text/plain"), None)
            .unwrap();

        store
            .append("/test/stream", b"world".to_vec(), Some("text/plain"), None)
            .unwrap();

        // Read from beginning
        let result = store.read("/test/stream", "-1").unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].data, b"hello");
        assert_eq!(result.messages[1].data, b"world");
    }

    #[test]
    fn test_read_after_offset() {
        let store = StreamStore::new();

        store
            .create(
                "/test/stream",
                StreamConfig {
                    content_type: Some("text/plain".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        let offset1 = store
            .append("/test/stream", b"msg1".to_vec(), Some("text/plain"), None)
            .unwrap();

        store
            .append("/test/stream", b"msg2".to_vec(), Some("text/plain"), None)
            .unwrap();

        // Read after first message
        let initial_offset = format_offset(0, 0);
        let result = store.read("/test/stream", &initial_offset).unwrap();
        assert_eq!(result.messages.len(), 2);

        // Read after offset1 (should only get msg2)
        let result = store.read("/test/stream", &offset1).unwrap();
        // Note: This depends on how offsets work - need to verify behavior
    }

    #[test]
    fn test_json_format_response() {
        let store = StreamStore::new();

        store
            .create(
                "/test/json",
                StreamConfig {
                    content_type: Some("application/json".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        store
            .append(
                "/test/json",
                b"{\"a\":1}".to_vec(),
                Some("application/json"),
                None,
            )
            .unwrap();

        store
            .append(
                "/test/json",
                b"{\"b\":2}".to_vec(),
                Some("application/json"),
                None,
            )
            .unwrap();

        let result = store.read("/test/json", "-1").unwrap();
        let formatted = store.format_response("/test/json", &result.messages);

        // Should be wrapped in array brackets
        let formatted_str = String::from_utf8(formatted).unwrap();
        assert!(formatted_str.starts_with('['));
        assert!(formatted_str.ends_with(']'));
    }

    #[test]
    fn test_delete_stream() {
        let store = StreamStore::new();

        store
            .create("/test/stream", StreamConfig::default())
            .unwrap();

        assert!(store.has("/test/stream"));

        let deleted = store.delete("/test/stream");
        assert!(deleted);
        assert!(!store.has("/test/stream"));
    }

    #[test]
    fn test_sequence_conflict() {
        let store = StreamStore::new();

        store
            .create(
                "/test/stream",
                StreamConfig {
                    content_type: Some("text/plain".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        // First append with seq 2
        store
            .append(
                "/test/stream",
                b"msg1".to_vec(),
                Some("text/plain"),
                Some("2"),
            )
            .unwrap();

        // Try to append with seq 1 (should fail)
        let result = store.append(
            "/test/stream",
            b"msg2".to_vec(),
            Some("text/plain"),
            Some("1"),
        );

        assert!(matches!(result, Err(StoreError::SequenceConflict(_))));
    }
}
