use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as JsonValue;

pub mod accounts;
pub mod iac;
pub mod stripe;
pub mod x402;

// Re-export account types
pub use accounts::{
    AccountConfig, AccountRole, AccountsConfig, AccountsConfigExt, Base58Keychain, FanoutRecipient,
    FanoutRole, Keychain, OperatedRole, OperatorRole, PayoutRole, TurnkeyKeychain,
    load_accounts_from_dir, to_snake_case,
};
// Re-export commonly used IAC types at crate root for convenience
pub use iac::{
    // Schema types (for JSON/API)
    Currency,
    DiagnosticSeverity,
    MetadataField,
    MetadataFieldSchema,
    MetadataValue,
    OverageConfig,
    PriceDefaults,
    PriceSchema,
    PricingType,
    // YAML parsing types (for product.yaml and variants/*.yaml)
    ProductBase,
    ProductSchema,
    ProductVariant,
    RecurringConfig,
    RecurringInterval,
    TrialConfig,
    ValidationDiagnostic,
    ValidationResult,
    // Consolidation
    consolidate_products,
    // Deep merge utilities
    deep_merge_json,
    merge_product_with_variant,
};

/// Default manifest file name
pub const MANIFEST_FILE_NAME: &str = "moneymq.yaml";

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

    /// Currency code
    pub currency: iac::Currency,

    /// The unit amount (in cents for currencies like USD)
    pub unit_amount: Option<i64>,

    /// Pricing type: one_time or recurring
    pub pricing_type: iac::PricingType,

    /// Recurring interval (day, week, month, year) if applicable
    pub recurring_interval: Option<iac::RecurringInterval>,

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
    pub fn new(currency: iac::Currency, pricing_type: iac::PricingType) -> Self {
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
    pub fn with_some_interval(mut self, interval: Option<iac::RecurringInterval>) -> Self {
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

    /// Product features with their values
    /// Keys are feature identifiers, values contain name, description, and value
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub features: IndexMap<String, ProductFeature>,

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

    /// Experiment configuration for A/B testing variants
    /// When set, this product inherits parent features and can override them
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment: Option<ExperimentConfig>,

    /// Parent product ID for experiment variants (derived from directory structure)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Configuration for A/B testing experiments
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentConfig {
    /// Traffic exposure percentage (0.0 to 1.0)
    /// e.g., 0.5 means 50% of traffic sees this variant
    pub exposure: f64,
}

fn random_id() -> String {
    use rand::{Rng, distr::Alphanumeric};
    let random_part: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(14)
        .map(char::from)
        .collect();
    format!("prod_{}", random_part)
}

impl Default for Product {
    fn default() -> Self {
        Self::new()
    }
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
            features: IndexMap::new(),
            created_at: Utc::now(),
            updated_at: None,
            product_type: None,
            images: vec![],
            statement_descriptor: None,
            unit_label: None,
            prices: vec![],
            experiment: None,
            parent_id: None,
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

    /// Get the base58 encoded filename for this product
    pub fn filename(&self) -> String {
        format!("{}.yaml", bs58::encode(&self.id).into_string())
    }
}

/// A product feature with its definition and value
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductFeature {
    /// Display name for the feature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Description of the feature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Feature value - can be boolean, number, or string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

impl ProductFeature {
    /// Create a new feature with name and description
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            description: Some(description.into()),
            value: None,
        }
    }

    /// Set the feature value
    pub fn with_value(mut self, value: serde_json::Value) -> Self {
        self.value = Some(value);
        self
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

/// Line item price from payment intent metadata
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineItemPrice {
    /// Product ID
    pub product: String,
    /// Experiment variant ID
    pub experiment_id: Option<String>,
}

/// Line item from payment intent metadata
#[derive(Debug, Clone, Deserialize)]
pub struct LineItem {
    /// Price details with product and experiment info
    pub price: LineItemPrice,
    /// Quantity of items
    #[serde(default = "default_quantity")]
    pub quantity: u32,
}

fn default_quantity() -> u32 {
    1
}

/// Basket item representing a product in a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasketItem {
    /// Product ID from catalog
    pub product_id: String,
    /// Experiment variant ID (e.g., "surfnet-lite#a")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    /// Product features (capabilities and limits purchased)
    #[serde(default, skip_serializing_if = "is_features_empty")]
    pub features: serde_json::Value,
    /// Quantity of items
    #[serde(default = "default_quantity")]
    pub quantity: u32,
}

fn is_features_empty(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::Object(m) => m.is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

/// Payment verification event data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentVerifiedData {
    /// Payer address
    pub payer: String,
    /// Payment amount as string
    pub amount: String,
    /// Network name
    pub network: String,
    /// Product ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
}

/// Payment settlement event data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentSettledData {
    /// Payer address
    pub payer: String,
    /// Payment amount as string
    pub amount: String,
    /// Currency code
    pub currency: String,
    /// Network name
    pub network: String,
    /// Transaction signature (for blockchain payments)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_signature: Option<String>,
    /// Product ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
}

/// Payment failure event data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentFailedData {
    /// Payer address (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payer: Option<String>,
    /// Payment amount as string
    pub amount: String,
    /// Network name
    pub network: String,
    /// Failure reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Product ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
}

