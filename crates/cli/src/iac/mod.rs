//! Infrastructure as Code (IAC) endpoints for MoneyMQ.
//!
//! These endpoints allow programmatic configuration of MoneyMQ manifests,
//! enabling both local development tools and cloud deployment workflows.
//!
//! # Endpoints
//!
//! - `GET /iac/data` - Retrieve current manifest configuration as JSON (includes lint diagnostics)
//! - `PUT /iac/data` - Update manifest configuration (partial updates supported)
//! - `GET /iac/schema` - Get JSON schema for the IAC structure

pub mod lint;
mod sync;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use indexmap::IndexMap;
// Re-export types from moneymq_types::iac for backwards compatibility
#[allow(unused_imports)]
pub use moneymq_types::iac::{
    AggregationFormula,
    AggregationSchema,
    CatalogSchema,
    // Enums
    Chain,
    CustomerMappingSchema,
    DeploymentType,
    KeyManagement,
    MeterSchema,
    PriceSchema,
    PricingType,
    // Product/Price schemas
    ProductSchema,
    RecurringConfig,
    RecurringInterval,
    SourceType,
    Stablecoin,
    ValueSettingsSchema,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use sync::insert_environment;

// ============================================================================
// State
// ============================================================================

/// State for IAC endpoints
#[derive(Clone)]
pub struct IacState {
    /// Path to the manifest file
    pub manifest_path: Arc<PathBuf>,
}

impl IacState {
    pub fn new(manifest_path: PathBuf) -> Self {
        Self {
            manifest_path: Arc::new(manifest_path),
        }
    }
}

// ============================================================================
// Deployment-specific Schema Types (for schema generation only)
// ============================================================================

/// Sandbox environment configuration - local development with embedded validator
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SandboxEnvSchema {
    /// API server binding address. Default: "0.0.0.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_address: Option<String>,

    /// API server port. Default: 8488
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Payment facilitator configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facilitator: Option<SandboxFacilitatorSchema>,

    /// Embedded validator network configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<SandboxNetworkSchema>,
}

/// Sandbox facilitator configuration
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SandboxFacilitatorSchema {
    /// Fee in basis points (0-10000, where 100 = 1%). Default: 0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<u64>,

    /// Key management - InMemory recommended for sandbox. Default: InMemory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_management: Option<KeyManagement>,
}

/// Sandbox network configuration - embedded validator settings
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SandboxNetworkSchema {
    /// Blockchain network. Default: Solana
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<Chain>,

    /// Payment recipient address (auto-generated if empty)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,

    /// Validator binding address. Default: "0.0.0.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_address: Option<String>,

    /// Embedded validator RPC port. Default: 8899
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_port: Option<u16>,

    /// Embedded validator WebSocket port. Default: 8900
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_port: Option<u16>,
}

/// SelfHosted environment configuration - self-hosted with external RPC
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SelfHostedEnvSchema {
    /// API server binding address. Default: "0.0.0.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_address: Option<String>,

    /// API server port. Default: 8488
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Payment facilitator configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facilitator: Option<IacFacilitatorConfig>,

    /// External network configuration (required)
    pub network: SelfHostedNetworkSchema,
}

/// SelfHosted network configuration - external validator connection
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SelfHostedNetworkSchema {
    /// Blockchain network. Default: Solana
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<Chain>,

    /// Payment recipient wallet address (required)
    pub recipient: String,

    /// Solana RPC endpoint URL (required)
    pub rpc_url: String,

    /// Solana WebSocket endpoint URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_url: Option<String>,
}

/// CloudHosted environment configuration - managed by moneymq.co
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CloudHostedEnvSchema {
    /// Project display name (required)
    pub project: String,

    /// Workspace slug/subdomain (required)
    pub workspace: String,

    /// Payment facilitator configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facilitator: Option<CloudHostedFacilitatorSchema>,
}

/// CloudHosted facilitator configuration
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CloudHostedFacilitatorSchema {
    /// Fee in basis points (0-10000, where 100 = 1%). Default: 0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<u64>,

    /// Key management - TurnKey required for cloud. Default: TurnKey
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_management: Option<KeyManagement>,
}

/// Environment configuration as a tagged union - deployment type determines available fields
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "deployment")]
pub enum IacEnvironmentSchema {
    /// Local development with embedded Solana validator
    Sandbox(SandboxEnvSchema),
    /// Self-hosted infrastructure with external RPC
    SelfHosted(SelfHostedEnvSchema),
    /// Hosted by moneymq.co - managed infrastructure
    CloudHosted(CloudHostedEnvSchema),
}

