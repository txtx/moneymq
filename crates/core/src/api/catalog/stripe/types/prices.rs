use moneymq_types::{Product, iac::PricingType};
use serde::Serialize;

/// Stripe-compatible price response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StripePrice {
    pub id: String,
    pub object: String,
    pub active: bool,
    pub currency: String,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub product: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring: Option<StripeRecurring>,
    #[serde(rename = "type")]
    pub pricing_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_amount: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StripeRecurring {
    pub interval: String,
    pub interval_count: i64,
}

impl StripePrice {
    /// Convert MoneyMQ Price to Stripe Price with sandbox/production context
    pub fn from_price_and_product(
        price: &moneymq_types::Price,
        product: &Product,
        use_sandbox: bool,
    ) -> Self {
        let external_id = if use_sandbox {
            price.sandboxes.get("default")
        } else {
            price.deployed_id.as_ref()
        };

        let product_external_id = if use_sandbox {
            product.sandboxes.get("default")
        } else {
            product.deployed_id.as_ref()
        };

        let recurring = if price.pricing_type == PricingType::Recurring {
            Some(StripeRecurring {
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
            id: external_id.cloned().unwrap_or_else(|| price.id.clone()),
            object: "price".to_string(),
            active: price.active,
            currency: price.currency.as_str().to_string(),
            created: price.created_at.timestamp(),
            metadata: if price.metadata.is_empty() {
                None
            } else {
                serde_json::to_value(&price.metadata).ok()
            },
            nickname: price.nickname.clone(),
            product: product_external_id
                .cloned()
                .unwrap_or_else(|| product.id.clone()),
            recurring,
            pricing_type: price.pricing_type.as_str().to_string(),
            unit_amount: price.unit_amount,
        }
    }
}
