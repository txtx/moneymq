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
    pub id: String,

    /// Product name (required)
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

    /// Prices for this product
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prices: Option<Vec<PriceSchema>>,

    /// Source filename (without extension) - used to track which YAML file this came from.
    /// When saving, this determines the target file. If not set, uses the product ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _source_file: Option<String>,
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
