use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as JsonValue;

/// Recursively parse JSON strings in a map
fn parse_json_values(map: &HashMap<String, String>) -> HashMap<String, JsonValue> {
    map.iter()
        .map(|(k, v)| {
            let value = if let Ok(parsed) = serde_json::from_str::<JsonValue>(v) {
                // Successfully parsed as JSON, recursively unpack if it's an object
                match parsed {
                    JsonValue::Object(_) | JsonValue::Array(_) => parsed,
                    _ => JsonValue::String(v.clone()),
                }
            } else {
                // Not valid JSON, keep as string
                JsonValue::String(v.clone())
            };
            (k.clone(), value)
        })
        .collect()
}

/// Recursively stringify JSON values back to strings with consistent formatting
fn stringify_json_values(map: &HashMap<String, JsonValue>) -> HashMap<String, String> {
    map.iter()
        .map(|(k, v)| {
            let string_value = match v {
                JsonValue::String(s) => s.clone(),
                // Use compact JSON encoding (no whitespace) for consistency
                _ => serde_json::to_string(v).unwrap_or_else(|_| v.to_string()),
            };
            (k.clone(), string_value)
        })
        .collect()
}

/// Normalize a metadata map for comparison by parsing and re-serializing JSON values
pub fn normalize_metadata_for_comparison(map: &HashMap<String, String>) -> HashMap<String, String> {
    stringify_json_values(&parse_json_values(map))
}

/// Custom serialization for metadata
fn serialize_metadata<S>(
    metadata: &HashMap<String, String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let parsed = parse_json_values(metadata);
    parsed.serialize(serializer)
}

/// Custom deserialization for metadata - handles both JSON strings and unpacked YAML structures
fn deserialize_metadata<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // Deserialize as a generic map of string keys to any JSON value
    let parsed: HashMap<String, JsonValue> = HashMap::deserialize(deserializer)?;

    // Convert all values to strings:
    // - If it's already a string, keep it as-is
    // - If it's a complex structure (object/array), serialize it to compact JSON
    let result = parsed
        .into_iter()
        .map(|(key, value)| {
            let string_value = match value {
                JsonValue::String(s) => s,
                _ => serde_json::to_string(&value).map_err(|e| {
                    D::Error::custom(format!("Failed to serialize metadata value: {}", e))
                })?,
            };
            Ok((key, string_value))
        })
        .collect::<Result<HashMap<String, String>, D::Error>>()?;

    Ok(result)
}

/// Price information for a product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    /// Unique identifier for the price (base58-encoded)
    pub id: String,

    /// External provider ID (e.g., Stripe price ID)
    pub external_id: Option<String>,

    /// Sandbox provider ID (e.g., Stripe sandbox price ID)
    pub sandbox_external_id: Option<String>,

    /// Whether the price is currently active
    pub active: bool,

    /// Three-letter ISO currency code (e.g., "usd")
    pub currency: String,

    /// The unit amount (in cents for currencies like USD)
    pub unit_amount: Option<i64>,

    /// Pricing type: "one_time" or "recurring"
    pub pricing_type: String,

    /// Recurring interval (e.g., "month", "year") if applicable
    pub recurring_interval: Option<String>,

    /// Recurring interval count (e.g., 3 for "every 3 months")
    pub recurring_interval_count: Option<i64>,

    /// Nickname for the price
    pub nickname: Option<String>,

    /// Additional metadata
    #[serde(
        serialize_with = "serialize_metadata",
        deserialize_with = "deserialize_metadata"
    )]
    pub metadata: HashMap<String, String>,

    /// When the price was created
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

/// MoneyMQ Product - provider-agnostic product representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    /// Unique identifier for the product (base58-encoded)
    pub id: String,

    /// External provider ID (e.g., Stripe product ID)
    pub external_id: Option<String>,

    /// Sandbox provider ID (e.g., Stripe sandbox product ID)
    pub sandbox_external_id: Option<String>,

    /// Product name
    pub name: Option<String>,

    /// Product description
    pub description: Option<String>,

    /// Whether the product is currently active/available
    pub active: bool,

    /// Additional metadata as key-value pairs
    /// JSON values are automatically unpacked when serializing to YAML
    #[serde(
        serialize_with = "serialize_metadata",
        deserialize_with = "deserialize_metadata"
    )]
    pub metadata: HashMap<String, String>,

    /// When the product was created
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,

    /// When the product was last updated
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub updated_at: Option<DateTime<Utc>>,

    /// Product type or category
    pub product_type: Option<String>,

    /// Images associated with the product
    pub images: Vec<String>,

    /// Statement descriptor (appears on customer's credit card statement)
    pub statement_descriptor: Option<String>,

    /// Unit label (e.g., "per user", "per month")
    pub unit_label: Option<String>,

    /// Prices associated with this product
    #[serde(default)]
    pub prices: Vec<Price>,
}

/// Metadata for products
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductMetadata {
    /// Custom key-value pairs
    pub data: HashMap<String, String>,
}

/// Catalog of products
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    /// List of products
    pub products: Vec<Product>,

    /// Total number of products
    pub total_count: usize,

    /// Provider source (e.g., "stripe", "stripe_sandbox")
    pub provider: String,

    /// When the catalog was fetched
    #[serde(with = "chrono::serde::ts_seconds")]
    pub fetched_at: DateTime<Utc>,
}

impl Catalog {
    /// Create a new catalog
    pub fn new(products: Vec<Product>, provider: String) -> Self {
        let total_count = products.len();
        Self {
            products,
            total_count,
            provider,
            fetched_at: Utc::now(),
        }
    }
}
