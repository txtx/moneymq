//! Embedded example catalogs for demo and testing
//!
//! This module provides pre-built catalog examples that can be loaded
//! without requiring files on disk.

use moneymq_types::{Product, merge_product_with_variant};

use super::loader::json_to_product;

// Embed the Weather API example catalog
const WEATHER_BASE: &str = include_str!("examples/weather/service.yaml");
const WEATHER_STARTER: &str = include_str!("examples/weather/variants/starter.yaml");
const WEATHER_PRO: &str = include_str!("examples/weather/variants/pro.yaml");
const WEATHER_ENTERPRISE: &str = include_str!("examples/weather/variants/enterprise.yaml");

/// Load the Weather API example catalog
///
/// Returns a vector of products representing a Weather API service
/// with three tiers: Starter (free), Pro ($29/mo), and Enterprise ($199/mo).
pub fn load_weather_example() -> Result<Vec<Product>, String> {
    let mut products = Vec::new();

    // Load each variant by merging with base
    let variants = [
        ("starter", WEATHER_STARTER),
        ("pro", WEATHER_PRO),
        ("enterprise", WEATHER_ENTERPRISE),
    ];

    for (variant_name, variant_content) in variants {
        let merged =
            merge_product_with_variant(WEATHER_BASE, variant_content, "weather", variant_name)?;

        let product = json_to_product(merged)?;
        products.push(product);
    }

    Ok(products)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_weather_example() {
        let products = load_weather_example().expect("Failed to load weather example");

        assert_eq!(products.len(), 3);

        // Find each tier
        let starter = products.iter().find(|p| p.id == "weather-starter").unwrap();
        let pro = products.iter().find(|p| p.id == "weather-pro").unwrap();
        let enterprise = products
            .iter()
            .find(|p| p.id == "weather-enterprise")
            .unwrap();

        // Check names
        assert_eq!(starter.name, Some("Weather Starter".to_string()));
        assert_eq!(pro.name, Some("Weather Pro".to_string()));
        assert_eq!(enterprise.name, Some("Weather Enterprise".to_string()));

        // Check prices (converted to cents)
        assert_eq!(starter.prices[0].unit_amount, Some(0));
        assert_eq!(pro.prices[0].unit_amount, Some(2900)); // $29.00
        assert_eq!(enterprise.prices[0].unit_amount, Some(19900)); // $199.00

        // Check features are merged
        assert!(starter.features.contains_key("requests_per_day"));
        assert!(starter.features.contains_key("forecast_days"));
        assert!(starter.features.contains_key("historical_data"));
        assert!(starter.features.contains_key("alerts"));

        // Check feature values
        let starter_requests = &starter.features["requests_per_day"];
        assert_eq!(starter_requests.name, Some("Daily Requests".to_string())); // from base
        assert_eq!(starter_requests.value, Some(serde_json::json!(100))); // from variant

        let pro_forecast = &pro.features["forecast_days"];
        assert_eq!(pro_forecast.value, Some(serde_json::json!(14)));

        let enterprise_requests = &enterprise.features["requests_per_day"];
        assert_eq!(enterprise_requests.value, Some(serde_json::json!(-1))); // unlimited
    }
}
