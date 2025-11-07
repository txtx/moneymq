use convert_case::{Case, Casing};
use std::collections::HashMap;
use std::{fs, path::PathBuf};

use moneymq_types::{Price, Product};
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
use serde_json::json;

use crate::yaml_util::to_pretty_yaml_with_header;

#[derive(Clone)]
pub struct MoneyMqMcp {
    tool_router: ToolRouter<MoneyMqMcp>,
    prompt_router: PromptRouter<MoneyMqMcp>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[schemars(example = r#"{
    "project_root_dir": "/path/to/user/project",
    "products": [
        {
            "name": "Premium Subscription",
            "description": "Access to premium features",
            "features": [
                {
                    "name": "Number of Networks",
                    "description": "The number of networks you can run in the cloud",
                    "feature_group": "Network Features",
                    "value": "5 networks",
                },
                {
                    "name": "Number of Requests",
                    "description": "The number of requests that can be made",
                    "feature_group": "Network Features",
                    "value": "100 requests",
                },
                {
                    "name": "Email Support",
                    "description": "Email support available",
                    "feature_group": "Support Features",
                    "value": "Yes",
                },
                {
                    "name": "Community Support",
                    "description": "Community support available on Discord",
                    "feature_group": "Support Features",
                    "value": "Yes",
                }   
            ],
            "product_type": "service",
            "statement_descriptor": "Moneymq Premium",
            "unit_label": "per month",
            "amount": 999,
            "currency": "usd",
            "interval": "month",
            "interval_count": 1,
            "pricing_type": "recurring"
        }
    ]
}"#)]
pub struct CatalogRequest {
    #[schemars(
        description = "The root directory of the user's project where the catalogs directory will be created",
        example = "/path/to/user/project"
    )]
    pub project_root_dir: String,
    #[schemars(
        description = "List of products to create in the catalog",
        example = r#"[{
            "name": "Premium Subscription",
            "description": "Access to premium features",
            "product_type": "service",
            "statement_descriptor": "Moneymq Premium",
            "unit_label": "per month",
            "amount": 999,
            "currency": "usd",
            "interval": "month",
            "interval_count": 1,
            "pricing_type": "recurring"
        }]"#
    )]
    pub products: Vec<ProductRequest>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ProductRequest {
    #[schemars(description = "The name of the product")]
    pub name: Option<String>,
    #[schemars(description = "The description of the product")]
    pub description: Option<String>,
    #[schemars(
        description = "List of features that is associated with the product.",
        example = r#"[
            {
                "name": "Number of Networks",
                "description": "The number of networks you can run in the cloud",
                "feature_group": "Network Features",
                "value": "5 networks",
            },
            {
                "name": "Number of Requests",
                "description": "The number of requests that can be made",
                "feature_group": "Network Features",
                "value": "100 requests",
            },
            {
                "name": "Email Support",
                "description": "Email support available",
                "feature_group": "Support Features",
                "value": "Yes",
            },
            {
                "name": "Community Support",
                "description": "Community support available on Discord",
                "feature_group": "Support Features",
                "value": "Yes",
            }        
        ]"#
    )]
    pub features: Vec<ProductFeature>,
    #[schemars(description = "The type of the product")]
    pub product_type: Option<String>,
    #[schemars(
        description = "Statement descriptor that appears on a customer's credit card statement"
    )]
    pub statement_descriptor: Option<String>,
    #[schemars(description = "The unit label for the product (e.g., 'per user', 'per month')")]
    pub unit_label: Option<String>,
    #[schemars(
        description = "The amount to be charged in the smallest currency unit (e.g., cents)"
    )]
    pub amount: Option<i64>,
    #[schemars(description = "The currency code (e.g., 'usd')", example = &"usd")]
    pub currency: String,
    #[schemars(
        description = "The billing interval (e.g., 'month', 'year')",
        example = &"month"
    )]
    pub interval: Option<String>,
    #[schemars(description = "The number of intervals between each billing cycle")]
    pub interval_count: Option<i64>,
    #[schemars(description = "The pricing type (e.g., 'one_time', 'recurring')", example = &"recurring")]
    pub pricing_type: String,
}

