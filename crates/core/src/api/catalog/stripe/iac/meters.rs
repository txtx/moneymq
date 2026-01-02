use anyhow::Result;
use chrono::DateTime;
use indexmap::IndexMap;
use moneymq_types::{
    Meter as MoneymqMeter, MeterAggregation, MeterCollection, MeterCustomerMapping,
    MeterValueSettings,
};

use super::{super::utils::timestamp_to_datetime, common::generate_base58_id};

/// Convert Stripe meter JSON to MoneyMQ Meter
fn convert_stripe_meter(
    meter_json: &serde_json::Value,
    is_production: bool,
) -> Result<MoneymqMeter> {
    let stripe_id = meter_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing meter ID"))?
        .to_string();

    let base58_id = generate_base58_id(&stripe_id);

    let (deployed_id, sandboxes) = if is_production {
        (Some(stripe_id), IndexMap::new())
    } else {
        // For sandbox, create an IndexMap with "default" sandbox
        let mut sandbox_map = IndexMap::new();
        sandbox_map.insert("default".to_string(), stripe_id);
        (None, sandbox_map)
    };

    let created = timestamp_to_datetime(meter_json.get("created").and_then(|v| v.as_i64()));

    let updated = meter_json
        .get("updated")
        .and_then(|v| v.as_i64())
        .and_then(|ts| DateTime::from_timestamp(ts, 0));

    let display_name = meter_json
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let event_name = meter_json
        .get("event_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing event_name"))?
        .to_string();

    let status = meter_json
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Parse customer_mapping
    let customer_mapping = meter_json.get("customer_mapping").and_then(|cm| {
        let mapping_type = cm.get("type")?.as_str()?.to_string();
        let event_payload_key = cm.get("event_payload_key")?.as_str()?.to_string();
        Some(MeterCustomerMapping {
            mapping_type,
            event_payload_key,
        })
    });

    // Parse default_aggregation
    let default_aggregation = meter_json.get("default_aggregation").and_then(|da| {
        let formula = da.get("formula")?.as_str()?.to_string();
        Some(MeterAggregation { formula })
    });

    // Parse value_settings
    let value_settings = meter_json.get("value_settings").and_then(|vs| {
        let event_payload_key = vs.get("event_payload_key")?.as_str()?.to_string();
        Some(MeterValueSettings { event_payload_key })
    });

    Ok(MoneymqMeter {
        id: base58_id,
        deployed_id,
        sandboxes,
        display_name,
        event_name,
        status,
        customer_mapping,
        default_aggregation,
        value_settings,
        created_at: created,
        updated_at: updated,
    })
}

/// Download all billing meters from Stripe
///
/// # Arguments
/// * `api_key` - Your Stripe secret API key
/// * `provider_name` - Name of the provider (e.g., "stripe")
/// * `is_production` - Whether this is production or sandbox
///
/// # Returns
/// `MeterCollection` containing all meters
///
/// # Example
/// ```ignore
/// let api_key = std::env::var("STRIPE_SECRET_KEY")?;
/// let meters = download_meters(&api_key, "stripe", true).await?;
/// println!("Downloaded {} meters", meters.total_count);
/// ```
pub async fn download_meters(
    api_key: &str,
    provider_name: &str,
    is_production: bool,
) -> Result<MeterCollection> {
    let mut meters = Vec::new();

    // Use reqwest to make a raw API call to Stripe billing meters endpoint
    // The stripe crate doesn't have billing meter support yet
    let http_client = reqwest::Client::new();

    let response = http_client
        .get("https://api.stripe.com/v1/billing/meters")
        .header("Authorization", format!("Bearer {}", api_key))
        .query(&[("limit", "100")])
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to fetch meters ({}): {}",
            status,
            error_body
        ));
    }

    let json: serde_json::Value = response.json().await?;

    if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
        for meter_json in data {
            if let Ok(meter) = convert_stripe_meter(meter_json, is_production) {
                meters.push(meter);
            }
        }
    }

    Ok(MeterCollection::new(meters, provider_name.to_string()))
}
