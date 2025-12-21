//! Infrastructure as Code (IAC) schema types for MoneyMQ.
//!
//! These types define the structure for product catalogs, pricing,
//! meters, and deployment configurations. They are used by both
//! the CLI (for IAC endpoints) and MCP (for LLM tool calls).

use indexmap::IndexMap;
#[cfg(feature = "schemars")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// Enum Types
// ============================================================================

/// Supported blockchain networks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum Chain {
    Solana,
}

/// Supported stablecoins
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum Stablecoin {
    /// USD Coin
    USDC,
}

/// Deployment type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum DeploymentType {
    /// Local development with embedded validator
    Sandbox,
    /// Self-hosted infrastructure
    SelfHosted,
    /// Hosted by moneymq.co
    CloudHosted,
}

/// Key management strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum KeyManagement {
    /// Keys stored in memory (development only)
    InMemory,
    /// Managed key custody service
    TurnKey,
}

/// Catalog source types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Stripe,
}

/// Currency codes for billing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Currency {
    /// US Dollar
    Usd,
    /// Euro
    Eur,
    /// British Pound
    Gbp,
}

impl Currency {
    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "usd" => Some(Currency::Usd),
            "eur" => Some(Currency::Eur),
            "gbp" => Some(Currency::Gbp),
            _ => None,
        }
    }

    /// Get all valid values as a string (for validation messages)
    pub fn valid_values() -> &'static str {
        "'usd', 'eur', 'gbp'"
    }

    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Currency::Usd => "usd",
            Currency::Eur => "eur",
            Currency::Gbp => "gbp",
        }
    }
}

/// Pricing type for products
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PricingType {
    /// Single payment
    OneTime,
    /// Recurring subscription
    Recurring,
}

impl PricingType {
    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "one_time" | "onetime" => Some(PricingType::OneTime),
            "recurring" => Some(PricingType::Recurring),
            _ => None,
        }
    }

    /// Get all valid values as a string (for validation messages)
    pub fn valid_values() -> &'static str {
        "'one_time', 'recurring'"
    }

    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            PricingType::OneTime => "one_time",
            PricingType::Recurring => "recurring",
        }
    }
}

/// Recurring interval for subscriptions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RecurringInterval {
    Day,
    Week,
    Month,
    Year,
}

impl RecurringInterval {
    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "day" => Some(RecurringInterval::Day),
            "week" => Some(RecurringInterval::Week),
            "month" => Some(RecurringInterval::Month),
            "year" => Some(RecurringInterval::Year),
            _ => None,
        }
    }

    /// Get all valid values as a string (for validation messages)
    pub fn valid_values() -> &'static str {
        "'day', 'week', 'month', 'year'"
    }

    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            RecurringInterval::Day => "day",
            RecurringInterval::Week => "week",
            RecurringInterval::Month => "month",
            RecurringInterval::Year => "year",
        }
    }
}

/// Meter aggregation formula
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum AggregationFormula {
    /// Sum of all values
    Sum,
    /// Count of events
    Count,
    /// Maximum value
    Max,
    /// Last value
    Last,
}

// ============================================================================
// Product/Price Schema Types
// ============================================================================

/// Product schema for catalog items
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct ProductSchema {
    /// Unique product identifier
    #[serde(default)]
    pub id: String,

    /// Product name (required for variants, optional for base products)
    #[serde(default)]
    pub name: String,

    /// Product description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether the product is active. Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Product type (e.g., "service", "good")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,

    /// Statement descriptor (appears on credit card statements)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_descriptor: Option<String>,

    /// Unit label (e.g., "per seat", "per GB")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_label: Option<String>,

    /// Product images URLs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,

    /// Custom metadata - can contain strings, arrays, or nested objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<IndexMap<String, serde_json::Value>>,

    /// Prices for this product (array for variants, object schema for base products)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prices: Option<Vec<PriceSchema>>,

    /// Source filename (without extension) - used to track which YAML file this came from.
    /// When saving, this determines the target file. If not set, uses the product ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _source_file: Option<String>,

    /// Product directory name - identifies which product group this belongs to.
    /// Base products and their variants share the same _product_dir.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _product_dir: Option<String>,

    /// Variant name - if set, this is a variant of the base product.
    /// Base products don't have this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _variant: Option<String>,
}

