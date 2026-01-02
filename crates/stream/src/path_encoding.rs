//! Path encoding for filesystem-safe storage.
//!
//! URL paths are encoded using base64url (RFC 4648 §5) to make them safe
//! for filesystem storage. Long paths are truncated with a hash suffix.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

/// Maximum length for encoded path before truncation
const MAX_PATH_LENGTH: usize = 200;
/// Length to truncate to (leaving room for hash suffix)
const TRUNCATE_LENGTH: usize = 180;
/// Length of hash prefix used for truncated paths
const HASH_PREFIX_LENGTH: usize = 16;

/// Encode a URL path to a filesystem-safe string.
///
/// Uses base64url encoding. Long paths are truncated with a hash suffix
/// to ensure uniqueness while staying within filesystem limits.
pub fn encode_path(path: &str) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(path.as_bytes());

    if encoded.len() > MAX_PATH_LENGTH {
        // Truncate and add hash suffix for uniqueness
        let hash = compute_hash(path);
        let truncated = &encoded[..TRUNCATE_LENGTH];
        format!("{}~{}", truncated, &hash[..HASH_PREFIX_LENGTH])
    } else {
        encoded
    }
}

/// Decode a filesystem-safe string back to a URL path.
///
/// Note: For truncated paths, this returns None as the original path
/// cannot be recovered from a truncated encoding.
pub fn decode_path(encoded: &str) -> Option<String> {
    // Check if this is a truncated path (contains ~ separator)
    if encoded.contains('~') {
        // Cannot recover original path from truncated encoding
        return None;
    }

    URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

/// Compute SHA256 hash of a string, returning hex encoding.
fn compute_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Generate a unique directory name for a stream.
///
/// Format: `{encoded_path}~{timestamp}~{random_hex}`
/// This allows safe async deletion and immediate path reuse.
pub fn generate_stream_dir_name(path: &str) -> String {
    let encoded = encode_path(path);
    let timestamp = chrono::Utc::now().timestamp_millis();
    let random: u64 = rand::random();
    format!("{}~{}~{:016x}", encoded, timestamp, random)
}

/// Extract the encoded path portion from a stream directory name.
pub fn extract_encoded_path(dir_name: &str) -> Option<&str> {
    // Find first ~ that separates encoded path from timestamp
    dir_name.split('~').next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_simple() {
        let path = "/stream/users:created";
        let encoded = encode_path(path);
        let decoded = decode_path(&encoded);
        assert_eq!(decoded, Some(path.to_string()));
    }

    #[test]
    fn test_encode_special_chars() {
        let path = "/stream/events?filter=active&limit=100";
        let encoded = encode_path(path);
        // Should be base64url safe
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('='));
    }

    #[test]
    fn test_encode_long_path() {
        let path = "/".to_string() + &"a".repeat(500);
        let encoded = encode_path(&path);

        // Should be truncated with hash suffix
        assert!(encoded.contains('~'));
        assert!(encoded.len() <= MAX_PATH_LENGTH);

        // Cannot decode truncated paths
        assert_eq!(decode_path(&encoded), None);
    }

    #[test]
    fn test_generate_stream_dir_name() {
        let path = "/stream/test";
        let dir_name = generate_stream_dir_name(path);

        // Should contain multiple ~ separators
        let parts: Vec<&str> = dir_name.split('~').collect();
        assert!(parts.len() >= 3);

        // Should be able to extract the encoded path
        let extracted = extract_encoded_path(&dir_name);
        assert!(extracted.is_some());
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let paths = vec![
            "/v1/streams/users",
            "/api/events/payment:completed",
            "/test/path/with/many/segments",
            "/unicode/路径/тест",
        ];

        for path in paths {
            let encoded = encode_path(path);
            if !encoded.contains('~') {
                // Non-truncated paths should roundtrip
                let decoded = decode_path(&encoded);
                assert_eq!(decoded, Some(path.to_string()), "Failed for path: {}", path);
            }
        }
    }
}