/// Transaction completed event data (includes receipt JWT)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionCompletedData {
    /// Signed JWT receipt
    pub receipt: String,
}

/// Strongly-typed channel event enum
///
/// Each variant contains its own typed payload. Serializes to a CloudEvents-like format:
/// ```json
/// {"id": "...", "type": "payment:settled", "data": {...}, "time": "..."}
/// ```
#[derive(Debug, Clone)]
pub enum ChannelEvent {
    /// Payment has been verified
    PaymentVerified {
        id: String,
        time: chrono::DateTime<chrono::Utc>,
        data: PaymentVerifiedData,
    },
    /// Payment has been settled
    PaymentSettled {
        id: String,
        time: chrono::DateTime<chrono::Utc>,
        data: PaymentSettledData,
    },
    /// Payment failed (generic)
    PaymentFailed {
        id: String,
        time: chrono::DateTime<chrono::Utc>,
        data: PaymentFailedData,
    },
    /// Transaction completed with receipt
    TransactionCompleted {
        id: String,
        time: chrono::DateTime<chrono::Utc>,
        data: TransactionCompletedData,
    },
    /// Custom event type (for arbitrary events like transaction:attach)
    Custom {
        id: String,
        time: chrono::DateTime<chrono::Utc>,
        event_type: String,
        data: serde_json::Value,
    },
}

impl ChannelEvent {
    /// Create a payment:verified event
    pub fn payment_verified(data: PaymentVerifiedData) -> Self {
        Self::PaymentVerified {
            id: uuid::Uuid::new_v4().to_string(),
            time: chrono::Utc::now(),
            data,
        }
    }

    /// Create a payment:settled event
    pub fn payment_settled(data: PaymentSettledData) -> Self {
        Self::PaymentSettled {
            id: uuid::Uuid::new_v4().to_string(),
            time: chrono::Utc::now(),
            data,
        }
    }

    /// Create a payment:failed event
    pub fn payment_failed(data: PaymentFailedData) -> Self {
        Self::PaymentFailed {
            id: uuid::Uuid::new_v4().to_string(),
            time: chrono::Utc::now(),
            data,
        }
    }

    /// Create a transaction:completed event
    pub fn transaction_completed(data: TransactionCompletedData) -> Self {
        Self::TransactionCompleted {
            id: uuid::Uuid::new_v4().to_string(),
            time: chrono::Utc::now(),
            data,
        }
    }

    /// Create a custom event with arbitrary type and data
    pub fn custom(event_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self::Custom {
            id: uuid::Uuid::new_v4().to_string(),
            time: chrono::Utc::now(),
            event_type: event_type.into(),
            data,
        }
    }

    /// Get the event ID
    pub fn id(&self) -> &str {
        match self {
            Self::PaymentVerified { id, .. } => id,
            Self::PaymentSettled { id, .. } => id,
            Self::PaymentFailed { id, .. } => id,
            Self::TransactionCompleted { id, .. } => id,
            Self::Custom { id, .. } => id,
        }
    }

    /// Get the event type string
    pub fn event_type(&self) -> &str {
        match self {
            Self::PaymentVerified { .. } => "payment:verified",
            Self::PaymentSettled { .. } => "payment:settled",
            Self::PaymentFailed { .. } => "payment:failed",
            Self::TransactionCompleted { .. } => "transaction:completed",
            Self::Custom { event_type, .. } => event_type,
        }
    }