/// Full IAC schema for UI form generation
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IacSchema {
    /// Catalog configurations keyed by catalog name (e.g., "v1")
    /// Each catalog contains products, prices, and meters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalogs: Option<IndexMap<String, CatalogSchema>>,

    /// Payment configuration - networks and currencies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payments: Option<IacPaymentsConfig>,

    /// Environment configurations - deployment type determines available fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environments: Option<IndexMap<String, IacEnvironmentSchema>>,
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// MoneyMQ Infrastructure as Code configuration.
///
/// This schema defines the complete configuration for a MoneyMQ payment stack,
/// including catalogs (product definitions), payment settings, and environments.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct IacRequest {
    /// Catalog configurations keyed by catalog name (e.g., "v1").
    /// Catalogs define products, prices, and billing meters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalogs: Option<IndexMap<String, IacCatalogConfig>>,

    /// Payment configuration - networks and currencies to accept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payments: Option<IacPaymentsConfig>,

    /// Environment configurations keyed by environment name.
    /// Examples: "sandbox", "staging", "production", "cloud"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environments: Option<IndexMap<String, IacEnvironmentConfig>>,

    /// Products to update. Each product will be written to its corresponding YAML file.
    /// The product ID determines the filename: {catalog_path}/products/{id}.yaml
    #[serde(skip_serializing_if = "Option::is_none")]
    pub products: Option<Vec<ProductSchema>>,
}

/// Catalog configuration - defines where product/price data comes from.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IacCatalogConfig {
    /// Human-readable description of this catalog
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Base path for catalog data files. Default: "billing/v1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_path: Option<String>,

    /// External source/provider for this catalog
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceType>,
}

/// Payment configuration - what payments to accept.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IacPaymentsConfig {
    /// Network and currency configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub networks: Option<IacNetworksConfig>,
}

/// Network configuration for payments.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IacNetworksConfig {
    /// Blockchain network to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<Chain>,

    /// Stablecoins to accept as payment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stablecoins: Option<Vec<Stablecoin>>,
}

/// Environment configuration - where and how to deploy.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IacEnvironmentConfig {
    /// Deployment type determines the infrastructure setup
    pub deployment: DeploymentType,

    /// Project display name (CloudHosted only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,

    /// Workspace slug/subdomain (CloudHosted only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,

    /// API server binding address (Sandbox/SelfHosted). Default: "0.0.0.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_address: Option<String>,

    /// API server port (Sandbox/SelfHosted). Default: 8488
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Payment facilitator configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facilitator: Option<IacFacilitatorConfig>,

    /// Blockchain network configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<IacNetworkEnvConfig>,
}

/// Facilitator configuration - payment processing settings.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IacFacilitatorConfig {
    /// Fee in basis points (0-10000, where 100 = 1%). Default: 0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<u64>,

    /// Key management strategy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_management: Option<KeyManagement>,
}

/// Network configuration for an environment.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct IacNetworkEnvConfig {
    /// Blockchain network
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<Chain>,

    /// Payment recipient wallet address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,

    /// Validator binding address (Sandbox only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_address: Option<String>,

    /// RPC port for embedded validator (Sandbox only). Default: 8899
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_port: Option<u16>,

    /// WebSocket port for embedded validator (Sandbox only). Default: 8900
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_port: Option<u16>,

    /// RPC URL for external validator (SelfHosted only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_url: Option<String>,

    /// WebSocket URL for external validator (SelfHosted only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_url: Option<String>,
}

/// Response from IAC endpoints.
#[derive(Debug, Serialize)]
pub struct IacResponse {
    /// Whether the operation succeeded
    pub success: bool,

    /// Human-readable message describing the result
    pub message: String,

    /// Configuration data (GET) or updated fields summary (PUT)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,

    /// Lint diagnostics (GET /iac/data only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<lint::LintResult>,
}

// ============================================================================
// Router
// ============================================================================

/// Create the IAC router with GET/PUT /iac/data and GET /iac/schema endpoints
pub fn create_router(state: IacState) -> Router<()> {
    Router::new()
        .route("/iac/data", get(get_iac).put(put_iac))
        .route("/iac/schema", get(get_schema))
        .with_state(state)
}

// ============================================================================
// Endpoints
// ============================================================================

/// GET /iac/schema - Return JSON schema for IAC configuration.
///
/// Uses schemars to generate a proper JSON Schema with:
/// - Tagged enum (oneOf) for deployment types with discriminator
/// - Each deployment type has its own set of required/optional fields
/// - Enum values for dropdowns (Chain, Stablecoin, KeyManagement, etc.)
pub async fn get_schema() -> impl IntoResponse {
    let schema = schemars::schema_for!(IacSchema);
    (StatusCode::OK, Json(schema))
}