impl Into<Product> for ProductRequest {
    fn into(self) -> Product {
        let ProductRequest {
            name,
            description,
            features,
            product_type,
            statement_descriptor,
            unit_label,
            amount,
            currency,
            interval,
            interval_count,
            pricing_type,
        } = self;
        let mut product = Product::new()
            .with_some_name(name)
            .with_some_description(description)
            .with_some_product_type(product_type)
            .with_some_statement_descriptor(statement_descriptor)
            .with_some_unit_label(unit_label)
            .add_price(
                Price::new(currency, pricing_type)
                    .with_some_amount(amount)
                    .with_some_interval(interval)
                    .with_some_interval_count(interval_count),
            );

        let mut feature_map = HashMap::new();
        for feature in features {
            let features_in_group = feature_map
                .entry(feature.feature_group.to_case(Case::Snake))
                .or_insert(Vec::new());

            features_in_group.push(json!({
                "name": feature.name,
                "description": feature.description,
                "value": feature.value,
                "key": feature.name.to_case(Case::Snake),
            }));
        }
        if !feature_map.is_empty() {
            feature_map.insert(
                "features".to_string(),
                feature_map
                    .keys()
                    .map(|k| serde_json::Value::String(k.clone()))
                    .collect(),
            );
            product.metadata = feature_map
                .into_iter()
                .map(|(k, v)| (k, serde_json::to_string(&v).unwrap()))
                .collect();
        }
        product
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[schemars(example = r#"{
    "name": "Number of Networks",
    "description": "The number of networks you can run in the cloud",
    "feature_group": "Network Features",
    "value": "5 networks",
}"#)]
pub struct ProductFeature {
    #[schemars(description = "The name of the feature", example = &"Number of Networks")]
    pub name: String,
    #[schemars(description = "The description of the feature", example = &"The number of networks you can run in the cloud")]
    pub description: Option<String>,
    #[schemars(description = "The feature group this feature belongs to", example = &"Network Features")]
    pub feature_group: String,
    #[schemars(description = "The value of the feature", example = &"5 networks")]
    pub value: String,
}

#[tool_router]
impl MoneyMqMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }
    #[tool(description = r#"
        Creates product catalog YAML files.

        Unless the user provides the project_root_dir, the directory MUST be the project's root directory.

        The features field MUST be provided.
        Product feature_mapping keys MUST correspond to feature key values.

        Good Example 1: {
            "project_root_dir": "/path/to/user/project",
            "products": [
                {
                    "name": "Premium Subscription",
                    "description": "Access to premium features",
                    "features": [
                        {
                            "name": "Number of Networks",
                            "description": "The number of networks you can run in the cloud",
                            "feature_group": "Network Features",
                            "value": "5 networks",
                        },
                        {
                            "name": "Number of Requests",
                            "description": "The number of requests that can be made",
                            "feature_group": "Network Features",
                            "value": "100 requests",
                        },
                        {
                            "name": "Email Support",
                            "description": "Email support available",
                            "feature_group": "Support Features",
                            "value": "Yes",
                        },
                        {
                            "name": "Community Support",
                            "description": "Community support available on Discord",
                            "feature_group": "Support Features",
                            "value": "Yes",
                        }   
                    ],
                    "product_type": "service",
                    "statement_descriptor": "Moneymq Premium",
                    "unit_label": "per month",
                    "amount": 999,
                    "currency": "usd",
                    "interval": "month",
                    "interval_count": 1,
                    "pricing_type": "recurring"
                }
            ]
        }

        Bad Example 1 (missing features): {
            "project_root_dir": "/path/to/user/project",
            "products": [
                {
                    "name": "Premium Subscription",
                    "description": "Access to premium features",
                    "product_type": "service",
                    "statement_descriptor": "Moneymq Premium",
                    "unit_label": "per month",
                    "amount": 999,
                    "currency": "usd",
                    "interval": "month",
                    "interval_count": 1,
                    "pricing_type": "recurring"
                }
            ]
        }

        Writes each product as `<id>.yaml` in the provided directory.
    "#)]
    async fn create_catalog(
        &self,
        Parameters(CatalogRequest {
            project_root_dir,
            products,
        }): Parameters<CatalogRequest>,
    ) -> Result<CallToolResult, McpError> {
        let root_path = PathBuf::from(&project_root_dir).join("catalogs");
        fs::create_dir_all(&root_path).map_err(|e| {
            tracing::error!(?e, "Failed to create catalogs directory");
            McpError::internal_error(
                "Failed to create catalogs directory",
                Some(json!({
                    "error": e.to_string()
                })),
            )
        })?;

        for product in products {
            let product: Product = product.into();

            let yaml_content = to_pretty_yaml_with_header(&product, Some("Product"), Some("v1"))
                .map_err(|e| {
                    tracing::error!(?e, "Failed to create product catalog");
                    McpError::internal_error(
                        "Failed to create product catalog",
                        Some(json!({
                            "error": e.to_string()
                        })),
                    )
                })?;

            tracing::info!(?product, "Creating product");

            let product_path = root_path.join(format!("{}.yaml", product.id));
            tracing::info!("Writing product to file: {}", product_path.display());
            fs::write(&product_path, yaml_content)
                .map_err(|e| {
                    tracing::error!(?e, "Failed to write product");
                    McpError::internal_error(
                        "Failed to write product",
                        Some(json!({
                            "error": format!("Failed to write product to {}: {}", product_path.display(), e)
                        })),
                    )
                })?;
        }

        Ok(CallToolResult::success(vec![]))
    }

    #[tool(description = r#"
        Takes the same input as a `create_catalog` request and returns true if the input is valid and returns an error otherwise.
        This can be used to validate a catalog request before attempting to create the catalog files.

        Input: {
            "directory": "/path/to/user/project/catalogs",
            "products": [
                {
                    "name": "Premium Subscription",
                    "description": "Access to premium features",
                    "features": [
                        {
                            "name": "Number of Networks",
                            "description": "The number of networks you can run in the cloud",
                            "feature_group": "Network Features",
                            "value": "5 networks",
                        },
                        {
                            "name": "Number of Requests",
                            "description": "The number of requests that can be made",
                            "feature_group": "Network Features",
                            "value": "100 requests",
                        },
                        {
                            "name": "Email Support",
                            "description": "Email support available",
                            "feature_group": "Support Features",
                            "value": "Yes",
                        },
                        {
                            "name": "Community Support",
                            "description": "Community support available on Discord",
                            "feature_group": "Support Features",
                            "value": "Yes",
                        }   
                    ],
                    "product_type": "service",
                    "statement_descriptor": "Moneymq Premium",
                    "unit_label": "per month",
                    "amount": 999,
                    "currency": "usd",
                    "interval": "month",
                    "interval_count": 1,
                    "pricing_type": "recurring"
                }
            ]
        }
    "#)]
    async fn validate_create_catalog_request(
        &self,
        Parameters(CatalogRequest {
            project_root_dir,
            products,
        }): Parameters<CatalogRequest>,
    ) -> Result<CallToolResult, McpError> {
        for product in products {
            let _: Product = product.into();
        }

        let root_path = PathBuf::from(&project_root_dir).join("catalogs");
        root_path.is_dir().then(|| ()).ok_or_else(|| {
            McpError::internal_error(
                "Invalid directory for catalog",
                Some(json!({
                    "error": format!("The provided directory '{}' is not valid", root_path.display())
                })),
            )
        })?;
        Ok(CallToolResult::success(vec![Content::text("true")]))
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
            instructions: Some("This server provides tools to interact with the MoneyMQ cli tool. This tool is used to create product catalogs (a la stripe), where you can manage YAML files that define your products and prices.
            Tools: create_catalog(takes a list of products to create in the catalog).".to_string()),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![RawResource {
                title: Some("Create Catalog Schema".to_string()),
                uri: "str:///create_catalog_schema".to_string(),
                name: "Schema for all types needed for a `create_catalog` request".to_string(),
                description: Some("A json file containing the schema for all types needed for a `create_catalog` request".to_string()),
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
            "str:///create_catalog_schema" => {
                let schema = schemars::schema_for!(CatalogRequest);
                let schema_json = serde_json::to_string_pretty(&schema).map_err(|e| {
                    McpError::internal_error(
                        "Failed to serialize schema to JSON",
                        Some(json!({
                            "error": e.to_string()
                        })),
                    )
                })?;
                tracing::debug!(?schema_json, "Retrieved create_catalog schema");
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