impl ProductSchema {
    /// Check if this is a base/template product (has _product_dir but no _variant)
    pub fn is_base_product(&self) -> bool {
        self._product_dir.is_some() && self._variant.is_none()
    }

    /// Check if this is a variant product (has both _product_dir and _variant)
    pub fn is_variant(&self) -> bool {
        self._variant.is_some()
    }

    /// Check if this product has valid prices (non-empty array)
    pub fn has_prices(&self) -> bool {
        self.prices.as_ref().map(|p| !p.is_empty()).unwrap_or(false)
    }
}

/// Price schema - defines how a product is priced
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(tag = "pricing_type")]
pub enum PriceSchema {
    /// One-time payment price
    #[serde(rename = "one_time")]
    OneTime(OneTimePriceSchema),
    /// Recurring subscription price
    #[serde(rename = "recurring")]
    Recurring(RecurringPriceSchema),
}

/// One-time price schema
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct OneTimePriceSchema {
    /// Unique price identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Currency code (required)
    pub currency: Currency,

    /// Amount in smallest currency unit (e.g., cents). Required for fixed prices
    pub unit_amount: i64,

    /// Whether the price is active. Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Price nickname for internal reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

    /// Custom metadata - can contain strings, arrays, or nested objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<IndexMap<String, serde_json::Value>>,
}

/// Recurring subscription price schema
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct RecurringPriceSchema {
    /// Unique price identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Currency code (required)
    pub currency: Currency,

    /// Amount in smallest currency unit (e.g., cents). Required for fixed prices
    pub unit_amount: i64,

    /// Billing interval (required)
    pub interval: RecurringInterval,

    /// Number of intervals between billings. Default: 1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_count: Option<i64>,

    /// Whether the price is active. Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Price nickname for internal reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

    /// Custom metadata - can contain strings, arrays, or nested objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<IndexMap<String, serde_json::Value>>,
}

// ============================================================================
// YAML Parsing Types (for product.yaml and variants/*.yaml files)
// ============================================================================

/// Base product definition from product.yaml
/// Contains common fields and metadata schema (without values)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct ProductBase {
    /// Product type (e.g., "service", "good")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,

    /// Unit label (e.g., "per network", "per user")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_label: Option<String>,

    /// Product images
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,

    /// Whether the product is active
    #[serde(default = "default_true")]
    pub active: bool,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,

    /// Metadata schema - defines available metadata fields with name/description
    /// Variants provide the actual values
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub metadata: IndexMap<String, MetadataField>,

    /// Base price configuration (can be overridden by variants)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prices: Option<PriceDefaults>,
}

fn default_true() -> bool {
    true
}

/// Product variant definition from variants/*.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct ProductVariant {
    /// Optional explicit ID (if not provided, generated from path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Product name (required)
    pub name: String,

    /// Product description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Statement descriptor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_descriptor: Option<String>,

    /// Override active status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Metadata values - key -> { value: X } or direct value
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub metadata: IndexMap<String, MetadataValue>,

    /// Prices for this variant
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prices: Vec<VariantPrice>,
}

/// Metadata field schema definition in base product
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(untagged)]
pub enum MetadataField {
    /// Simple schema with name/description
    Schema(MetadataFieldSchema),
    /// Array of schema items (for grouped features)
    SchemaList(Vec<MetadataFieldSchema>),
}

/// Schema definition for a metadata field
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct MetadataFieldSchema {
    /// Optional key identifier (for array items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
}

/// Metadata value in a variant
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(untagged)]
pub enum MetadataValue {
    /// Object with value field: { value: X }
    WithValue { value: serde_json::Value },
    /// Direct value (for simple cases)
    Direct(serde_json::Value),
}

impl MetadataValue {
    /// Get the underlying value
    pub fn get_value(&self) -> &serde_json::Value {
        match self {
            MetadataValue::WithValue { value } => value,
            MetadataValue::Direct(v) => v,
        }
    }
}

