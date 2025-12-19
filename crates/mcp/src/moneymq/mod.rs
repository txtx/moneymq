use std::{fs, path::PathBuf};

use indexmap::IndexMap;
use moneymq_types::iac::{PriceSchema, ProductSchema, ValidationDiagnostic, ValidationResult};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::*,
    prompt_handler, prompt_router, schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::yaml_util::to_pretty_yaml_with_header;

// ============================================================================
// Manifest types for reading catalog configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    pub catalogs: IndexMap<String, CatalogConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogConfig {
    #[serde(default = "default_catalog_path")]
    pub catalog_path: String,
    #[serde(default = "default_source_type")]
    pub source_type: String,
}

fn default_catalog_path() -> String {
    "billing/v1".to_string()
}

fn default_source_type() -> String {
    "none".to_string()
}

// ============================================================================
// MCP Server
// ============================================================================

#[derive(Clone)]
pub struct MoneyMqMcp {
    tool_router: ToolRouter<MoneyMqMcp>,
    prompt_router: PromptRouter<MoneyMqMcp>,
}

// ============================================================================
// Request Types using ProductSchema
// ============================================================================

/// Request to add products to the catalog
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CatalogRequest {
    /// The root directory of the user's project where the catalogs directory will be created
    #[schemars(example = "/path/to/user/project")]
    pub project_root_dir: String,

    /// List of products to create in the catalog using ProductSchema
    pub products: Vec<ProductSchema>,
}

// ============================================================================
// Validation for ProductSchema
// ============================================================================