/// Recursively load variants from a directory, supporting nested variant structures.
///
/// For a variant with nested variants (e.g., variants/lite/variants/a/), this function:
/// 1. Merges the intermediate variant with the base
/// 2. Recursively loads nested variants using the merged base
/// 3. Returns only leaf variants (those without nested variants)
fn load_variants_recursive_json(
    variants_dir: &Path,
    base_json: &serde_json::Value,
    product_dir_name: &str,
    variant_prefix: &str,
    source_prefix: &str,
) -> Vec<serde_json::Value> {
    let mut products = Vec::new();

    let entries = match std::fs::read_dir(variants_dir) {
        Ok(e) => e,
        Err(_) => return products,
    };

    for entry in entries.flatten() {
        let variant_path = entry.path();

        // Determine variant name and yaml path based on layout:
        // Old: variants/{variant}.yaml (file)
        // New: variants/{variant}/product.yaml (directory)
        let (variant_name, yaml_path, is_directory) = if variant_path.is_dir() {
            // New layout: variants/{variant}/product.yaml
            let yaml_file = variant_path.join("product.yaml");
            if !yaml_file.exists() {
                continue;
            }
            let name = variant_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            (name, yaml_file, true)
        } else if variant_path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            // Old layout: variants/{variant}.yaml
            let name = variant_path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            (name, variant_path.clone(), false)
        } else {
            continue;
        };

        // Build full variant ID path (e.g., "lite" or "lite-a")
        let full_variant_id = if variant_prefix.is_empty() {
            variant_name.clone()
        } else {
            format!("{}-{}", variant_prefix, variant_name)
        };

        // Build source file path
        let source_file = if source_prefix.is_empty() {
            format!("{}/variants/{}/product", product_dir_name, variant_name)
        } else {
            format!("{}/variants/{}/product", source_prefix, variant_name)
        };

        // Read variant content
        let content = match std::fs::read_to_string(&yaml_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let variant_yaml: serde_yml::Value = match serde_yml::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let variant_json = match serde_json::to_value(&variant_yaml) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Check for nested variants (only for directory layout)
        let nested_variants_dir = variant_path.join("variants");
        if is_directory && nested_variants_dir.exists() {
            // This variant has nested variants - merge and recurse
            let merged = moneymq_types::deep_merge_json(base_json.clone(), variant_json);

            let nested_source_prefix = format!(
                "{}/variants/{}",
                if source_prefix.is_empty() {
                    product_dir_name
                } else {
                    source_prefix
                },
                variant_name
            );

            let nested_products = load_variants_recursive_json(
                &nested_variants_dir,
                &merged,
                product_dir_name,
                &full_variant_id,
                &nested_source_prefix,
            );
            products.extend(nested_products);
        } else {
            // Leaf variant - merge with base and add metadata
            let mut merged = moneymq_types::deep_merge_json(base_json.clone(), variant_json);

            if let Some(obj) = merged.as_object_mut() {
                // Set ID for variant (matches loader format)
                if !obj.contains_key("id") {
                    obj.insert(
                        "id".to_string(),
                        serde_json::Value::String(format!(
                            "{}-{}",
                            product_dir_name, full_variant_id
                        )),
                    );
                }
                obj.insert(
                    "_source_file".to_string(),
                    serde_json::Value::String(source_file),
                );
                obj.insert(
                    "_product_dir".to_string(),
                    serde_json::Value::String(product_dir_name.to_string()),
                );
                obj.insert(
                    "_variant".to_string(),
                    serde_json::Value::String(full_variant_id),
                );
            }
            products.push(merged);
        }
    }

    products
}

