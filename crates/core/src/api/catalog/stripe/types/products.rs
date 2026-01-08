use indexmap::IndexMap;
use moneymq_types::{Price, Product, ProductFeature};
use serde::Serialize;

/// Experiment configuration for A/B testing (Stripe API representation)
#[derive(Debug, Serialize, Clone)]
pub struct StripeExperimentConfig {
    /// Traffic exposure percentage (0.0 to 1.0)
    pub exposure: f64,
}

/// Recurring interval configuration for subscription prices
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StripeProductPriceRecurring {
    pub interval: String,
    pub interval_count: i64,
}

/// Inline price for product response (simplified version of StripePrice)
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StripeProductPrice {
    pub id: String,
    pub object: String,
    pub active: bool,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_amount: Option<i64>,
    #[serde(rename = "type")]
    pub pricing_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring: Option<StripeProductPriceRecurring>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
}

impl StripeProductPrice {
    pub fn from_price(price: &Price, use_sandbox: bool) -> Self {
        let price_id = if use_sandbox {
            price.sandboxes.get("default")
        } else {
            price.deployed_id.as_ref()
        };

        let recurring = if price.pricing_type == moneymq_types::iac::PricingType::Recurring {
            Some(StripeProductPriceRecurring {
                interval: price
                    .recurring_interval
                    .as_ref()
                    .map(|i| i.as_str().to_string())
                    .unwrap_or_else(|| "month".to_string()),
                interval_count: price.recurring_interval_count.unwrap_or(1),
            })
        } else {
            None
        };

        Self {
            id: price_id.cloned().unwrap_or_else(|| price.id.clone()),
            object: "price".to_string(),
            active: price.active,
            currency: price.currency.as_str().to_string(),
            unit_amount: price.unit_amount,
            pricing_type: price.pricing_type.as_str().to_string(),
            recurring,
            nickname: price.nickname.clone(),
        }
    }
}

/// Stripe-compatible product response
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StripeProduct {
    pub id: String,
    pub object: String,
    pub active: bool,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_descriptor: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
    /// Experiment configuration (exposure percentage) - set when an experiment variant was selected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment: Option<StripeExperimentConfig>,
    /// Selected experiment variant ID (only set when an experiment was materialized)
    /// e.g., "surfnet-lite#a" while id is "surfnet-lite"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    /// Product features
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<IndexMap<String, ProductFeature>>,
    /// Default price for this product
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_price: Option<StripeProductPrice>,
}

impl StripeProduct {
    /// Convert MoneyMQ Product to Stripe Product with sandbox/production context
    pub fn from_product(product: &Product, use_sandbox: bool) -> Self {
        let external_id = if use_sandbox {
            product.sandboxes.get("default")
        } else {
            product.deployed_id.as_ref()
        };

        Self {
            id: external_id.cloned().unwrap_or_else(|| product.id.clone()),
            object: "product".to_string(),
            active: product.active,
            created: product.created_at.timestamp(),
            description: product.description.clone(),
            images: if product.images.is_empty() {
                None
            } else {
                Some(product.images.clone())
            },
            metadata: if product.metadata.is_empty() {
                None
            } else {
                serde_json::to_value(&product.metadata).ok()
            },
            name: product.name.clone(),
            statement_descriptor: product.statement_descriptor.clone(),
            product_type: product.product_type.clone(),
            unit_label: product.unit_label.clone(),
            updated: product.updated_at.map(|dt| dt.timestamp()),
            experiment: product.experiment.as_ref().map(|e| StripeExperimentConfig {
                exposure: e.exposure,
            }),
            experiment_id: None,
            features: if product.features.is_empty() {
                None
            } else {
                Some(product.features.clone())
            },
            default_price: product
                .prices
                .first()
                .map(|p| StripeProductPrice::from_price(p, use_sandbox)),
        }
    }

    /// Create a materialized product from a parent product with a selected experiment variant
    /// The returned product has:
    /// - id: parent product ID (e.g., "surfnet-lite")
    /// - experiment_id: the selected experiment variant ID (e.g., "surfnet-lite#a")
    /// - All other fields (features, metadata, prices, etc.): from the experiment variant (overrides parent)
    pub fn from_product_with_experiment(
        parent: &Product,
        experiment: &Product,
        use_sandbox: bool,
    ) -> Self {
        // Use parent's external ID for the product ID
        let external_id = if use_sandbox {
            parent.sandboxes.get("default")
        } else {
            parent.deployed_id.as_ref()
        };

        Self {
            // Use parent's ID (the stable product identifier)
            id: external_id.cloned().unwrap_or_else(|| parent.id.clone()),
            object: "product".to_string(),
            // All other fields come from the experiment variant (overrides parent)
            active: experiment.active,
            created: experiment.created_at.timestamp(),
            description: experiment
                .description
                .clone()
                .or_else(|| parent.description.clone()),
            images: if experiment.images.is_empty() {
                if parent.images.is_empty() {
                    None
                } else {
                    Some(parent.images.clone())
                }
            } else {
                Some(experiment.images.clone())
            },
            metadata: if experiment.metadata.is_empty() {
                if parent.metadata.is_empty() {
                    None
                } else {
                    serde_json::to_value(&parent.metadata).ok()
                }
            } else {
                serde_json::to_value(&experiment.metadata).ok()
            },
            name: experiment.name.clone().or_else(|| parent.name.clone()),
            statement_descriptor: experiment
                .statement_descriptor
                .clone()
                .or_else(|| parent.statement_descriptor.clone()),
            product_type: experiment
                .product_type
                .clone()
                .or_else(|| parent.product_type.clone()),
            unit_label: experiment
                .unit_label
                .clone()
                .or_else(|| parent.unit_label.clone()),
            updated: experiment.updated_at.map(|dt| dt.timestamp()),
            // Include the experiment config from the selected variant
            experiment: experiment
                .experiment
                .as_ref()
                .map(|e| StripeExperimentConfig {
                    exposure: e.exposure,
                }),
            // Set experiment_id to the selected experiment variant's ID
            experiment_id: Some(experiment.id.clone()),
            // Use experiment's features (overrides parent), fall back to parent if empty
            features: if experiment.features.is_empty() {
                if parent.features.is_empty() {
                    None
                } else {
                    Some(parent.features.clone())
                }
            } else {
                Some(experiment.features.clone())
            },
            // Use experiment's price (overrides parent), fall back to parent if empty
            default_price: experiment
                .prices
                .first()
                .map(|p| StripeProductPrice::from_price(p, use_sandbox))
                .or_else(|| {
                    parent
                        .prices
                        .first()
                        .map(|p| StripeProductPrice::from_price(p, use_sandbox))
                }),
        }
    }
}
