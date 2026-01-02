use chrono::{DateTime, Utc};

/// Generate a mock Stripe ID with the given prefix
///
/// # Arguments
/// * `prefix` - The Stripe resource prefix (e.g., "cus", "pm", "sub", "si", "in", "bmes")
///
/// # Returns
/// A mock Stripe ID in the format `{prefix}_{24_char_uuid}`
///
/// # Example
/// ```
/// let customer_id = generate_stripe_id("cus");
/// // Returns something like: "cus_a1b2c3d4e5f6g7h8i9j0k1l2"
/// ```
pub fn generate_stripe_id(prefix: &str) -> String {
    let uuid_str = uuid::Uuid::new_v4().to_string().replace("-", "");
    format!("{}_{}", prefix, &uuid_str[..24])
}

/// Convert an optional Unix timestamp to a DateTime<Utc>, or return current time if None or invalid
///
/// # Arguments
/// * `timestamp` - Optional Unix timestamp in seconds
///
/// # Returns
/// A DateTime<Utc> created from the timestamp, or Utc::now() if timestamp is None or invalid
///
/// # Example
/// ```
/// let created_at = timestamp_to_datetime(Some(1234567890));
/// let now = timestamp_to_datetime(None);
/// ```
pub fn timestamp_to_datetime(timestamp: Option<i64>) -> DateTime<Utc> {
    timestamp
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_stripe_id() {
        let customer_id = generate_stripe_id("cus");
        assert!(customer_id.starts_with("cus_"));
        assert_eq!(customer_id.len(), 4 + 24); // "cus_" + 24 chars

        let sub_id = generate_stripe_id("sub");
        assert!(sub_id.starts_with("sub_"));
        assert_eq!(sub_id.len(), 4 + 24);
    }

    #[test]
    fn test_generate_stripe_id_uniqueness() {
        let id1 = generate_stripe_id("test");
        let id2 = generate_stripe_id("test");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_timestamp_to_datetime() {
        // Test with valid timestamp
        let dt = timestamp_to_datetime(Some(1234567890));
        assert_eq!(dt.timestamp(), 1234567890);

        // Test with None returns current time (within reasonable range)
        let now_before = Utc::now();
        let dt_none = timestamp_to_datetime(None);
        let now_after = Utc::now();
        assert!(dt_none >= now_before && dt_none <= now_after);
    }
}