/// GET /iac/data - Retrieve current manifest configuration as JSON.
///
/// This endpoint returns the manifest configuration with resolved catalog data.
/// For each catalog with a `catalog_path`, products are loaded from
/// `{catalog_path}/products/*.yaml` files.
pub async fn get_iac(State(state): State<IacState>) -> impl IntoResponse {
    let manifest_file = state.manifest_path.as_ref();
    let manifest_dir = manifest_file.parent().unwrap_or(std::path::Path::new("."));

    // Read existing manifest
    let manifest_content = match std::fs::read_to_string(manifest_file) {
        Ok(content) => content,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(IacResponse {
                    success: false,
                    message: format!("Failed to read manifest: {}", e),
                    config: None,
                    diagnostics: None,
                }),
            );
        }
    };

    // Parse YAML to JSON-compatible value
    let manifest: serde_yml::Value = match serde_yml::from_str(&manifest_content) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(IacResponse {
                    success: false,
                    message: format!("Failed to parse manifest: {}", e),
                    config: None,
                    diagnostics: None,
                }),
            );
        }
    };

    // Convert to JSON
    let mut json_config = match serde_json::to_value(&manifest) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(IacResponse {
                    success: false,
                    message: format!("Failed to convert manifest to JSON: {}", e),
                    config: None,
                    diagnostics: None,
                }),
            );
        }
    };

    // Resolve catalog products from catalog_path directories
    if let Some(catalogs) = json_config.get_mut("catalogs")
        && let Some(catalogs_obj) = catalogs.as_object_mut()
    {
        for (_catalog_name, catalog_value) in catalogs_obj.iter_mut() {
            if let Some(catalog_obj) = catalog_value.as_object_mut() {
                // Get catalog_path (default to "billing/v1")
                let catalog_path = catalog_obj
                    .get("catalog_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("billing/v1");

                // Load products from {catalog_path}/products/
                // Supports both legacy flat files (*.yaml) and variant-based directories
                let products_dir = manifest_dir.join(catalog_path).join("products");
                if products_dir.exists()
                    && let Ok(entries) = std::fs::read_dir(&products_dir)
                {
                    let mut products: Vec<serde_json::Value> = Vec::new();

                    for entry in entries.flatten() {
                        let path = entry.path();

                        if path.is_dir() {
                            // Check for variant-based product directory (product.yaml + variants/)
                            let product_yaml = path.join("product.yaml");
                            if product_yaml.exists()
                                && let Ok(base_content) = std::fs::read_to_string(&product_yaml)
                            {
                                // Parse base product.yaml
                                if let Ok(base_yaml) =
                                    serde_yml::from_str::<serde_yml::Value>(&base_content)
                                    && let Ok(base_json) = serde_json::to_value(&base_yaml)
                                {
                                    let dir_name_str = path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "unknown".to_string());

                                    // Create base product entry for the UI
                                    let mut base_product = base_json.clone();
                                    if let Some(obj) = base_product.as_object_mut() {
                                        // Set ID for base product (used for grouping in UI)
                                        obj.insert(
                                            "id".to_string(),
                                            serde_json::Value::String(format!(
                                                "{}/product",
                                                dir_name_str
                                            )),
                                        );
                                        // Set name for base product if not present
                                        if !obj.contains_key("name") {
                                            let name = dir_name_str
                                                .chars()
                                                .next()
                                                .map(|c| c.to_uppercase().to_string())
                                                .unwrap_or_default()
                                                + &dir_name_str[1..];
                                            obj.insert(
                                                "name".to_string(),
                                                serde_json::Value::String(name),
                                            );
                                        }
                                        obj.insert(
                                            "_source_file".to_string(),
                                            serde_json::Value::String(format!(
                                                "{}/product",
                                                dir_name_str
                                            )),
                                        );
                                        obj.insert(
                                            "_product_dir".to_string(),
                                            serde_json::Value::String(dir_name_str.clone()),
                                        );
                                    }
                                    products.push(base_product);

                                    // Load variants recursively (supports nested variants)
                                    let variants_dir = path.join("variants");
                                    if variants_dir.exists() {
                                        let variant_products = load_variants_recursive_json(
                                            &variants_dir,
                                            &base_json,
                                            &dir_name_str,
                                            "",
                                            "",
                                        );
                                        products.extend(variant_products);
                                    }
                                }
                            }
                        } else if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                            // Legacy flat file format
                            if let Ok(content) = std::fs::read_to_string(&path)
                                && let Ok(product_yaml) =
                                    serde_yml::from_str::<serde_yml::Value>(&content)
                                && let Ok(mut product_json) = serde_json::to_value(&product_yaml)
                            {
                                // Add _source_file to track the original filename
                                if let Some(obj) = product_json.as_object_mut()
                                    && let Some(stem) = path.file_stem()
                                {
                                    obj.insert(
                                        "_source_file".to_string(),
                                        serde_json::Value::String(
                                            stem.to_string_lossy().to_string(),
                                        ),
                                    );
                                }
                                products.push(product_json);
                            }
                        }
                    }

                    // Add products to catalog
                    if !products.is_empty() {
                        catalog_obj
                            .insert("products".to_string(), serde_json::Value::Array(products));
                    }
                }
            }
        }
    }

    // Get catalog_path from first catalog for linting
    let catalog_path = json_config
        .get("catalogs")
        .and_then(|c| c.as_object())
        .and_then(|m| m.values().next())
        .and_then(|v| v.get("catalog_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("billing/v1");

    // Run lint rules and include diagnostics
    let lint_result = lint::lint_all(manifest_dir, catalog_path);

    (
        StatusCode::OK,
        Json(IacResponse {
            success: true,
            message: "Current manifest configuration".to_string(),
            config: Some(json_config),
            diagnostics: Some(lint_result),
        }),
    )
}

/// PUT /iac/data - Update manifest configuration.
///
/// Uses text-based insertion to preserve comments and formatting.
/// Supports updating environments and products.
pub async fn put_iac(
    State(state): State<IacState>,
    Json(request): Json<IacRequest>,
) -> impl IntoResponse {
    let manifest_file = state.manifest_path.as_ref();
    let manifest_dir = manifest_file.parent().unwrap_or(std::path::Path::new("."));

    // Read existing manifest as text
    let mut content = match std::fs::read_to_string(manifest_file) {
        Ok(content) => content,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(IacResponse {
                    success: false,
                    message: format!("Failed to read manifest: {}", e),
                    config: None,
                    diagnostics: None,
                }),
            );
        }
    };

    let mut updated_sections: Vec<String> = Vec::new();

    // Handle environment updates with text-based insertion
    if let Some(environments) = &request.environments {
        for (name, config) in environments {
            let result = insert_environment(&content, name, config);
            if result.changed {
                content = result.content;
            }
            updated_sections.push(result.message);
        }
    }

    // Write manifest updates back to file
    if let Err(e) = std::fs::write(manifest_file, &content) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(IacResponse {
                success: false,
                message: format!("Failed to write manifest: {}", e),
                config: None,
                diagnostics: None,
            }),
        );
    }

    // Handle product updates - write each product to its YAML file
    if let Some(products) = &request.products {
        // Get catalog_path from manifest (default to "billing/v1")
        let catalog_path = get_catalog_path(&content);
        let products_dir = manifest_dir.join(&catalog_path).join("products");

        // Ensure products directory exists
        if let Err(e) = std::fs::create_dir_all(&products_dir) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(IacResponse {
                    success: false,
                    message: format!("Failed to create products directory: {}", e),
                    config: None,
                    diagnostics: None,
                }),
            );
        }

        for product in products {
            // Use _source_file if available, otherwise fall back to product ID
            let filename = product._source_file.as_ref().unwrap_or(&product.id).clone();
            let product_file = products_dir.join(format!("{}.yaml", filename));

            // Ensure parent directory exists (for nested variant paths like surfnet/variants/light/product.yaml)
            if let Some(parent) = product_file.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(IacResponse {
                        success: false,
                        message: format!(
                            "Failed to create directory for product '{}': {}",
                            product.id, e
                        ),
                        config: None,
                        diagnostics: None,
                    }),
                );
            }

            // Merge with existing file content if it exists
            let yaml_content = if product_file.exists() {
                match std::fs::read_to_string(&product_file) {
                    Ok(existing_content) => {
                        let result = sync::merge_product_update(&existing_content, product);
                        if !result.changed {
                            updated_sections.push(result.message);
                            continue;
                        }
                        result.content
                    }
                    Err(_) => {
                        // If reading fails, create new file
                        match crate::yaml_util::to_pretty_yaml_with_header(
                            product,
                            Some("Product"),
                            Some("v1"),
                        ) {
                            Ok(yaml) => yaml,
                            Err(e) => {
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(IacResponse {
                                        success: false,
                                        message: format!(
                                            "Failed to serialize product '{}': {}",
                                            product.id, e
                                        ),
                                        config: None,
                                        diagnostics: None,
                                    }),
                                );
                            }
                        }
                    }
                }
            } else {
                // New file - create from scratch
                match crate::yaml_util::to_pretty_yaml_with_header(
                    product,
                    Some("Product"),
                    Some("v1"),
                ) {
                    Ok(yaml) => yaml,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(IacResponse {
                                success: false,
                                message: format!(
                                    "Failed to serialize product '{}': {}",
                                    product.id, e
                                ),
                                config: None,
                                diagnostics: None,
                            }),
                        );
                    }
                }
            };

            // Write product YAML file
            if let Err(e) = std::fs::write(&product_file, &yaml_content) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(IacResponse {
                        success: false,
                        message: format!("Failed to write product '{}': {}", product.id, e),
                        config: None,
                        diagnostics: None,
                    }),
                );
            }

            updated_sections.push(format!("Updated product '{}'", product.id));
        }
    }

    let message = if updated_sections.is_empty() {
        "No changes made".to_string()
    } else {
        updated_sections.join("; ")
    };

    let updated_config = serde_json::to_value(&request).ok();

    (
        StatusCode::OK,
        Json(IacResponse {
            success: true,
            message,
            config: updated_config,
            diagnostics: None,
        }),
    )
}

