use anyhow::Result;
use chrono::{DateTime, Utc};
use moneymq_types::{Catalog, Price as MoneymqPrice, Product as MoneymqProduct};
use sha2::{Digest, Sha256};
use stripe::{Client, ListPrices, ListProducts, Price as StripePrice, PriceId, Product as StripeProduct, ProductId};

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

    MoneymqPrice {
        id: base58_id,
        external_id: if is_production { Some(stripe_id.clone()) } else { None },
        sandbox_external_id: if !is_production { Some(stripe_id) } else { None },
        active: stripe_price.active.unwrap_or(true),
        currency: stripe_price.currency.unwrap_or_default().to_string(),
        unit_amount: stripe_price.unit_amount,
        pricing_type,
        recurring_interval,
        recurring_interval_count,
        nickname: stripe_price.nickname,
        metadata: stripe_price.metadata.unwrap_or_default(),
        created_at,
    }
}

/// Convert Stripe Product to MoneyMQ Product
fn convert_product(stripe_product: StripeProduct, _is_production: bool, prices: Vec<MoneymqPrice>) -> MoneymqProduct {
    let created_at = stripe_product
        .created
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(|| Utc::now());

    let updated_at = stripe_product
        .updated
        .and_then(|ts| DateTime::from_timestamp(ts, 0));

    let stripe_id = stripe_product.id.to_string();
    let base58_id = generate_base58_id(&stripe_id);

    MoneymqProduct {
        id: base58_id,
        external_id: Some(stripe_id),
        sandbox_external_id: None,
        name: stripe_product.name,
        description: stripe_product.description,
        active: stripe_product.active.unwrap_or(true),
        metadata: stripe_product.metadata.unwrap_or_default(),
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
pub async fn download_catalog(api_key: &str, provider_name: &str, is_production: bool) -> Result<Catalog> {
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
                    .map(|p| convert_price(p, is_production))
            );

            if !has_more {
                break;
            }
        }

        products.push(convert_product(stripe_product, is_production, product_prices));
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
    let product_id: ProductId = external_id.parse()
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

    // Update metadata
    if !local_product.metadata.is_empty() {
        params.metadata = Some(local_product.metadata.clone());
    }

    StripeProduct::update(&client, &product_id, params).await?;

    Ok(())
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