/// Default price configuration from product.yaml base
/// Used to set defaults that variants can override
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct PriceDefaults {
    /// Default pricing type (one_time or recurring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_type: Option<PricingType>,

    /// Default currency
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<Currency>,

    /// Default recurring interval
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring_interval: Option<RecurringInterval>,

    /// Default recurring interval count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring_interval_count: Option<i64>,
}

/// Price definition in a variant
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct VariantPrice {
    /// Optional explicit price ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Currency (overrides base default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<Currency>,

    /// Unit amount in cents
    pub unit_amount: i64,

    /// Pricing type (overrides base default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_type: Option<PricingType>,

    /// Active status
    #[serde(default = "default_true")]
    pub active: bool,

    /// Recurring interval (overrides base default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring_interval: Option<RecurringInterval>,

    /// Recurring interval count (overrides base default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring_interval_count: Option<i64>,
}

// ============================================================================
// Deep Merge Utilities
// ============================================================================

/// Deep merge two JSON values.
/// The overlay value takes precedence over the base value.
/// For objects, keys from both are merged recursively.
/// For other types, overlay completely replaces base.
pub fn deep_merge_json(base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;

    match (base, overlay) {
        // Both are objects: merge recursively
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                let merged = if let Some(base_value) = base_map.remove(&key) {
                    deep_merge_json(base_value, overlay_value)
                } else {
                    overlay_value
                };
                base_map.insert(key, merged);
            }
            Value::Object(base_map)
        }
        // Overlay is not null: use overlay
        (_, overlay) if !overlay.is_null() => overlay,
        // Overlay is null: keep base
        (base, _) => base,
    }
}

/// Merge a base product YAML with a variant YAML.
/// Both are parsed as serde_json::Value, deep merged (variant overrides base),
/// and then the ID is generated from the directory/variant name.
///
/// # Arguments
/// * `base_yaml` - The base product.yaml content as a string
/// * `variant_yaml` - The variant product.yaml content as a string
/// * `product_dir` - The product directory name (e.g., "surfnet")
/// * `variant_name` - The variant name (e.g., "pro")
///
/// # Returns
/// A Result containing the merged JSON value with the generated ID
pub fn merge_product_with_variant(
    base_yaml: &str,
    variant_yaml: &str,
    product_dir: &str,
    variant_name: &str,
) -> Result<serde_json::Value, String> {
    use serde_json::Value;

    // Parse base YAML to JSON
    let base: Value = serde_yml::from_str(base_yaml)
        .map_err(|e| format!("Failed to parse base product.yaml: {}", e))?;

    // Parse variant YAML to JSON
    let variant: Value = serde_yml::from_str(variant_yaml)
        .map_err(|e| format!("Failed to parse variant product.yaml: {}", e))?;

    // Deep merge: variant overrides base
    let mut merged = deep_merge_json(base, variant);

    // Generate ID from path if not explicitly set
    if let Value::Object(ref mut map) = merged {
        let has_id = map
            .get("id")
            .map(|v| !v.is_null() && v != "")
            .unwrap_or(false);
        if !has_id {
            let generated_id = format!("{}-{}", product_dir, variant_name);
            map.insert("id".to_string(), Value::String(generated_id));
        }
    }

    Ok(merged)
}

// ============================================================================
// Meter Schema Types
// ============================================================================

/// Meter schema for usage-based billing
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct MeterSchema {
    /// Unique meter identifier
    pub id: String,

    /// Display name for the meter
    pub display_name: String,

    /// Event name that triggers this meter (required)
    pub event_name: String,

    /// Meter status. Default: active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Customer mapping configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_mapping: Option<CustomerMappingSchema>,

    /// Aggregation settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<AggregationSchema>,

    /// Value settings for aggregation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_settings: Option<ValueSettingsSchema>,
}

/// Customer mapping for meters
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CustomerMappingSchema {
    /// Mapping type (e.g., "by_id")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_type: Option<String>,

    /// Event payload key containing customer identifier
    pub event_payload_key: String,
}

/// Aggregation settings for meters
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct AggregationSchema {
    /// Aggregation formula
    pub formula: AggregationFormula,
}

