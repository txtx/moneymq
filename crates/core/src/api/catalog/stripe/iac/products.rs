use anyhow::Result;
use chrono::DateTime;
use indexmap::IndexMap;
use moneymq_types::{Catalog, Product as MoneymqProduct};
use stripe::{Client, ListProducts, Product as StripeProduct, ProductId, UpdateProduct};

use super::{
    super::utils::timestamp_to_datetime,
    common::{generate_base58_id, metadata_to_sorted_indexmap},
};

/// Convert Stripe Product to MoneyMQ Product
pub fn convert_product(
    stripe_product: StripeProduct,
    is_production: bool,
    prices: Vec<moneymq_types::Price>,
) -> MoneymqProduct {
    let created_at = timestamp_to_datetime(stripe_product.created);

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
        features: IndexMap::new(),
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
/// ```ignore
/// let api_key = std::env::var("STRIPE_SECRET_KEY")?;
/// let catalog = download_catalog(&api_key, "stripe", true).await?;
/// println!("Downloaded {} products", catalog.total_count);
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
        let product_prices =
            super::prices::fetch_product_prices(&client, &product_id, is_production).await?;

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