    /// Get the event timestamp
    pub fn time(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Self::PaymentVerified { time, .. } => *time,
            Self::PaymentSettled { time, .. } => *time,
            Self::PaymentFailed { time, .. } => *time,
            Self::TransactionCompleted { time, .. } => *time,
            Self::Custom { time, .. } => *time,
        }
    }

    /// Get the event data as a JSON value
    pub fn data(&self) -> serde_json::Value {
        match self {
            Self::PaymentVerified { data, .. } => serde_json::to_value(data).unwrap_or_default(),
            Self::PaymentSettled { data, .. } => serde_json::to_value(data).unwrap_or_default(),
            Self::PaymentFailed { data, .. } => serde_json::to_value(data).unwrap_or_default(),
            Self::TransactionCompleted { data, .. } => {
                serde_json::to_value(data).unwrap_or_default()
            }
            Self::Custom { data, .. } => data.clone(),
        }
    }
}

impl Serialize for ChannelEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("ChannelEvent", 4)?;
        state.serialize_field("id", self.id())?;
        state.serialize_field("type", self.event_type())?;
        state.serialize_field("time", &self.time())?;

        match self {
            Self::PaymentVerified { data, .. } => {
                state.serialize_field("data", data)?;
            }
            Self::PaymentSettled { data, .. } => {
                state.serialize_field("data", data)?;
            }
            Self::PaymentFailed { data, .. } => {
                state.serialize_field("data", data)?;
            }
            Self::TransactionCompleted { data, .. } => {
                state.serialize_field("data", data)?;
            }
            Self::Custom { data, .. } => {
                state.serialize_field("data", data)?;
            }
        }

        state.end()
    }
}

impl<'de> Deserialize<'de> for ChannelEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawEvent {
            id: String,
            #[serde(rename = "type")]
            event_type: String,
            time: chrono::DateTime<chrono::Utc>,
            data: serde_json::Value,
        }

        let raw = RawEvent::deserialize(deserializer)?;

        match raw.event_type.as_str() {
            "payment:verified" => {
                let data: PaymentVerifiedData =
                    serde_json::from_value(raw.data).map_err(serde::de::Error::custom)?;
                Ok(Self::PaymentVerified {
                    id: raw.id,
                    time: raw.time,
                    data,
                })
            }
            "payment:settled" => {
                let data: PaymentSettledData =
                    serde_json::from_value(raw.data).map_err(serde::de::Error::custom)?;
                Ok(Self::PaymentSettled {
                    id: raw.id,
                    time: raw.time,
                    data,
                })
            }
            "payment:failed" => {
                let data: PaymentFailedData =
                    serde_json::from_value(raw.data).map_err(serde::de::Error::custom)?;
                Ok(Self::PaymentFailed {
                    id: raw.id,
                    time: raw.time,
                    data,
                })
            }
            "transaction:completed" => {
                let data: TransactionCompletedData =
                    serde_json::from_value(raw.data).map_err(serde::de::Error::custom)?;
                Ok(Self::TransactionCompleted {
                    id: raw.id,
                    time: raw.time,
                    data,
                })
            }
            _ => Ok(Self::Custom {
                id: raw.id,
                time: raw.time,
                event_type: raw.event_type,
                data: raw.data,
            }),
        }
    }
}

/// Event type constants for backward compatibility and pattern matching
pub mod event_types {
    /// Payment has been verified
    pub const PAYMENT_VERIFIED: &str = "payment:verified";

    /// Payment has been settled
    pub const PAYMENT_SETTLED: &str = "payment:settled";

    /// Payment verification failed
    pub const PAYMENT_VERIFICATION_FAILED: &str = "payment:verification_failed";

    /// Payment settlement failed
    pub const PAYMENT_SETTLEMENT_FAILED: &str = "payment:settlement_failed";

    /// Payment failed (generic)
    pub const PAYMENT_FAILED: &str = "payment:failed";

    /// New transaction received (for processors)
    pub const TRANSACTION: &str = "transaction";

    /// Processor attaching data to transaction
    pub const TRANSACTION_ATTACH: &str = "transaction:attach";

    /// Transaction completed with receipt
    pub const TRANSACTION_COMPLETED: &str = "transaction:completed";
}

/// Payment defaults
pub mod defaults {
    /// Default JWT expiration time in hours
    pub const JWT_EXPIRATION_HOURS: u64 = 24;

    /// Default currency code
    pub const CURRENCY: &str = "USDC";

    /// Default network (lowercase)
    pub const NETWORK: &str = "solana";
}