/// Value settings for meter aggregation
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct ValueSettingsSchema {
    /// Event payload key containing the value to aggregate
    pub event_payload_key: String,
}

// ============================================================================
// Catalog Schema
// ============================================================================

/// Complete catalog schema with products and meters
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CatalogSchema {
    /// Catalog description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Base path for catalog data files. Default: "billing/v1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_path: Option<String>,

    /// External source type (e.g., stripe for sync)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceType>,

    /// Products in this catalog
    #[serde(skip_serializing_if = "Option::is_none")]
    pub products: Option<Vec<ProductSchema>>,

    /// Meters for usage-based billing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meters: Option<Vec<MeterSchema>>,
}

// ============================================================================
// Validation
// ============================================================================

/// Severity level for validation diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// A single validation diagnostic message
#[derive(Debug, Clone, Serialize)]
pub struct ValidationDiagnostic {
    /// Unique rule identifier
    pub rule: String,
    /// Human-readable message
    pub message: String,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Field path where the issue was found
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Expected value/type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// Received value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received: Option<String>,
    /// Suggestion for how to fix the issue
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl ValidationDiagnostic {
    pub fn error(rule: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            severity: DiagnosticSeverity::Error,
            field: None,
            expected: None,
            received: None,
            suggestion: None,
        }
    }

