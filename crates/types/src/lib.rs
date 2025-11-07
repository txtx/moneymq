use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as JsonValue;

pub mod x402;

/// Recursively parse JSON strings in a map
fn parse_json_values(map: &IndexMap<String, String>) -> IndexMap<String, JsonValue> {
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
fn stringify_json_values(map: &IndexMap<String, JsonValue>) -> IndexMap<String, String> {
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
pub fn normalize_metadata_for_comparison(
    map: &IndexMap<String, String>,
) -> IndexMap<String, String> {
    stringify_json_values(&parse_json_values(map))
}

/// Custom serialization for metadata
fn serialize_metadata<S>(
    metadata: &IndexMap<String, String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let parsed = parse_json_values(metadata);
    parsed.serialize(serializer)
}

/// Custom deserialization for metadata - handles both JSON strings and unpacked YAML structures
fn deserialize_metadata<'de, D>(deserializer: D) -> Result<IndexMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // Deserialize as a generic map of string keys to any JSON value
    let parsed: IndexMap<String, JsonValue> = IndexMap::deserialize(deserializer)?;

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
        .collect::<Result<IndexMap<String, String>, D::Error>>()?;

    Ok(result)
}

/// Sandbox information for a price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceSandbox {
    /// Provider ID for this sandbox environment
    pub id: String,
}

/// Price information for a product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    /// Unique identifier for the price (base58-encoded hash of deployed_id)
    pub id: String,

    /// Deployed ID (e.g., Stripe price ID for production deployment)
    pub deployed_id: Option<String>,

    /// Sandbox deployment IDs (e.g., Stripe sandbox price IDs)
    /// Key is the sandbox name (e.g., "default", "staging")
    /// Value is the deployed ID for that sandbox
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sandboxes: IndexMap<String, String>,

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
    pub metadata: IndexMap<String, String>,

    /// When the price was created
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

