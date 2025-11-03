use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use moneymq_types::{
    Catalog, Meter as MoneymqMeter, MeterAggregation, MeterCollection, MeterCustomerMapping,
    MeterValueSettings, Price as MoneymqPrice, Product as MoneymqProduct,
};
use sha2::{Digest, Sha256};
use stripe::{
    Client, ListPrices, ListProducts, Price as StripePrice, PriceId, Product as StripeProduct,
    ProductId,
};

/// Convert Stripe metadata (HashMap) to IndexMap with sorted keys for consistent ordering
fn metadata_to_sorted_indexmap(metadata: HashMap<String, String>) -> IndexMap<String, String> {
    let mut keys: Vec<_> = metadata.keys().collect();
    keys.sort();
    keys.into_iter()
        .map(|k| (k.clone(), metadata.get(k).unwrap().clone()))
        .collect()
}

/// Information about a Stripe account
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub business_name: Option<String>,
    pub display_name: Option<String>,
    pub account_id: String,
    pub is_test: bool,
}

/// Retrieve account information from Stripe
///
/// # Arguments
/// * `api_key` - Your Stripe secret API key
///
/// # Returns
/// `AccountInfo` containing account details
///
/// # Example
/// ```no_run
/// use moneymq_driver_stripe::get_account_info;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let api_key = std::env::var("STRIPE_SECRET_KEY")?;
///     let account_info = get_account_info(&api_key).await?;
///     println!("Account: {}", account_info.display_name.unwrap_or_default());
///     Ok(())
/// }
/// ```
pub async fn get_account_info(api_key: &str) -> Result<AccountInfo> {
    // Use the account endpoint directly via HTTP
    let http_client = reqwest::Client::new();
    let response = http_client
        .get("https://api.stripe.com/v1/account")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to fetch account info ({}): {}",
            status,
            error_body
        ));
    }

    let account_json: serde_json::Value = response.json().await?;
    let is_test = api_key.starts_with("sk_test_") || api_key.starts_with("rk_test_");

    // Extract account information from JSON
    let business_name = account_json
        .get("business_profile")
        .and_then(|bp| bp.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let display_name = account_json
        .get("settings")
        .and_then(|s| s.get("dashboard"))
        .and_then(|d| d.get("display_name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let account_id = account_json
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(AccountInfo {
        business_name,
        display_name,
        account_id,
        is_test,
    })
}

/// Generate a base58-encoded ID from a Stripe product ID
/// Uses SHA256 hash to ensure consistent length and valid base58 characters
fn generate_base58_id(stripe_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(stripe_id.as_bytes());
    let hash = hasher.finalize();

    // Take first 16 bytes of hash for a reasonable ID length
    bs58::encode(&hash[..16]).into_string()
}

/// Convert Stripe Price to MoneyMQ Price
fn convert_price(stripe_price: StripePrice, is_production: bool) -> MoneymqPrice {
    let created_at = stripe_price
        .created
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(|| Utc::now());

    let stripe_id = stripe_price.id.to_string();
    let base58_id = generate_base58_id(&stripe_id);

    let (pricing_type, recurring_interval, recurring_interval_count) =
        if let Some(recurring) = stripe_price.recurring {
            (
                "recurring".to_string(),
                Some(format!("{:?}", recurring.interval).to_lowercase()),
                Some(recurring.interval_count as i64),
            )
        } else {
            ("one_time".to_string(), None, None)
        };

    use indexmap::IndexMap;

    let (deployed_id, sandboxes) = if is_production {
        (Some(stripe_id), IndexMap::new())
    } else {
        // For sandbox, create an IndexMap with "default" sandbox
        let mut sandbox_map = IndexMap::new();
        sandbox_map.insert("default".to_string(), stripe_id);
        (None, sandbox_map)
    };

    MoneymqPrice {
        id: base58_id,
        deployed_id,
        sandboxes,
        active: stripe_price.active.unwrap_or(true),
        currency: stripe_price.currency.unwrap_or_default().to_string(),
        unit_amount: stripe_price.unit_amount,
        pricing_type,
        recurring_interval,
        recurring_interval_count,
        nickname: stripe_price.nickname,
        metadata: metadata_to_sorted_indexmap(stripe_price.metadata.unwrap_or_default()),
        created_at,
    }
}

/// Convert Stripe Product to MoneyMQ Product
fn convert_product(
    stripe_product: StripeProduct,
    is_production: bool,
    prices: Vec<MoneymqPrice>,
) -> MoneymqProduct {
    use indexmap::IndexMap;

    let created_at = stripe_product
        .created
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(|| Utc::now());

    let updated_at = stripe_product
        .updated
        .and_then(|ts| DateTime::from_timestamp(ts, 0));

    let stripe_id = stripe_product.id.to_string();
    let base58_id = generate_base58_id(&stripe_id);

    let (deployed_id, sandboxes) = if is_production {
        (Some(stripe_id), IndexMap::new())
    } else {
        // For sandbox, create an IndexMap with "default" sandbox
        let mut sandbox_map = IndexMap::new();
        sandbox_map.insert("default".to_string(), stripe_id);
        (None, sandbox_map)
    };

    MoneymqProduct {
        id: base58_id,
        deployed_id,
        sandboxes,
        name: stripe_product.name,
        description: stripe_product.description,
        active: stripe_product.active.unwrap_or(true),
        metadata: metadata_to_sorted_indexmap(stripe_product.metadata.unwrap_or_default()),
        created_at,
        updated_at,
        product_type: stripe_product.type_.map(|t| format!("{:?}", t)),
        images: stripe_product.images.unwrap_or_default(),
        statement_descriptor: stripe_product.statement_descriptor,
        unit_label: stripe_product.unit_label,
        prices,
    }
}

/// Downloads the complete product catalog from Stripe
///
/// # Arguments
/// * `api_key` - Your Stripe secret API key
/// * `provider_name` - Name of the provider configuration (e.g., "stripe", "stripe_sandbox")
/// * `is_production` - Whether this is a production environment (affects which external_id field is populated)
///
/// # Returns
/// A `Catalog` containing all products from your Stripe account
///
/// # Example
/// ```no_run
/// use moneymq_driver_stripe::download_catalog;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let api_key = std::env::var("STRIPE_SECRET_KEY")?;
///     let catalog = download_catalog(&api_key, "stripe", true).await?;
///     println!("Downloaded {} products", catalog.total_count);
///     Ok(())
/// }
/// ```
pub async fn download_catalog(
    api_key: &str,
    provider_name: &str,
    is_production: bool,
) -> Result<Catalog> {
    let client = Client::new(api_key);
    let mut all_stripe_products = Vec::new();
    let mut starting_after: Option<ProductId> = None;

    // Fetch all products
    loop {
        let mut params = ListProducts::new();
        params.limit = Some(100); // Maximum allowed by Stripe API

        if let Some(ref last_id) = starting_after {
            params.starting_after = Some(last_id.clone());
        }

        let response = StripeProduct::list(&client, &params).await?;
        let has_more = response.has_more;

        if let Some(last_product) = response.data.last() {
            starting_after = Some(last_product.id.clone());
        }

        all_stripe_products.extend(response.data);

        if !has_more {
            break;
        }
    }

    // Convert Stripe products to MoneyMQ products, fetching prices for each
    let mut products = Vec::new();
    for stripe_product in all_stripe_products {
        let product_id = stripe_product.id.clone();

        // Fetch all prices for this product
        let mut product_prices = Vec::new();
        let mut price_starting_after: Option<PriceId> = None;

        loop {
            let mut price_params = ListPrices::new();
            price_params.product = Some(stripe::IdOrCreate::Id(&product_id));
            price_params.limit = Some(100);

            if let Some(ref last_id) = price_starting_after {
                price_params.starting_after = Some(last_id.clone());
            }

            let price_response = StripePrice::list(&client, &price_params).await?;
            let has_more = price_response.has_more;

            if let Some(last_price) = price_response.data.last() {
                price_starting_after = Some(last_price.id.clone());
            }

            product_prices.extend(
                price_response
                    .data
                    .into_iter()
                    .map(|p| convert_price(p, is_production)),
            );

            if !has_more {
                break;
            }
        }

        products.push(convert_product(
            stripe_product,
            is_production,
            product_prices,
        ));
    }

    Ok(Catalog::new(products, provider_name.to_string()))
}

/// Update a product in Stripe with local changes
///
/// # Arguments
/// * `api_key` - Your Stripe secret API key
/// * `external_id` - The Stripe product ID to update
/// * `local_product` - The local product with changes to apply
///
/// # Returns
/// Updated `Product` from Stripe
pub async fn update_product(
    api_key: &str,
    external_id: &str,
    local_product: &MoneymqProduct,
) -> Result<()> {
    use stripe::UpdateProduct;

    let client = Client::new(api_key);
    let product_id: ProductId = external_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid product ID: {}", e))?;

    let mut params = UpdateProduct::new();

    // Update fields that have values
    if let Some(name) = &local_product.name {
        params.name = Some(name.as_str());
    }

    if let Some(description) = &local_product.description {
        params.description = Some(description.to_string());
    }

    params.active = Some(local_product.active);

    // Update metadata (convert IndexMap to HashMap for Stripe API)
    if !local_product.metadata.is_empty() {
        params.metadata = Some(
            local_product
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
    }

    StripeProduct::update(&client, &product_id, params).await?;

    Ok(())
}

/// Create a new product in Stripe
///
/// # Arguments
/// * `api_key` - Your Stripe secret API key
/// * `local_product` - The local product to create
///
/// # Returns
/// The Stripe product ID of the created product
pub async fn create_product(api_key: &str, local_product: &MoneymqProduct) -> Result<String> {
    use stripe::CreateProduct;

    let client = Client::new(api_key);

    // Name is required for CreateProduct
    let name = local_product.name.as_deref().unwrap_or("Unnamed Product");
    let mut params = CreateProduct::new(name);

    // Set optional fields
    if let Some(description) = &local_product.description {
        params.description = Some(description.as_str());
    }

    params.active = Some(local_product.active);

    // Set metadata (convert IndexMap to HashMap for Stripe API)
    if !local_product.metadata.is_empty() {
        params.metadata = Some(
            local_product
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
    }

    let created_product = StripeProduct::create(&client, params).await?;

    Ok(created_product.id.to_string())
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
/// ```no_run
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let api_key = std::env::var("STRIPE_SECRET_KEY")?;
///     let meters = download_meters(&api_key, "stripe", true).await?;
///     println!("Downloaded {} meters", meters.total_count);
///     Ok(())
/// }
/// ```
pub async fn download_meters(
    api_key: &str,
    provider_name: &str,
    _is_production: bool,
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
            if let Ok(meter) = convert_stripe_meter(meter_json, _is_production) {
                meters.push(meter);
            }
        }
    }

    Ok(MeterCollection::new(meters, provider_name.to_string()))
}

/// Convert Stripe meter JSON to MoneyMQ Meter
fn convert_stripe_meter(
    meter_json: &serde_json::Value,
    is_production: bool,
) -> Result<MoneymqMeter> {
    use indexmap::IndexMap;

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

    let created = meter_json
        .get("created")
        .and_then(|v| v.as_i64())
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(|| Utc::now());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_catalog_structure() {
        // This test verifies the Catalog structure
        let catalog = Catalog::new(vec![], "stripe".to_string());
        assert_eq!(catalog.total_count, 0);
        assert_eq!(catalog.products.len(), 0);
        assert_eq!(catalog.provider, "stripe");
    }

    // To test with a real API key, set STRIPE_SECRET_KEY environment variable
    // and uncomment this test
    // #[tokio::test]
    // #[ignore]
    // async fn test_download_catalog_integration() {
    //     let api_key = std::env::var("STRIPE_SECRET_KEY")
    //         .expect("STRIPE_SECRET_KEY not set");
    //     let catalog = download_catalog(&api_key, "stripe").await.unwrap();
    //     println!("Downloaded {} products", catalog.total_count);
    // }
}
