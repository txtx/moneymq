use axum::{Json, extract::State, response::IntoResponse};

use crate::api::catalog::{
    ProviderState,
    stripe::types::{ListParams, ListResponse, StripeProduct},
};

/// GET /v1/products - List products
pub async fn list_products(
    State(state): State<ProviderState>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(100) as usize;

    // Find starting position
    let start_idx = if let Some(starting_after) = params.starting_after {
        state
            .products
            .iter()
            .position(|p| {
                let external_id = if state.use_sandbox {
                    p.sandboxes.get("default")
                } else {
                    p.deployed_id.as_ref()
                };
                external_id.map(|id| id == &starting_after).unwrap_or(false)
            })
            .map(|idx| idx + 1)
            .unwrap_or(0)
    } else {
        0
    };

    let end_idx = (start_idx + limit).min(state.products.len());
    let products_slice = &state.products[start_idx..end_idx];

    let stripe_products: Vec<StripeProduct> = products_slice
        .iter()
        .map(|p| StripeProduct::from_product(p, state.use_sandbox))
        .collect();

    let has_more = end_idx < state.products.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: stripe_products,
        has_more,
        url: "/v1/products".to_string(),
    })
}
