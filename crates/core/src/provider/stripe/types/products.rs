use moneymq_types::Product;
use serde::Serialize;

/// Stripe-compatible product response
#[derive(Debug, Serialize)]
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
        }
    }
}