    pub fn warning(rule: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            severity: DiagnosticSeverity::Warning,
            field: None,
            expected: None,
            received: None,
            suggestion: None,
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn with_expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    pub fn with_received(mut self, received: impl Into<String>) -> Self {
        self.received = Some(received.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Result of validation with diagnostics
#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub diagnostics: Vec<ValidationDiagnostic>,
    pub error_count: usize,
    pub warning_count: usize,
    pub is_valid: bool,
}

impl ValidationResult {
    pub fn from_diagnostics(diagnostics: Vec<ValidationDiagnostic>) -> Self {
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .count();

        Self {
            diagnostics,
            error_count,
            warning_count,
            is_valid: error_count == 0,
        }
    }

    pub fn format_for_llm(&self) -> String {
        if self.is_valid && self.warning_count == 0 {
            return String::new();
        }

        let mut output = String::new();
        output.push_str("\n## Validation Results\n\n");

        if !self.is_valid {
            output.push_str(&format!(
                "❌ **{} error(s) found** - Please fix these issues and try again:\n\n",
                self.error_count
            ));
        }

        for diagnostic in &self.diagnostics {
            let icon = match diagnostic.severity {
                DiagnosticSeverity::Error => "🔴",
                DiagnosticSeverity::Warning => "🟡",
                DiagnosticSeverity::Info => "🔵",
            };

            output.push_str(&format!(
                "{} **[{}]** {}\n",
                icon, diagnostic.rule, diagnostic.message
            ));

            if let Some(field) = &diagnostic.field {
                output.push_str(&format!("   - **Field:** `{}`\n", field));
            }
            if let Some(expected) = &diagnostic.expected {
                output.push_str(&format!("   - **Expected:** {}\n", expected));
            }
            if let Some(received) = &diagnostic.received {
                output.push_str(&format!("   - **Received:** {}\n", received));
            }
            if let Some(suggestion) = &diagnostic.suggestion {
                output.push_str(&format!("   - **Suggestion:** {}\n", suggestion));
            }
            output.push('\n');
        }

        output
    }
}

// ============================================================================
// Product Consolidation
// ============================================================================

/// Consolidate IAC products: merge base product attributes with their variants.
///
/// This function takes a list of products that may include:
/// - Base products (have `_product_dir` but no `_variant`)
/// - Variant products (have both `_product_dir` and `_variant`)
/// - Standalone products (no `_product_dir`)
///
/// It returns only the variant products, with attributes inherited from their base products.
/// Base products are used as templates and are not included in the output.
pub fn consolidate_products(products: &[ProductSchema]) -> Vec<crate::Product> {
    use std::collections::HashMap;

    // Group products by product_dir
    let mut base_products: HashMap<String, &ProductSchema> = HashMap::new();
    let mut variants: Vec<&ProductSchema> = Vec::new();

    for product in products {
        if let Some(ref product_dir) = product._product_dir {
            if product._variant.is_some() {
                // This is a variant
                variants.push(product);
            } else {
                // This is a base product
                base_products.insert(product_dir.clone(), product);
            }
        } else {
            // No product_dir - treat as standalone product if it has prices
            if product._variant.is_none() && product.has_prices() {
                variants.push(product);
            }
        }
    }

    // Convert variants to Product, merging with base product attributes
    variants
        .iter()
        .filter_map(|variant| {
            // Find the base product for this variant
            let base = variant
                ._product_dir
                .as_ref()
                .and_then(|dir| base_products.get(dir));

            // Generate product ID from variant name: {product_dir}-{variant}
            let product_id = if !variant.id.is_empty() {
                // Use explicit ID if provided
                variant.id.clone()
            } else if let (Some(product_dir), Some(variant_name)) =
                (&variant._product_dir, &variant._variant)
            {
                // Generate from product_dir and variant: surfnet-pro
                format!("{}-{}", product_dir, variant_name)
            } else if let Some(source_file) = &variant._source_file {
                // Fallback to source_file for standalone products
                source_file.clone()
            } else {
                format!("prod_{}", rand::random::<u32>())
            };

            // Skip products without valid prices
            if !variant.has_prices() {
                return None;
            }

            // Merge metadata from base and variant
            let metadata = merge_metadata(
                base.and_then(|b| b.metadata.as_ref()),
                variant.metadata.as_ref(),
            );

            // Convert prices
            let prices: Vec<crate::Price> = variant
                .prices
                .as_ref()
                .map(|p| p.iter().cloned().map(|ps| ps.into()).collect())
                .unwrap_or_default();

            // Build the final Product
            let mut product = crate::Product::new();
            product.id = product_id;
            product.name = if variant.name.is_empty() {
                base.map(|b| b.name.clone())
            } else {
                Some(variant.name.clone())
            };
            product.description = variant
                .description
                .clone()
                .or_else(|| base.and_then(|b| b.description.clone()));
            product.active = variant.active.unwrap_or(true)
                && base.map(|b| b.active.unwrap_or(true)).unwrap_or(true);
            product.metadata = metadata;
            product.product_type = variant
                .product_type
                .clone()
                .or_else(|| base.and_then(|b| b.product_type.clone()));
            product.images = variant
                .images
                .clone()
                .or_else(|| base.and_then(|b| b.images.clone()))
                .unwrap_or_default();
            product.statement_descriptor = variant.statement_descriptor.clone();
            product.unit_label = variant
                .unit_label
                .clone()
                .or_else(|| base.and_then(|b| b.unit_label.clone()));
            product.prices = prices;

            Some(product)
        })
        .collect()
}

/// Merge metadata from base product and variant.
/// Variant metadata takes precedence over base metadata.
fn merge_metadata(
    base_metadata: Option<&IndexMap<String, serde_json::Value>>,
    variant_metadata: Option<&IndexMap<String, serde_json::Value>>,
) -> IndexMap<String, String> {
    let mut result = IndexMap::new();

    // Add base metadata first
    if let Some(base) = base_metadata {
        for (key, value) in base {
            result.insert(key.clone(), value.to_string());
        }
    }

    // Overlay variant metadata (overwrites base)
    if let Some(variant) = variant_metadata {
        for (key, value) in variant {
            result.insert(key.clone(), value.to_string());
        }
    }

    result
}

// ============================================================================
// Conversions from Schema Types to Runtime Types
// ============================================================================

impl From<PriceSchema> for crate::Price {
    fn from(schema: PriceSchema) -> Self {
        match schema {
            PriceSchema::OneTime(p) => crate::Price::new(p.currency, PricingType::OneTime)
                .with_some_amount(Some(p.unit_amount)),

            PriceSchema::Recurring(p) => crate::Price::new(p.currency, PricingType::Recurring)
                .with_some_amount(Some(p.unit_amount))
                .with_some_interval(Some(p.interval))
                .with_some_interval_count(p.interval_count),
        }
    }
}

impl From<ProductSchema> for crate::Product {
    fn from(schema: ProductSchema) -> Self {
        let mut product = crate::Product::new();
        product.id = schema.id;
        product.name = Some(schema.name);
        product.description = schema.description;
        product.active = schema.active.unwrap_or(true);
        product.product_type = schema.product_type;
        product.statement_descriptor = schema.statement_descriptor;
        product.unit_label = schema.unit_label;
        product.images = schema.images.unwrap_or_default();

        // Convert metadata from serde_json::Value to String
        if let Some(metadata) = schema.metadata {
            product.metadata = metadata
                .into_iter()
                .map(|(k, v)| (k, v.to_string()))
                .collect();
        }

        // Convert prices
        if let Some(prices) = schema.prices {
            product.prices = prices.into_iter().map(|p| p.into()).collect();
        }

        product
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_price(amount: i64) -> PriceSchema {
        PriceSchema::OneTime(OneTimePriceSchema {
            id: None,
            currency: Currency::Usd,
            unit_amount: amount,
            active: Some(true),
            nickname: None,
            metadata: None,
        })
    }

    fn make_base_product(product_dir: &str) -> ProductSchema {
        ProductSchema {
            id: String::new(),
            name: String::new(),
            description: Some("Base description".to_string()),
            active: Some(true),
            product_type: Some("service".to_string()),
            statement_descriptor: None,
            unit_label: Some("per network".to_string()),
            images: Some(vec!["base-image.png".to_string()]),
            metadata: None,
            prices: None, // Base products don't have prices array
            _source_file: Some(format!("{}/product", product_dir)),
            _product_dir: Some(product_dir.to_string()),
            _variant: None,
        }
    }

    fn make_variant(product_dir: &str, variant_name: &str, amount: i64) -> ProductSchema {
        ProductSchema {
            id: String::new(),
            name: format!("Product {}", variant_name),
            description: Some(format!("{} variant description", variant_name)),
            active: Some(true),
            product_type: None, // Inherits from base
            statement_descriptor: None,
            unit_label: None, // Inherits from base
            images: None,     // Inherits from base
            metadata: None,
            prices: Some(vec![make_price(amount)]),
            _source_file: Some(format!("{}/{}", product_dir, variant_name)),
            _product_dir: Some(product_dir.to_string()),
            _variant: Some(variant_name.to_string()),
        }
    }

    #[test]
    fn test_consolidate_single_variant() {
        let products = vec![
            make_base_product("surfnet"),
            make_variant("surfnet", "pro", 999),
        ];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 1);
        let product = &consolidated[0];
        assert_eq!(product.id, "surfnet-pro");
        assert_eq!(product.name, Some("Product pro".to_string()));
        assert_eq!(
            product.description,
            Some("pro variant description".to_string())
        );
        // Inherited from base
        assert_eq!(product.product_type, Some("service".to_string()));
        assert_eq!(product.unit_label, Some("per network".to_string()));
        assert_eq!(product.images, vec!["base-image.png".to_string()]);
        // Price from variant
        assert_eq!(product.prices.len(), 1);
        assert_eq!(product.prices[0].unit_amount, Some(999));
    }

    #[test]
    fn test_consolidate_multiple_variants() {
        let products = vec![
            make_base_product("surfnet"),
            make_variant("surfnet", "light", 399),
            make_variant("surfnet", "pro", 999),
            make_variant("surfnet", "max", 1999),
        ];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 3);

        // Check all variants are present
        let ids: Vec<&str> = consolidated.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"surfnet-light"));
        assert!(ids.contains(&"surfnet-pro"));
        assert!(ids.contains(&"surfnet-max"));
    }

