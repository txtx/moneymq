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

use std::{path::PathBuf, sync::Arc};

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use indexmap::IndexMap;
// Re-export types from moneymq_types::iac for backwards compatibility
pub use moneymq_types::iac::{
    AggregationFormula,
    AggregationSchema,
    // Catalog schema
    CatalogSchema,
    // Enums
    Chain,
    Currency,
    CustomerMappingSchema,
    DeploymentType,
    KeyManagement,
    // Meter schemas
    MeterSchema,
    OneTimePriceSchema,
    PriceSchema,
    PricingType,
    // Product/Price schemas
    ProductSchema,
    RecurringInterval,
    RecurringPriceSchema,
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
    if let Some(catalogs) = json_config.get_mut("catalogs") {
        if let Some(catalogs_obj) = catalogs.as_object_mut() {
            for (_catalog_name, catalog_value) in catalogs_obj.iter_mut() {
                if let Some(catalog_obj) = catalog_value.as_object_mut() {
                    // Get catalog_path (default to "billing/v1")
                    let catalog_path = catalog_obj
                        .get("catalog_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("billing/v1");

                    // Load products from {catalog_path}/products/*.yaml
                    let products_dir = manifest_dir.join(catalog_path).join("products");
                    if products_dir.exists() {
                        if let Ok(entries) = std::fs::read_dir(&products_dir) {
                            let mut products: Vec<serde_json::Value> = Vec::new();

                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                                    if let Ok(content) = std::fs::read_to_string(&path) {
                                        // Parse as generic YAML then convert to JSON
                                        if let Ok(product_yaml) =
                                            serde_yml::from_str::<serde_yml::Value>(&content)
                                        {
                                            if let Ok(mut product_json) =
                                                serde_json::to_value(&product_yaml)
                                            {
                                                // Add _source_file to track the original filename
                                                if let Some(obj) = product_json.as_object_mut() {
                                                    if let Some(stem) = path.file_stem() {
                                                        obj.insert(
                                                            "_source_file".to_string(),
                                                            serde_json::Value::String(
                                                                stem.to_string_lossy().to_string(),
                                                            ),
                                                        );
                                                    }
                                                }
                                                products.push(product_json);
                                            }
                                        }
                                    }
                                }
                            }

                            // Add products to catalog
                            if !products.is_empty() {
                                catalog_obj.insert(
                                    "products".to_string(),
                                    serde_json::Value::Array(products),
                                );
                            }
                        }
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
    if let Ok(manifest) = serde_yml::from_str::<serde_yml::Value>(content) {
        if let Some(catalogs) = manifest.get("catalogs") {
            if let Some(catalogs_map) = catalogs.as_mapping() {
                // Get the first catalog's catalog_path
                if let Some((_, catalog)) = catalogs_map.iter().next() {
                    if let Some(path) = catalog.get("catalog_path") {
                        if let Some(path_str) = path.as_str() {
                            return path_str.to_string();
                        }
                    }
                }
            }
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
                    "stablecoins": ["USDC", "USDT"]
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
        assert!(json.contains("\"USDT\""));
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
        assert!(json.contains("OneTimePriceSchema"));
        assert!(json.contains("RecurringPriceSchema"));
        assert!(json.contains("MeterSchema"));
        assert!(json.contains("CatalogSchema"));

        // Verify enums
        assert!(json.contains("\"one_time\""));
        assert!(json.contains("\"recurring\""));
        assert!(json.contains("\"month\""));
        assert!(json.contains("\"year\""));
        assert!(json.contains("\"usd\""));
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
            prices: None,
            _source_file: None,
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
        let price = PriceSchema::OneTime(OneTimePriceSchema {
            id: Some("price_123".to_string()),
            currency: Currency::Usd,
            unit_amount: 9900, // $99.00
            active: Some(true),
            nickname: Some("Lifetime access".to_string()),
            metadata: None,
        });

        let yaml = serde_yml::to_string(&price).unwrap();
        assert!(yaml.contains("pricing_type: one_time"));
        assert!(yaml.contains("currency: usd"));
        assert!(yaml.contains("unit_amount: 9900"));
        assert!(yaml.contains("nickname: Lifetime access"));
    }

    #[test]
    fn test_yaml_format_recurring_price() {
        let price = PriceSchema::Recurring(RecurringPriceSchema {
            id: Some("price_456".to_string()),
            currency: Currency::Usd,
            unit_amount: 2900, // $29.00
            interval: RecurringInterval::Month,
            interval_count: Some(1),
            active: Some(true),
            nickname: Some("Monthly Pro".to_string()),
            metadata: None,
        });

        let yaml = serde_yml::to_string(&price).unwrap();
        assert!(yaml.contains("pricing_type: recurring"));
        assert!(yaml.contains("currency: usd"));
        assert!(yaml.contains("unit_amount: 2900"));
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
                prices: Some(vec![PriceSchema::Recurring(RecurringPriceSchema {
                    id: None,
                    currency: Currency::Usd,
                    unit_amount: 999,
                    interval: RecurringInterval::Month,
                    interval_count: None,
                    active: None,
                    nickname: None,
                    metadata: None,
                })]),
                _source_file: None,
            }]),
            meters: None,
        };

        let yaml = serde_yml::to_string(&catalog).unwrap();
        assert!(yaml.contains("description: Main product catalog"));
        assert!(yaml.contains("catalog_path: billing/v1"));
        assert!(yaml.contains("source_type: stripe"));
        assert!(yaml.contains("name: Basic Plan"));
        assert!(yaml.contains("unit_amount: 999"));
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
currency: usd
unit_amount: 4999
nickname: One-time purchase
"#;

        let price: PriceSchema = serde_yml::from_str(yaml).unwrap();
        match price {
            PriceSchema::OneTime(config) => {
                assert_eq!(config.currency, Currency::Usd);
                assert_eq!(config.unit_amount, 4999);
                assert_eq!(config.nickname, Some("One-time purchase".to_string()));
            }
            _ => panic!("Expected OneTime variant"),
        }
    }

    #[test]
    fn test_yaml_deserialize_recurring_price() {
        let yaml = r#"
pricing_type: recurring
currency: eur
unit_amount: 1999
interval: year
interval_count: 1
"#;

        let price: PriceSchema = serde_yml::from_str(yaml).unwrap();
        match price {
            PriceSchema::Recurring(config) => {
                assert_eq!(config.currency, Currency::Eur);
                assert_eq!(config.unit_amount, 1999);
                assert_eq!(config.interval, RecurringInterval::Year);
                assert_eq!(config.interval_count, Some(1));
            }
            _ => panic!("Expected Recurring variant"),
        }
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
