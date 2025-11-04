use moneymq_types::Meter;
use serde::Serialize;

/// Stripe-compatible billing meter response
#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct StripeMeterCustomerMapping {
    #[serde(rename = "type")]
    pub mapping_type: String,
    pub event_payload_key: String,
}

#[derive(Debug, Serialize)]
pub struct StripeMeterAggregation {
    pub formula: String,
}

#[derive(Debug, Serialize)]
pub struct StripeMeterValueSettings {
    pub event_payload_key: String,
}

/// Stripe-compatible meter event response
#[derive(Debug, Serialize)]
pub struct StripeMeterEvent {
    pub id: String,
    pub object: String,
    pub event_name: String,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
}

impl StripeBillingMeter {
    /// Convert MoneyMQ Meter to Stripe Billing Meter with sandbox/production context
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
            default_aggregation: meter.default_aggregation.as_ref().map(|da| {
                StripeMeterAggregation {
                    formula: da.formula.clone(),
                }
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