    #[test]
    fn test_base_product_not_included() {
        let products = vec![
            make_base_product("surfnet"),
            make_variant("surfnet", "pro", 999),
        ];

        let consolidated = consolidate_products(&products);

        // Base product should not be in output
        for product in &consolidated {
            assert!(product.id != "surfnet/product");
            assert!(!product.id.ends_with("/product"));
        }
    }

    #[test]
    fn test_variant_without_prices_excluded() {
        let mut variant_no_prices = make_variant("surfnet", "empty", 0);
        variant_no_prices.prices = Some(vec![]); // Empty prices array

        let products = vec![
            make_base_product("surfnet"),
            variant_no_prices,
            make_variant("surfnet", "pro", 999),
        ];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 1);
        assert_eq!(consolidated[0].id, "surfnet-pro");
    }

    #[test]
    fn test_standalone_product_with_prices() {
        let standalone = ProductSchema {
            id: "standalone-product".to_string(),
            name: "Standalone Product".to_string(),
            description: Some("A standalone product".to_string()),
            active: Some(true),
            product_type: Some("good".to_string()),
            statement_descriptor: None,
            unit_label: None,
            images: None,
            metadata: None,
            prices: Some(vec![make_price(599)]),
            _source_file: Some("standalone".to_string()),
            _product_dir: None, // No product_dir = standalone
            _variant: None,
        };

        let products = vec![standalone];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 1);
        assert_eq!(consolidated[0].name, Some("Standalone Product".to_string()));
    }

    #[test]
    fn test_variant_overrides_base_description() {
        let base = make_base_product("myproduct");
        let mut variant = make_variant("myproduct", "custom", 799);
        variant.description = Some("Custom description overrides base".to_string());

        let products = vec![base, variant];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 1);
        assert_eq!(
            consolidated[0].description,
            Some("Custom description overrides base".to_string())
        );
    }

    #[test]
    fn test_variant_inherits_missing_fields() {
        let mut base = make_base_product("myproduct");
        base.product_type = Some("subscription".to_string());
        base.unit_label = Some("per user".to_string());

        let mut variant = make_variant("myproduct", "basic", 499);
        variant.product_type = None; // Should inherit
        variant.unit_label = None; // Should inherit

        let products = vec![base, variant];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 1);
        assert_eq!(
            consolidated[0].product_type,
            Some("subscription".to_string())
        );
        assert_eq!(consolidated[0].unit_label, Some("per user".to_string()));
    }

    #[test]
    fn test_metadata_merge() {
        let mut base_metadata = IndexMap::new();
        base_metadata.insert(
            "feature".to_string(),
            serde_json::json!({"name": "Feature", "description": "A feature"}),
        );

        let mut variant_metadata = IndexMap::new();
        variant_metadata.insert("limit".to_string(), serde_json::json!(100));

        let mut base = make_base_product("myproduct");
        base.metadata = Some(base_metadata);

        let mut variant = make_variant("myproduct", "pro", 999);
        variant.metadata = Some(variant_metadata);

        let products = vec![base, variant];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 1);
        // Both metadata keys should be present
        assert!(consolidated[0].metadata.contains_key("feature"));
        assert!(consolidated[0].metadata.contains_key("limit"));
    }

    #[test]
    fn test_multiple_product_dirs() {
        let products = vec![
            make_base_product("product-a"),
            make_variant("product-a", "lite", 199),
            make_variant("product-a", "pro", 499),
            make_base_product("product-b"),
            make_variant("product-b", "starter", 99),
            make_variant("product-b", "enterprise", 999),
        ];

        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 4);

        let ids: Vec<&str> = consolidated.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"product-a-lite"));
        assert!(ids.contains(&"product-a-pro"));
        assert!(ids.contains(&"product-b-starter"));
        assert!(ids.contains(&"product-b-enterprise"));
    }

    #[test]
    fn test_is_base_product() {
        let base = make_base_product("surfnet");
        let variant = make_variant("surfnet", "pro", 999);

        assert!(base.is_base_product());
        assert!(!variant.is_base_product());
    }

    #[test]
    fn test_is_variant() {
        let base = make_base_product("surfnet");
        let variant = make_variant("surfnet", "pro", 999);

        assert!(!base.is_variant());
        assert!(variant.is_variant());
    }

    #[test]
    fn test_has_prices() {
        let with_prices = make_variant("surfnet", "pro", 999);
        let mut without_prices = make_variant("surfnet", "empty", 0);
        without_prices.prices = None;

        assert!(with_prices.has_prices());
        assert!(!without_prices.has_prices());
    }

    #[test]
    fn test_active_status_inheritance() {
        let mut base = make_base_product("myproduct");
        base.active = Some(false); // Base is inactive

        let variant = make_variant("myproduct", "pro", 999);

        let products = vec![base, variant];
        let consolidated = consolidate_products(&products);

        assert_eq!(consolidated.len(), 1);
        // Product should be inactive because base is inactive
        assert!(!consolidated[0].active);
    }

    // ========================================================================
    // Deep Merge Tests
    // ========================================================================

    #[test]
    fn test_deep_merge_json_simple_override() {
        let base = serde_json::json!({
            "name": "Base",
            "value": 100
        });
        let overlay = serde_json::json!({
            "name": "Overlay"
        });

        let result = deep_merge_json(base, overlay);

        assert_eq!(result["name"], "Overlay");
        assert_eq!(result["value"], 100);
    }

    #[test]
    fn test_deep_merge_json_nested_objects() {
        let base = serde_json::json!({
            "product": {
                "type": "service",
                "unit_label": "per network"
            }
        });
        let overlay = serde_json::json!({
            "product": {
                "name": "My Product"
            }
        });

        let result = deep_merge_json(base, overlay);

        assert_eq!(result["product"]["type"], "service");
        assert_eq!(result["product"]["unit_label"], "per network");
        assert_eq!(result["product"]["name"], "My Product");
    }

    #[test]
    fn test_deep_merge_json_array_replacement() {
        let base = serde_json::json!({
            "prices": [{"amount": 100}]
        });
        let overlay = serde_json::json!({
            "prices": [{"amount": 200}, {"amount": 300}]
        });

        let result = deep_merge_json(base, overlay);

        // Arrays are completely replaced, not merged
        let prices = result["prices"].as_array().unwrap();
        assert_eq!(prices.len(), 2);
        assert_eq!(prices[0]["amount"], 200);
        assert_eq!(prices[1]["amount"], 300);
    }

    #[test]
    fn test_deep_merge_json_null_handling() {
        let base = serde_json::json!({
            "name": "Base",
            "description": "Original"
        });
        let overlay = serde_json::json!({
            "description": null
        });

        let result = deep_merge_json(base, overlay);

        // Null overlay keeps base value
        assert_eq!(result["name"], "Base");
        assert_eq!(result["description"], "Original");
    }

    #[test]
    fn test_merge_product_with_variant_basic() {
        let base_yaml = r#"
product_type: service
unit_label: per network
active: true
"#;
        let variant_yaml = r#"
name: Surfnet Pro
description: For active development
prices:
  - currency: usd
    unit_amount: 999
"#;

        let result = merge_product_with_variant(base_yaml, variant_yaml, "surfnet", "pro").unwrap();

        assert_eq!(result["id"], "surfnet-pro");
        assert_eq!(result["name"], "Surfnet Pro");
        assert_eq!(result["description"], "For active development");
        assert_eq!(result["product_type"], "service");
        assert_eq!(result["unit_label"], "per network");
        assert_eq!(result["active"], true);

        let prices = result["prices"].as_array().unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0]["unit_amount"], 999);
    }

    #[test]
    fn test_merge_product_with_variant_explicit_id() {
        let base_yaml = r#"
product_type: service
"#;
        let variant_yaml = r#"
id: custom-product-id
name: Custom Product
"#;

        let result =
            merge_product_with_variant(base_yaml, variant_yaml, "product", "variant").unwrap();

        // Explicit ID should be preserved
        assert_eq!(result["id"], "custom-product-id");
    }

    #[test]
    fn test_merge_product_with_variant_metadata_deep_merge() {
        let base_yaml = r#"
metadata:
  transaction_limit:
    name: Transaction Limit
    description: Total transactions
"#;
        let variant_yaml = r#"
name: Pro
metadata:
  transaction_limit:
    value: 500
"#;

        let result = merge_product_with_variant(base_yaml, variant_yaml, "surfnet", "pro").unwrap();

        // Metadata should be deep merged
        let tx_limit = &result["metadata"]["transaction_limit"];
        assert_eq!(tx_limit["name"], "Transaction Limit");
        assert_eq!(tx_limit["description"], "Total transactions");
        assert_eq!(tx_limit["value"], 500);
    }
}
