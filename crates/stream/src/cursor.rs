//! Cursor system for CDN cache collision prevention.
//!
//! The cursor system divides time into fixed intervals and returns cursor values
//! that change at interval boundaries. This prevents infinite cache loops when
//! multiple clients request the same data within the same time period.

use chrono::{DateTime, Utc};
use rand::Rng;

/// Options for cursor calculation.
#[derive(Debug, Clone)]
pub struct CursorOptions {
    /// Interval duration in seconds (default: 20)
    pub interval_seconds: u64,
    /// Epoch for interval calculation (default: Oct 9, 2024)
    pub epoch: DateTime<Utc>,
}

impl Default for CursorOptions {
    fn default() -> Self {
        Self {
            interval_seconds: 20,
            epoch: DateTime::parse_from_rfc3339("2024-10-09T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        }
    }
}

/// Calculate the current cursor value based on time intervals.
///
/// The cursor is the number of intervals that have passed since the epoch.
pub fn calculate_cursor(options: &CursorOptions) -> u64 {
    let now = Utc::now();
    let elapsed = now
        .signed_duration_since(options.epoch)
        .num_seconds()
        .max(0) as u64;
    elapsed / options.interval_seconds
}

/// Generate a response cursor that is guaranteed to be monotonically increasing.
///
/// If the client provides a cursor that is >= the current interval, we add random
/// jitter to ensure the cursor advances. This prevents cache collisions while
/// maintaining monotonicity.
///
/// # Arguments
/// * `client_cursor` - The cursor value provided by the client (if any)
/// * `options` - Cursor calculation options
///
/// # Returns
/// A cursor value that is guaranteed to be >= client_cursor
pub fn generate_response_cursor(client_cursor: Option<u64>, options: &CursorOptions) -> u64 {
    let current_interval = calculate_cursor(options);

    match client_cursor {
        Some(cursor) if cursor >= current_interval => {
            // Client cursor is at or ahead of current interval
            // Add random jitter (1-3600 seconds worth of intervals)
            let mut rng = rand::thread_rng();
            let jitter_seconds: u64 = rng.gen_range(1..=3600);
            let jitter_intervals = jitter_seconds / options.interval_seconds;
            cursor + jitter_intervals.max(1)
        }
        Some(cursor) => {
            // Client cursor is behind, return current interval
            current_interval.max(cursor)
        }
        None => {
            // No client cursor, return current interval
            current_interval
        }
    }
}

/// Parse a cursor string into a u64.
pub fn parse_cursor(cursor: &str) -> Option<u64> {
    cursor.parse().ok()
}

/// Format a cursor value as a string.
pub fn format_cursor(cursor: u64) -> String {
    cursor.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cursor() {
        let options = CursorOptions {
            interval_seconds: 20,
            epoch: Utc::now() - chrono::Duration::seconds(100),
        };

        let cursor = calculate_cursor(&options);
        // Should be around 5 (100 seconds / 20 second intervals)
        assert!(cursor >= 4 && cursor <= 6);
    }

    #[test]
    fn test_generate_response_cursor_no_client() {
        let options = CursorOptions::default();
        let cursor = generate_response_cursor(None, &options);
        assert!(cursor > 0);
    }

    #[test]
    fn test_generate_response_cursor_client_behind() {
        let options = CursorOptions {
            interval_seconds: 20,
            epoch: Utc::now() - chrono::Duration::seconds(1000),
        };

        let current = calculate_cursor(&options);
        let result = generate_response_cursor(Some(current - 10), &options);

        // Should return at least the current interval
        assert!(result >= current - 10);
    }

    #[test]
    fn test_generate_response_cursor_client_ahead() {
        let options = CursorOptions::default();
        let client_cursor = calculate_cursor(&options) + 100;

        let result = generate_response_cursor(Some(client_cursor), &options);

        // Should be strictly greater than client cursor due to jitter
        assert!(result > client_cursor);
    }

    #[test]
    fn test_parse_format_cursor() {
        assert_eq!(parse_cursor("12345"), Some(12345));
        assert_eq!(parse_cursor("invalid"), None);
        assert_eq!(format_cursor(12345), "12345");
    }
}
