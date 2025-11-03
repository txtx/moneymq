use anyhow::Result;
use indexmap::IndexMap;
use moneymq_types::Price as MoneymqPrice;
use stripe::{
    Client, CreatePrice, CreatePriceRecurring, CreatePriceRecurringInterval, Currency, ListPrices,
    Price as StripePrice, PriceId, ProductId,
};

use super::{
    super::utils::timestamp_to_datetime,
    common::{generate_base58_id, metadata_to_sorted_indexmap},
};

/// Convert Stripe Price to MoneyMQ Price
pub fn convert_price(stripe_price: StripePrice, is_production: bool) -> MoneymqPrice {
    let created_at = timestamp_to_datetime(stripe_price.created);

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

/// Fetch all prices for a given product
pub async fn fetch_product_prices(
    client: &Client,
    product_id: &ProductId,
    is_production: bool,
) -> Result<Vec<MoneymqPrice>> {
    let mut product_prices = Vec::new();
    let mut price_starting_after: Option<PriceId> = None;

    loop {
        let mut price_params = ListPrices::new();
        price_params.product = Some(stripe::IdOrCreate::Id(product_id));
        price_params.limit = Some(100);

        if let Some(ref last_id) = price_starting_after {
            price_params.starting_after = Some(last_id.clone());
        }

        let price_response = StripePrice::list(client, &price_params).await?;
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

    Ok(product_prices)
}

/// Create a price in Stripe
///
/// # Arguments
/// * `api_key` - Your Stripe secret API key
/// * `product_id` - The Stripe product ID to attach this price to
/// * `local_price` - The local price to create
///
/// # Returns
/// The Stripe price ID of the created price
pub async fn create_price(
    api_key: &str,
    product_id: &str,
    local_price: &MoneymqPrice,
) -> Result<String> {
    let client = Client::new(api_key);

    let mut params = CreatePrice::new(Currency::USD);

    // Set the product this price belongs to
    params.product = Some(stripe::IdOrCreate::Id(product_id));

    // Set currency
    if let Ok(currency) = local_price.currency.to_uppercase().parse::<Currency>() {
        params.currency = currency;
    }

    // Set unit amount
    if let Some(amount) = local_price.unit_amount {
        params.unit_amount = Some(amount);
    }

    // Set recurring interval if this is a recurring price
    if let Some(interval_str) = &local_price.recurring_interval {
        let interval = match interval_str.as_str() {
            "day" => CreatePriceRecurringInterval::Day,
            "week" => CreatePriceRecurringInterval::Week,
            "month" => CreatePriceRecurringInterval::Month,
            "year" => CreatePriceRecurringInterval::Year,
            _ => CreatePriceRecurringInterval::Month, // default to month
        };

        let mut recurring = CreatePriceRecurring {
            interval,
            aggregate_usage: None,
            interval_count: None,
            trial_period_days: None,
            usage_type: None,
        };

        if let Some(interval_count) = local_price.recurring_interval_count {
            recurring.interval_count = Some(interval_count as u64);
        }

        params.recurring = Some(recurring);
    }

    // Set active status
    params.active = Some(local_price.active);

    // Set nickname
    if let Some(nickname) = &local_price.nickname {
        params.nickname = Some(nickname.as_str());
    }

    // Set metadata (convert IndexMap to HashMap for Stripe API)
    if !local_price.metadata.is_empty() {
        params.metadata = Some(
            local_price
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
    }

    let created_price = StripePrice::create(&client, params).await?;

    Ok(created_price.id.to_string())
}