impl Price {
    pub fn new(currency: String, pricing_type: String) -> Self {
        Self {
            id: random_id(),
            deployed_id: None,
            sandboxes: IndexMap::new(),
            active: true,
            currency,
            unit_amount: None,
            pricing_type,
            recurring_interval: None,
            recurring_interval_count: None,
            nickname: None,
            metadata: IndexMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Set the unit amount
    pub fn with_some_amount(mut self, amount: Option<i64>) -> Self {
        self.unit_amount = amount;
        self
    }

    /// Set the recurring interval
    pub fn with_some_interval(mut self, interval: Option<String>) -> Self {
        self.recurring_interval = interval;
        self
    }

    /// Set the recurring interval count
    pub fn with_some_interval_count(mut self, interval_count: Option<i64>) -> Self {
        self.recurring_interval_count = interval_count;
        self
    }

    /// Get the provider ID for a given sandbox name ("default" for primary sandbox)
    pub fn get_sandbox_id(&self, sandbox_name: &str) -> Option<&String> {
        self.sandboxes.get(sandbox_name)
    }

    /// Set the provider ID for a given sandbox
    pub fn set_sandbox_id(&mut self, sandbox_name: String, id: String) {
        self.sandboxes.insert(sandbox_name, id);
    }

    /// Check if price has a sandbox with the given name
    pub fn has_sandbox(&self, sandbox_name: &str) -> bool {
        self.sandboxes.contains_key(sandbox_name)
    }
}

/// MoneyMQ Product - provider-agnostic product representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    /// Unique identifier for the product (base58-encoded hash of deployed_id)
    pub id: String,

    /// Deployed ID (e.g., Stripe product ID for production deployment)
    pub deployed_id: Option<String>,

    /// Sandbox deployment IDs (e.g., Stripe sandbox product IDs)
    /// Key is the sandbox name (e.g., "default", "staging")
    /// Value is the deployed ID for that sandbox
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sandboxes: IndexMap<String, String>,

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
    pub metadata: IndexMap<String, String>,

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

fn random_id() -> String {
    use rand::Rng;
    use rand::distr::Alphanumeric;
    let id: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();
    id
}

impl Product {
    pub fn new() -> Self {
        Self {
            id: random_id(),
            deployed_id: None,
            sandboxes: IndexMap::new(),
            name: None,
            description: None,
            active: true,
            metadata: IndexMap::new(),
            created_at: Utc::now(),
            updated_at: None,
            product_type: None,
            images: vec![],
            statement_descriptor: None,
            unit_label: None,
            prices: vec![],
        }
    }

    /// Set the product name
    pub fn with_some_name(mut self, name: Option<String>) -> Self {
        self.name = name;
        self
    }

    /// Set the product description
    pub fn with_some_description(mut self, description: Option<String>) -> Self {
        self.description = description;
        self
    }

    /// Set the product type
    pub fn with_some_product_type(mut self, product_type: Option<String>) -> Self {
        self.product_type = product_type;
        self
    }

    /// Set the statement descriptor
    pub fn with_some_statement_descriptor(mut self, statement_descriptor: Option<String>) -> Self {
        self.statement_descriptor = statement_descriptor;
        self
    }

    /// Set the unit label
    pub fn with_some_unit_label(mut self, unit_label: Option<String>) -> Self {
        self.unit_label = unit_label;
        self
    }

    pub fn add_price(mut self, price: Price) -> Self {
        self.prices.push(price);
        self
    }

    /// Get the provider ID for a given sandbox name ("default" for primary sandbox)
    pub fn get_sandbox_id(&self, sandbox_name: &str) -> Option<&String> {
        self.sandboxes.get(sandbox_name)
    }

    /// Set the provider ID for a given sandbox
    pub fn set_sandbox_id(&mut self, sandbox_name: String, id: String) {
        self.sandboxes.insert(sandbox_name, id);
    }

    /// Check if product has a sandbox with the given name
    pub fn has_sandbox(&self, sandbox_name: &str) -> bool {
        self.sandboxes.contains_key(sandbox_name)
    }
}

/// Metadata for products
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductMetadata {
    /// Custom key-value pairs
    pub data: IndexMap<String, String>,
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

/// Meter event - provider-agnostic meter representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meter {
    /// Unique identifier for the meter (base58-encoded hash of deployed_id)
    pub id: String,

    /// Deployed ID (e.g., Stripe meter ID for production deployment)
    pub deployed_id: Option<String>,

    /// Sandbox deployment IDs (e.g., Stripe sandbox meter IDs)
    /// Key is the sandbox name (e.g., "default", "staging")
    /// Value is the deployed ID for that sandbox
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sandboxes: IndexMap<String, String>,

    /// Display name for the meter
    pub display_name: Option<String>,

    /// Event name that this meter tracks
    pub event_name: String,

    /// Status of the meter (e.g., "active", "inactive")
    pub status: Option<String>,

    /// Customer mapping for the meter
    pub customer_mapping: Option<MeterCustomerMapping>,

    /// Default aggregation settings
    pub default_aggregation: Option<MeterAggregation>,

    /// Value settings for the meter
    pub value_settings: Option<MeterValueSettings>,

    /// When the meter was created
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,

    /// When the meter was last updated
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl Meter {
    /// Get the deployed ID for a given sandbox name ("default" for primary sandbox)
    pub fn get_sandbox_id(&self, sandbox_name: &str) -> Option<&String> {
        self.sandboxes.get(sandbox_name)
    }

    /// Set the deployed ID for a given sandbox
    pub fn set_sandbox_id(&mut self, sandbox_name: String, id: String) {
        self.sandboxes.insert(sandbox_name, id);
    }

    /// Check if meter has a sandbox with the given name
    pub fn has_sandbox(&self, sandbox_name: &str) -> bool {
        self.sandboxes.contains_key(sandbox_name)
    }
}

/// Customer mapping configuration for a meter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterCustomerMapping {
    /// Type of customer mapping (e.g., "by_id")
    pub mapping_type: String,

    /// Event payload key containing the customer identifier
    pub event_payload_key: String,
}

/// Aggregation settings for a meter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterAggregation {
    /// Formula for aggregation (e.g., "sum", "count")
    pub formula: String,
}

/// Value settings for meter events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterValueSettings {
    /// Event payload key containing the value to aggregate
    pub event_payload_key: String,
}

/// Collection of meters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterCollection {
    /// List of meters
    pub meters: Vec<Meter>,

    /// Total number of meters
    pub total_count: usize,

    /// Provider source (e.g., "stripe", "stripe_sandbox")
    pub provider: String,

    /// When the collection was fetched
    #[serde(with = "chrono::serde::ts_seconds")]
    pub fetched_at: DateTime<Utc>,
}

impl MeterCollection {
    /// Create a new meter collection
    pub fn new(meters: Vec<Meter>, provider: String) -> Self {
        let total_count = meters.len();
        Self {
            meters,
            total_count,
            provider,
            fetched_at: Utc::now(),
        }
    }
}