/// Extract the catalog_path from manifest content (defaults to "billing/v1")
fn get_catalog_path(content: &str) -> String {
    // Parse the manifest to find catalog_path
    if let Ok(manifest) = serde_yml::from_str::<serde_yml::Value>(content)
        && let Some(catalogs) = manifest.get("catalogs")
        && let Some(catalogs_map) = catalogs.as_mapping()
    {
        // Get the first catalog's catalog_path
        if let Some((_, catalog)) = catalogs_map.iter().next()
            && let Some(path) = catalog.get("catalog_path")
            && let Some(path_str) = path.as_str()
        {
            return path_str.to_string();
        }
    }
    "billing/v1".to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iac_request_deserialize_full() {
        let json = r#"{
            "catalogs": {
                "v1": {
                    "description": "My Catalog",
                    "catalog_path": "billing/v1"
                }
            },
            "payments": {
                "networks": {
                    "chain": "Solana",
                    "stablecoins": ["USDC"]
                }
            },
            "environments": {
                "cloud": {
                    "deployment": "CloudHosted",
                    "project": "My Project",
                    "workspace": "my-workspace",
                    "facilitator": {
                        "fee": 0,
                        "key_management": "TurnKey"
                    }
                }
            }
        }"#;

        let request: IacRequest = serde_json::from_str(json).unwrap();

        assert!(request.catalogs.is_some());
        assert!(request.payments.is_some());
        assert!(request.environments.is_some());

        let envs = request.environments.unwrap();
        let cloud = envs.get("cloud").unwrap();
        assert!(matches!(cloud.deployment, DeploymentType::CloudHosted));
        assert_eq!(cloud.project, Some("My Project".to_string()));
    }

    #[test]
    fn test_iac_request_deserialize_partial() {
        let json = r#"{
            "environments": {
                "production": {
                    "deployment": "SelfHosted",
                    "port": 8488
                }
            }
        }"#;

        let request: IacRequest = serde_json::from_str(json).unwrap();

        assert!(request.catalogs.is_none());
        assert!(request.payments.is_none());
        assert!(request.environments.is_some());
    }

    #[test]
    fn test_iac_request_serialize() {
        let mut environments = IndexMap::new();
        environments.insert(
            "cloud".to_string(),
            IacEnvironmentConfig {
                deployment: DeploymentType::CloudHosted,
                project: Some("Test".to_string()),
                workspace: Some("test-ws".to_string()),
                binding_address: None,
                port: None,
                facilitator: None,
                network: None,
            },
        );

        let request = IacRequest {
            catalogs: None,
            payments: None,
            environments: Some(environments),
            products: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("CloudHosted"));
        assert!(json.contains("test-ws"));
        assert!(!json.contains("catalogs"));
    }

    #[test]
    fn test_empty_request() {
        let request = IacRequest::default();
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_iac_response_serialize() {
        let response = IacResponse {
            success: true,
            message: "Updated".to_string(),
            config: Some(serde_json::json!({"test": true})),
            diagnostics: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("Updated"));
    }

    #[test]
    fn test_schema_generation() {
        let schema = schemars::schema_for!(IacSchema);
        let json = serde_json::to_string_pretty(&schema).unwrap();

        // Verify it's a valid JSON schema
        assert!(json.contains("\"$schema\""));

        // Verify enum values are present
        assert!(json.contains("\"Solana\""));
        assert!(json.contains("\"USDC\""));
        assert!(json.contains("\"Sandbox\""));
        assert!(json.contains("\"SelfHosted\""));
        assert!(json.contains("\"CloudHosted\""));
        assert!(json.contains("\"InMemory\""));
        assert!(json.contains("\"TurnKey\""));

        // Verify oneOf is present for the tagged enum
        assert!(json.contains("\"oneOf\""));

        // Verify deployment-specific fields
        assert!(json.contains("\"project\"")); // CloudHosted
        assert!(json.contains("\"workspace\"")); // CloudHosted
        assert!(json.contains("\"rpc_port\"")); // Sandbox
        assert!(json.contains("\"rpc_url\"")); // SelfHosted
        assert!(json.contains("\"recipient\"")); // SelfHosted network

        // Verify schema definitions for our types
        let schema_val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let defs = &schema_val["$defs"];
        assert!(defs["SandboxEnvSchema"].is_object());
        assert!(defs["SelfHostedEnvSchema"].is_object());
        assert!(defs["CloudHostedEnvSchema"].is_object());
    }

    #[test]
    fn test_yaml_format_sandbox_env() {
        let env = IacEnvironmentSchema::Sandbox(SandboxEnvSchema {
            binding_address: Some("0.0.0.0".to_string()),
            port: Some(8488),
            facilitator: Some(SandboxFacilitatorSchema {
                fee: Some(0),
                key_management: Some(KeyManagement::InMemory),
            }),
            network: Some(SandboxNetworkSchema {
                chain: Some(Chain::Solana),
                recipient: None,
                binding_address: Some("0.0.0.0".to_string()),
                rpc_port: Some(8899),
                ws_port: Some(8900),
            }),
        });

        let yaml = serde_yml::to_string(&env).unwrap();
        assert!(yaml.contains("deployment: Sandbox"));
        assert!(yaml.contains("port: 8488"));
        assert!(yaml.contains("rpc_port: 8899"));
        assert!(yaml.contains("key_management: InMemory"));
    }

    #[test]
    fn test_yaml_format_self_hosted_env() {
        let env = IacEnvironmentSchema::SelfHosted(SelfHostedEnvSchema {
            binding_address: Some("0.0.0.0".to_string()),
            port: Some(8488),
            facilitator: Some(IacFacilitatorConfig {
                fee: Some(100),
                key_management: Some(KeyManagement::TurnKey),
            }),
            network: SelfHostedNetworkSchema {
                chain: Some(Chain::Solana),
                recipient: "wallet123".to_string(),
                rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
                ws_url: Some("wss://api.mainnet-beta.solana.com".to_string()),
            },
        });

        let yaml = serde_yml::to_string(&env).unwrap();
        assert!(yaml.contains("deployment: SelfHosted"));
        assert!(yaml.contains("recipient: wallet123"));
        assert!(yaml.contains("rpc_url: https://api.mainnet-beta.solana.com"));
        assert!(yaml.contains("key_management: TurnKey"));
    }

    #[test]
    fn test_yaml_format_cloud_hosted_env() {
        let env = IacEnvironmentSchema::CloudHosted(CloudHostedEnvSchema {
            project: "My Project".to_string(),
            workspace: "my-workspace".to_string(),
            facilitator: Some(CloudHostedFacilitatorSchema {
                fee: Some(50),
                key_management: Some(KeyManagement::TurnKey),
            }),
        });

        let yaml = serde_yml::to_string(&env).unwrap();
        assert!(yaml.contains("deployment: CloudHosted"));
        assert!(yaml.contains("project: My Project"));
        assert!(yaml.contains("workspace: my-workspace"));
        assert!(yaml.contains("fee: 50"));
    }

    #[test]
    fn test_yaml_deserialize_roundtrip() {
        // Test that we can deserialize YAML and it matches the original
        let yaml = r#"
deployment: CloudHosted
project: Test Project
workspace: test-ws
facilitator:
  fee: 25
  key_management: TurnKey
"#;

        let env: IacEnvironmentSchema = serde_yml::from_str(yaml).unwrap();
        match env {
            IacEnvironmentSchema::CloudHosted(config) => {
                assert_eq!(config.project, "Test Project");
                assert_eq!(config.workspace, "test-ws");
                assert_eq!(config.facilitator.as_ref().unwrap().fee, Some(25));
            }
            _ => panic!("Expected CloudHosted variant"),
        }
    }

    #[test]
    fn test_yaml_deserialize_sandbox() {
        let yaml = r#"
deployment: Sandbox
port: 9000
network:
  rpc_port: 9899
  ws_port: 9900
"#;

        let env: IacEnvironmentSchema = serde_yml::from_str(yaml).unwrap();
        match env {
            IacEnvironmentSchema::Sandbox(config) => {
                assert_eq!(config.port, Some(9000));
                assert_eq!(config.network.as_ref().unwrap().rpc_port, Some(9899));
            }
            _ => panic!("Expected Sandbox variant"),
        }
    }

    #[test]
    fn test_yaml_deserialize_self_hosted() {
        let yaml = r#"
deployment: SelfHosted
network:
  recipient: abc123
  rpc_url: https://solana.example.com
"#;

        let env: IacEnvironmentSchema = serde_yml::from_str(yaml).unwrap();
        match env {
            IacEnvironmentSchema::SelfHosted(config) => {
                assert_eq!(config.network.recipient, "abc123");
                assert_eq!(config.network.rpc_url, "https://solana.example.com");
            }
            _ => panic!("Expected SelfHosted variant"),
        }
    }

    // ========================================================================
    // Catalog/Product/Price Schema Tests
    // ========================================================================

    #[test]
    fn test_schema_includes_catalog_types() {
        let schema = schemars::schema_for!(IacSchema);
        let json = serde_json::to_string_pretty(&schema).unwrap();

        // Verify product/price types are included
        assert!(json.contains("ProductSchema"));
        assert!(json.contains("PriceSchema"));
        assert!(json.contains("RecurringConfig"));
        assert!(json.contains("MeterSchema"));
        assert!(json.contains("CatalogSchema"));

        // Verify enums
        assert!(json.contains("\"one_time\""));
        assert!(json.contains("\"recurring\""));
        assert!(json.contains("\"month\""));
        assert!(json.contains("\"year\""));
    }

    #[test]
    fn test_yaml_format_product() {
        let product = ProductSchema {
            id: "prod_123".to_string(),
            name: "Pro Plan".to_string(),
            description: Some("Professional subscription plan".to_string()),
            active: Some(true),
            product_type: Some("service".to_string()),
            statement_descriptor: None,
            unit_label: Some("per seat".to_string()),
            images: Some(vec!["https://example.com/pro.png".to_string()]),
            metadata: None,
            features: None,
            price: None,
            _source_file: None,
            _product_dir: None,
            _variant: None,
        };

        let yaml = serde_yml::to_string(&product).unwrap();
        assert!(yaml.contains("id: prod_123"));
        assert!(yaml.contains("name: Pro Plan"));
        assert!(yaml.contains("description: Professional subscription plan"));
        assert!(yaml.contains("product_type: service"));
        assert!(yaml.contains("unit_label: per seat"));
    }

    #[test]
    fn test_yaml_format_one_time_price() {
        let mut amounts = indexmap::IndexMap::new();
        amounts.insert("usd".to_string(), 99.00);

        let price = PriceSchema {
            id: Some("price_123".to_string()),
            amounts,
            pricing_type: Some(PricingType::OneTime),
            recurring: None,
            overage: None,
            trial: None,
            active: Some(true),
            nickname: Some("Lifetime access".to_string()),
            metadata: None,
        };

        let yaml = serde_yml::to_string(&price).unwrap();
        assert!(yaml.contains("pricing_type: one_time"));
        assert!(yaml.contains("usd: 99.0"));
        assert!(yaml.contains("nickname: Lifetime access"));
    }

    #[test]
    fn test_yaml_format_recurring_price() {
        let mut amounts = indexmap::IndexMap::new();
        amounts.insert("usd".to_string(), 29.00);

        let price = PriceSchema {
            id: Some("price_456".to_string()),
            amounts,
            pricing_type: Some(PricingType::Recurring),
            recurring: Some(RecurringConfig {
                interval: RecurringInterval::Month,
                interval_count: Some(1),
            }),
            overage: None,
            trial: None,
            active: Some(true),
            nickname: Some("Monthly Pro".to_string()),
            metadata: None,
        };

        let yaml = serde_yml::to_string(&price).unwrap();
        assert!(yaml.contains("pricing_type: recurring"));
        assert!(yaml.contains("usd: 29.0"));
        assert!(yaml.contains("interval: month"));
    }

    #[test]
    fn test_yaml_format_meter() {
        let meter = MeterSchema {
            id: "meter_api_calls".to_string(),
            display_name: "API Calls".to_string(),
            event_name: "api_request".to_string(),
            status: Some("active".to_string()),
            customer_mapping: Some(CustomerMappingSchema {
                mapping_type: Some("by_id".to_string()),
                event_payload_key: "customer_id".to_string(),
            }),
            aggregation: Some(AggregationSchema {
                formula: AggregationFormula::Sum,
            }),
            value_settings: Some(ValueSettingsSchema {
                event_payload_key: "request_count".to_string(),
            }),
        };

        let yaml = serde_yml::to_string(&meter).unwrap();
        assert!(yaml.contains("id: meter_api_calls"));
        assert!(yaml.contains("display_name: API Calls"));
        assert!(yaml.contains("event_name: api_request"));
        assert!(yaml.contains("formula: sum"));
        assert!(yaml.contains("event_payload_key: customer_id"));
    }

    #[test]
    fn test_yaml_format_catalog() {
        let mut amounts = indexmap::IndexMap::new();
        amounts.insert("usd".to_string(), 9.99);

        let catalog = CatalogSchema {
            description: Some("Main product catalog".to_string()),
            catalog_path: Some("billing/v1".to_string()),
            source_type: Some(SourceType::Stripe),
            products: Some(vec![ProductSchema {
                id: "prod_basic".to_string(),
                name: "Basic Plan".to_string(),
                description: None,
                active: Some(true),
                product_type: None,
                statement_descriptor: None,
                unit_label: None,
                images: None,
                metadata: None,
                features: None,
                price: Some(PriceSchema {
                    id: None,
                    amounts,
                    pricing_type: Some(PricingType::Recurring),
                    recurring: Some(RecurringConfig {
                        interval: RecurringInterval::Month,
                        interval_count: None,
                    }),
                    overage: None,
                    trial: None,
                    active: None,
                    nickname: None,
                    metadata: None,
                }),
                _source_file: None,
                _product_dir: None,
                _variant: None,
            }]),
            meters: None,
        };

        let yaml = serde_yml::to_string(&catalog).unwrap();
        assert!(yaml.contains("description: Main product catalog"));
        assert!(yaml.contains("catalog_path: billing/v1"));
        assert!(yaml.contains("source_type: stripe"));
        assert!(yaml.contains("name: Basic Plan"));
        assert!(yaml.contains("usd: 9.99"));
    }

    #[test]
    fn test_yaml_deserialize_product() {
        let yaml = r#"
id: prod_abc
name: Enterprise Plan
description: For large teams
active: true
product_type: service
"#;

        let product: ProductSchema = serde_yml::from_str(yaml).unwrap();
        assert_eq!(product.id, "prod_abc");
        assert_eq!(product.name, "Enterprise Plan");
        assert_eq!(product.description, Some("For large teams".to_string()));
        assert_eq!(product.active, Some(true));
        assert_eq!(product.product_type, Some("service".to_string()));
    }

    #[test]
    fn test_yaml_deserialize_one_time_price() {
        let yaml = r#"
pricing_type: one_time
amounts:
  usd: 49.99
nickname: One-time purchase
"#;

        let price: PriceSchema = serde_yml::from_str(yaml).unwrap();
        assert_eq!(price.pricing_type, Some(PricingType::OneTime));
        assert_eq!(price.amounts.get("usd"), Some(&49.99));
        assert_eq!(price.nickname, Some("One-time purchase".to_string()));
    }

    #[test]
    fn test_yaml_deserialize_recurring_price() {
        let yaml = r#"
pricing_type: recurring
amounts:
  eur: 19.99
recurring:
  interval: year
  interval_count: 1
"#;

        let price: PriceSchema = serde_yml::from_str(yaml).unwrap();
        assert_eq!(price.pricing_type, Some(PricingType::Recurring));
        assert_eq!(price.amounts.get("eur"), Some(&19.99));
        let recurring = price.recurring.unwrap();
        assert_eq!(recurring.interval, RecurringInterval::Year);
        assert_eq!(recurring.interval_count, Some(1));
    }

    #[test]
    fn test_yaml_deserialize_meter() {
        let yaml = r#"
id: meter_storage
display_name: Storage Usage
event_name: storage_update
aggregation:
  formula: max
"#;

        let meter: MeterSchema = serde_yml::from_str(yaml).unwrap();
        assert_eq!(meter.id, "meter_storage");
        assert_eq!(meter.display_name, "Storage Usage");
        assert_eq!(meter.event_name, "storage_update");
        assert_eq!(
            meter.aggregation.as_ref().unwrap().formula,
            AggregationFormula::Max
        );
    }
}