/// Validate a ProductSchema and return diagnostics
fn validate_product_schema(product: &ProductSchema, index: usize) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();
    let prefix = format!("products[{}]", index);

    // Validate name is not empty
    if product.name.trim().is_empty() {
        diagnostics.push(
            ValidationDiagnostic::error(
                "empty-required-field",
                format!("Product at index {} has empty 'name' field", index),
            )
            .with_field(format!("{}.name", prefix))
            .with_expected("A non-empty string")
            .with_suggestion("Provide a product name like 'Premium Subscription'"),
        );
    }

    // Validate id is not empty
    if product.id.trim().is_empty() {
        diagnostics.push(
            ValidationDiagnostic::error(
                "empty-required-field",
                format!("Product at index {} has empty 'id' field", index),
            )
            .with_field(format!("{}.id", prefix))
            .with_expected("A unique identifier string")
            .with_suggestion("Provide a unique id like 'prod_premium' or 'premium-subscription'"),
        );
    }

    // Validate prices
    match &product.prices {
        None => {
            diagnostics.push(
                ValidationDiagnostic::error(
                    "missing-prices",
                    format!("Product at index {} has no 'prices' array", index),
                )
                .with_field(format!("{}.prices", prefix))
                .with_expected("At least one price object")
                .with_suggestion("Add a prices array with at least one price"),
            );
        }
        Some(prices) if prices.is_empty() => {
            diagnostics.push(
                ValidationDiagnostic::error(
                    "empty-prices",
                    format!("Product at index {} has empty 'prices' array", index),
                )
                .with_field(format!("{}.prices", prefix))
                .with_expected("At least one price object")
                .with_suggestion("Add at least one price to the prices array"),
            );
        }
        Some(prices) => {
            // Validate each price
            for (i, price) in prices.iter().enumerate() {
                let price_prefix = format!("{}.prices[{}]", prefix, i);
                match price {
                    PriceSchema::OneTime(p) => {
                        if p.unit_amount < 0 {
                            diagnostics.push(
                                ValidationDiagnostic::error(
                                    "invalid-amount",
                                    format!("Price at index {} has negative unit_amount", i),
                                )
                                .with_field(format!("{}.unit_amount", price_prefix))
                                .with_expected("A non-negative integer (cents)")
                                .with_received(p.unit_amount.to_string()),
                            );
                        }
                    }
                    PriceSchema::Recurring(p) => {
                        if p.unit_amount < 0 {
                            diagnostics.push(
                                ValidationDiagnostic::error(
                                    "invalid-amount",
                                    format!("Price at index {} has negative unit_amount", i),
                                )
                                .with_field(format!("{}.unit_amount", price_prefix))
                                .with_expected("A non-negative integer (cents)")
                                .with_received(p.unit_amount.to_string()),
                            );
                        }
                        if let Some(count) = p.interval_count {
                            if count < 1 {
                                diagnostics.push(
                                    ValidationDiagnostic::error(
                                        "invalid-interval-count",
                                        format!("Price at index {} has invalid interval_count", i),
                                    )
                                    .with_field(format!("{}.interval_count", price_prefix))
                                    .with_expected("A positive integer (default: 1)")
                                    .with_received(count.to_string()),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    diagnostics
}

/// Validate a catalog request and return all diagnostics
fn validate_catalog_request(request: &CatalogRequest) -> ValidationResult {
    let mut diagnostics = Vec::new();

    // Validate project_root_dir
    if request.project_root_dir.trim().is_empty() {
        diagnostics.push(
            ValidationDiagnostic::error(
                "missing-required-field",
                "The 'project_root_dir' field is empty",
            )
            .with_field("project_root_dir")
            .with_expected("An absolute path to the project root directory")
            .with_suggestion("Provide the full path like '/path/to/user/project'"),
        );
    }

    // Validate products array
    if request.products.is_empty() {
        diagnostics.push(
            ValidationDiagnostic::error("missing-required-field", "The 'products' array is empty")
                .with_field("products")
                .with_expected("At least one ProductSchema object")
                .with_suggestion("Add at least one product to the products array"),
        );
    } else {
        // Validate each product
        for (i, product) in request.products.iter().enumerate() {
            diagnostics.extend(validate_product_schema(product, i));
        }
    }

    ValidationResult::from_diagnostics(diagnostics)
}

// ============================================================================
// Tool Implementation
// ============================================================================

#[tool_router]
impl MoneyMqMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    #[tool(description = r#"
Creates product catalog YAML files for MoneyMQ billing using ProductSchema.

## ProductSchema Structure

Each product must include:
- `id`: Unique product identifier (string)
- `name`: Product display name (string)
- `prices`: Array of price objects (required, at least one)

Optional fields:
- `description`: Product description
- `active`: Whether product is active (default: true)
- `product_type`: Type like "service" or "good"
- `statement_descriptor`: Text on credit card statements
- `unit_label`: Like "per seat" or "per GB"
- `images`: Array of image URLs
- `metadata`: Custom key-value data

## PriceSchema (Tagged Union)

Prices use `pricing_type` as the discriminator tag:

### One-Time Price:
```json
{
    "pricing_type": "one_time",
    "currency": "usd",
    "unit_amount": 999
}
```

### Recurring Price:
```json
{
    "pricing_type": "recurring",
    "currency": "usd",
    "unit_amount": 999,
    "interval": "month",
    "interval_count": 1
}
```

## Valid Enum Values
- `currency`: "usd", "eur", "gbp"
- `pricing_type`: "one_time", "recurring"
- `interval`: "day", "week", "month", "year"

## Complete Example (Recurring Subscription):
```json
{
    "project_root_dir": "/path/to/project",
    "products": [{
        "id": "prod_premium",
        "name": "Premium Plan",
        "description": "Access to all premium features",
        "product_type": "service",
        "prices": [{
            "pricing_type": "recurring",
            "currency": "usd",
            "unit_amount": 1999,
            "interval": "month"
        }],
        "metadata": {
            "features": ["unlimited_api", "priority_support"]
        }
    }]
}
```

## Complete Example (One-Time Payment):
```json
{
    "project_root_dir": "/path/to/project",
    "products": [{
        "id": "prod_lifetime",
        "name": "Lifetime License",
        "prices": [{
            "pricing_type": "one_time",
            "currency": "usd",
            "unit_amount": 9900
        }]
    }]
}
```

Writes each product as `<id>.yaml` in the products directory.
    "#)]
    async fn add_product_to_catalog(
        &self,
        Parameters(request): Parameters<CatalogRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Step 1: Validate the entire request
        let validation = validate_catalog_request(&request);

        if !validation.is_valid {
            // Return detailed validation errors for the LLM to iterate
            let error_details = json!({
                "error_type": "validation_failed",
                "error_count": validation.error_count,
                "warning_count": validation.warning_count,
                "diagnostics": validation.diagnostics,
                "hint": "Please review the errors above and fix each issue in your request. All required fields must be provided with valid values.",
                "valid_enums": {
                    "currency": ["usd", "eur", "gbp"],
                    "pricing_type": ["one_time", "recurring"],
                    "interval": ["day", "week", "month", "year"]
                }
            });

            let error_msg = format!(
                "## Validation Failed\n\n{} error(s) found in your request.\n{}",
                validation.error_count,
                validation.format_for_llm()
            );

            let full_error_msg = format!(
                "{}\n\n---\n## Machine-Readable Diagnostics\n```json\n{}\n```",
                error_msg,
                serde_json::to_string_pretty(&error_details).unwrap_or_default()
            );

            return Ok(CallToolResult::error(vec![Content::text(&full_error_msg)]));
        }

        // Step 2: Note any warnings
        let warning_msg = if validation.warning_count > 0 {
            Some(validation.format_for_llm())
        } else {
            None
        };

        let CatalogRequest {
            project_root_dir,
            products,
        } = request;
        let project_path = PathBuf::from(&project_root_dir);

        // Read manifest to get catalog path
        let manifest_path = project_path.join(moneymq_types::MANIFEST_FILE_NAME);
        if !manifest_path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(&format!(
                "## Error: Manifest Not Found\n\n`{}` not found at `{}`.\n\n**Solution:** Run `moneymq init` first to initialize your project.",
                moneymq_types::MANIFEST_FILE_NAME,
                project_root_dir
            ))]));
        }

        let manifest_content = fs::read_to_string(&manifest_path).map_err(|e| {
            tracing::error!(?e, "Failed to read manifest");
            McpError::internal_error(
                "Failed to read manifest",
                Some(json!({"error": e.to_string()})),
            )
        })?;

        let manifest: Manifest = serde_yml::from_str(&manifest_content).map_err(|e| {
            tracing::error!(?e, "Failed to parse manifest");
            McpError::internal_error(
                "Failed to parse manifest",
                Some(json!({"error": e.to_string()})),
            )
        })?;

        // Get first catalog's path
        let (_catalog_name, catalog_config) = manifest.catalogs.first().ok_or_else(|| {
            McpError::invalid_request(
                "No catalogs found in manifest. Please run 'moneymq init' first.".to_string(),
                None,
            )
        })?;

        let is_stripe = catalog_config.source_type == "stripe";

        // Create products directory
        let products_path = project_path
            .join(&catalog_config.catalog_path)
            .join("products");

        fs::create_dir_all(&products_path).map_err(|e| {
            tracing::error!(?e, "Failed to create products directory");
            McpError::internal_error(
                "Failed to create products directory",
                Some(json!({"error": e.to_string()})),
            )
        })?;

        let mut created_files = Vec::new();

        for product in products {
            // Serialize ProductSchema directly to YAML
            let yaml_content = to_pretty_yaml_with_header(&product, Some("Product"), Some("v1"))
                .map_err(|e| {
                    tracing::error!(?e, "Failed to serialize product");
                    McpError::internal_error(
                        "Failed to serialize product",
                        Some(json!({"error": e.to_string()})),
                    )
                })?;

            tracing::info!(product_id = %product.id, product_name = %product.name, "Creating product");

            // Generate base58 filename from product ID
            let filename_base58 = bs58::encode(&product.id).into_string();
            let product_path = products_path.join(format!("{}.yaml", filename_base58));

            tracing::info!("Writing product to file: {}", product_path.display());
            fs::write(&product_path, yaml_content).map_err(|e| {
                tracing::error!(?e, "Failed to write product");
                McpError::internal_error(
                    "Failed to write product",
                    Some(json!({
                        "error": format!("Failed to write product to {}: {}", product_path.display(), e)
                    })),
                )
            })?;

            created_files.push(product_path.display().to_string());
        }

        // Build success message with next steps
        let mut success_msg = if is_stripe {
            format!(
                "✓ Created {} product(s) in {}\n\nNext steps:\n\n1. Sync to Stripe:\n   moneymq catalog sync\n\n2. Or run MoneyMQ Studio to test:\n   moneymq run",
                created_files.len(),
                products_path.display()
            )
        } else {
            format!(
                "✓ Created {} product(s) in {}\n\nNext step:\nRun MoneyMQ Studio to test your products:\n   moneymq sandbox",
                created_files.len(),
                products_path.display()
            )
        };

        // Append any warnings
        if let Some(warnings) = warning_msg {
            success_msg.push_str(&format!("\n\n{}", warnings));
        }

        Ok(CallToolResult::success(vec![Content::text(&success_msg)]))
    }
}

#[prompt_router]
impl MoneyMqMcp {}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for MoneyMqMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder()
                .enable_prompts()
                .enable_resources()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(r#"MoneyMQ Product Catalog MCP Server

## Available Tools

### add_product_to_catalog
Creates product catalog YAML files using ProductSchema. Validates all input with detailed error reporting.

## ProductSchema Structure

Products require:
- id: Unique identifier (e.g., "prod_premium")
- name: Display name
- prices: Array of PriceSchema objects (at least one)

## PriceSchema (Tagged Union by pricing_type)

One-time price:
```json
{"pricing_type": "one_time", "currency": "usd", "unit_amount": 999}
```

Recurring price:
```json
{"pricing_type": "recurring", "currency": "usd", "unit_amount": 999, "interval": "month"}
```

## Valid Enum Values
- currency: "usd", "eur", "gbp"
- pricing_type: "one_time", "recurring"
- interval (for recurring): "day", "week", "month", "year"

## Handling Errors

When you receive a validation error:
1. Check the `field` path to identify the issue location
2. Check `expected` for what value was expected
3. Check `received` for what was provided
4. Check `suggestion` for how to fix it
5. Fix ALL errors before retrying

## Common Issues
- Missing `prices` array - every product needs at least one price
- Invalid enum value - use exact lowercase values
- Empty `id` or `name` - these are required strings

## Resources
- add_product_to_catalog_schema: JSON Schema for CatalogRequest
"#.to_string()),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![RawResource {
                title: Some("Add Product to Catalog Schema".to_string()),
                uri: "str:///add_product_to_catalog_schema".to_string(),
                name: "Schema for all types needed for an `add_product_to_catalog` request".to_string(),
                description: Some("A json file containing the schema for all types needed for an `add_product_to_catalog` request, including ProductSchema and PriceSchema".to_string()),
                mime_type: Some("application/json".to_string()),
                size: None,
                icons: None
            }
            .no_annotation()],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        ReadResourceRequestParam { uri }: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        match uri.as_str() {
            "str:///add_product_to_catalog_schema" => {
                let schema = schemars::schema_for!(CatalogRequest);
                let schema_json = serde_json::to_string_pretty(&schema).map_err(|e| {
                    McpError::internal_error(
                        "Failed to serialize schema to JSON",
                        Some(json!({
                            "error": e.to_string()
                        })),
                    )
                })?;
                tracing::debug!(?schema_json, "Retrieved add_product_to_catalog schema");
                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::TextResourceContents {
                        uri: uri.clone(),
                        mime_type: Some("application/json".to_string()),
                        text: schema_json,
                        meta: None,
                    }],
                })
            }
            _ => Err(McpError::resource_not_found(
                "resource_not_found",
                Some(json!({
                    "uri": uri
                })),
            )),
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: Vec::new(),
        })
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        if let Some(http_request_part) = context.extensions.get::<axum::http::request::Parts>() {
            let initialize_headers = &http_request_part.headers;
            let initialize_uri = &http_request_part.uri;
            tracing::info!(?initialize_headers, %initialize_uri, "initialize from http server");
        }
        Ok(self.get_info())
    }
}
