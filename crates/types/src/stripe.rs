//! Stripe-compatible response types for the catalog API

use crate::{Meter, Price, Product};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Stripe-compatible list response
#[derive(Debug, Clone, Serialize)]
pub struct ListResponse<T> {
    pub object: String,
    pub data: Vec<T>,
    pub has_more: bool,
    pub url: String,
}

/// Query parameters for list endpoints
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub starting_after: Option<String>,
    pub product: Option<String>,
}

/// Stripe-compatible product response
#[derive(Debug, Clone, Serialize)]
pub struct StripeProduct {
    pub id: String,
    pub object: String,
    pub active: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_descriptor: Option<String>,
    pub images: Vec<String>,
    pub metadata: IndexMap<String, String>,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_price: Option<String>,
}

impl StripeProduct {
    pub fn from_product(product: &Product, use_sandbox: bool) -> Self {
        let external_id = if use_sandbox {
            product.sandboxes.get("default")
        } else {
            product.deployed_id.as_ref()
        };

        let default_price = product.prices.first().and_then(|p| {
            if use_sandbox {
                p.sandboxes.get("default").cloned()
            } else {
                p.deployed_id.clone()
            }
        });

        Self {
            id: external_id.cloned().unwrap_or_else(|| product.id.clone()),
            object: "product".to_string(),
            active: product.active,
            name: product.name.clone(),
            description: product.description.clone(),
            unit_label: product.unit_label.clone(),
            statement_descriptor: product.statement_descriptor.clone(),
            images: product.images.clone(),
            metadata: product.metadata.clone(),
            created: product.created_at.timestamp(),
            updated: product.updated_at.map(|dt| dt.timestamp()),
            product_type: product.product_type.clone(),
            default_price,
        }
    }
}

/// Stripe-compatible price response
#[derive(Debug, Clone, Serialize)]
pub struct StripePrice {
    pub id: String,
    pub object: String,
    pub active: bool,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_amount: Option<i64>,
    pub product: String,
    #[serde(rename = "type")]
    pub pricing_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub metadata: IndexMap<String, String>,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring: Option<StripeRecurring>,
}

/// Stripe-compatible recurring configuration
#[derive(Debug, Clone, Serialize)]
pub struct StripeRecurring {
    pub interval: String,
    pub interval_count: i64,
}

impl StripePrice {
    pub fn from_price_and_product(price: &Price, product: &Product, use_sandbox: bool) -> Self {
        let price_id = if use_sandbox {
            price.sandboxes.get("default")
        } else {
            price.deployed_id.as_ref()
        };

        let product_id = if use_sandbox {
            product.sandboxes.get("default")
        } else {
            product.deployed_id.as_ref()
        };

        let recurring = if price.pricing_type == "recurring" {
            Some(StripeRecurring {
                interval: price
                    .recurring_interval
                    .clone()
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
            currency: price.currency.clone(),
            unit_amount: price.unit_amount,
            product: product_id.cloned().unwrap_or_else(|| product.id.clone()),
            pricing_type: price.pricing_type.clone(),
            nickname: price.nickname.clone(),
            metadata: price.metadata.clone(),
            created: price.created_at.timestamp(),
            recurring,
        }
    }
}

/// Stripe-compatible billing meter response
#[derive(Debug, Clone, Serialize)]
pub struct StripeBillingMeter {
    pub id: String,
    pub object: String,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_mapping: Option<StripeMeterCustomerMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_aggregation: Option<StripeMeterAggregation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_settings: Option<StripeMeterValueSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
}

/// Stripe-compatible customer mapping for meters
#[derive(Debug, Clone, Serialize)]
pub struct StripeMeterCustomerMapping {
    #[serde(rename = "type")]
    pub mapping_type: String,
    pub event_payload_key: String,
}

/// Stripe-compatible aggregation settings for meters
#[derive(Debug, Clone, Serialize)]
pub struct StripeMeterAggregation {
    pub formula: String,
}

/// Stripe-compatible value settings for meters
#[derive(Debug, Clone, Serialize)]
pub struct StripeMeterValueSettings {
    pub event_payload_key: String,
}

impl StripeBillingMeter {
    pub fn from_meter(meter: &Meter, use_sandbox: bool) -> Self {
        let external_id = if use_sandbox {
            meter.sandboxes.get("default")
        } else {
            meter.deployed_id.as_ref()
        };

        Self {
            id: external_id.cloned().unwrap_or_else(|| meter.id.clone()),
            object: "billing.meter".to_string(),
            created: meter.created_at.timestamp(),
            display_name: meter.display_name.clone(),
            event_name: meter.event_name.clone(),
            status: meter.status.clone(),
            customer_mapping: meter.customer_mapping.as_ref().map(|cm| {
                StripeMeterCustomerMapping {
                    mapping_type: cm.mapping_type.clone(),
                    event_payload_key: cm.event_payload_key.clone(),
                }
            }),
            default_aggregation: meter
                .default_aggregation
                .as_ref()
                .map(|da| StripeMeterAggregation {
                    formula: da.formula.clone(),
                }),
            value_settings: meter
                .value_settings
                .as_ref()
                .map(|vs| StripeMeterValueSettings {
                    event_payload_key: vs.event_payload_key.clone(),
                }),
            updated: meter.updated_at.map(|dt| dt.timestamp()),
        }
    }
}
