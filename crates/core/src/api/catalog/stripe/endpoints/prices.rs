use axum::{Extension, Json, response::IntoResponse};
use moneymq_types::Product;

use crate::api::catalog::{
    CatalogState,
    stripe::types::{ListParams, ListResponse, StripePrice},
};

/// GET /v1/prices - List prices
pub async fn list_prices(
    Extension(state): Extension<CatalogState>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(100) as usize;

    // Collect all prices from all products
    let mut all_prices: Vec<(moneymq_types::Price, Product)> = Vec::new();

    for product in state.products.iter() {
        // If product filter is specified, only include prices for that product
        if let Some(ref product_filter) = params.product {
            let product_external_id = if state.use_sandbox {
                product.sandboxes.get("default")
            } else {
                product.deployed_id.as_ref()
            };

            if product_external_id
                .map(|id| id != product_filter)
                .unwrap_or(true)
            {
                continue;
            }
        }

        for price in &product.prices {
            all_prices.push((price.clone(), product.clone()));
        }
    }

    // Sort by unit_amount ascending (prices without amount go last)
    all_prices.sort_by(|(a, _), (b, _)| match (a.unit_amount, b.unit_amount) {
        (Some(a_amt), Some(b_amt)) => a_amt.cmp(&b_amt),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    // Find starting position
    let start_idx = if let Some(starting_after) = params.starting_after {
        all_prices
            .iter()
            .position(|(price, _)| {
                let external_id = if state.use_sandbox {
                    price.sandboxes.get("default")
                } else {
                    price.deployed_id.as_ref()
                };
                external_id.map(|id| id == &starting_after).unwrap_or(false)
            })
            .map(|idx| idx + 1)
            .unwrap_or(0)
    } else {
        0
    };

    let end_idx = (start_idx + limit).min(all_prices.len());
    let prices_slice = &all_prices[start_idx..end_idx];

    let stripe_prices: Vec<StripePrice> = prices_slice
        .iter()
        .map(|(price, product)| {
            StripePrice::from_price_and_product(price, product, state.use_sandbox)
        })
        .collect();

    let has_more = end_idx < all_prices.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: stripe_prices,
        has_more,
        url: "/v1/prices".to_string(),
    })
}
