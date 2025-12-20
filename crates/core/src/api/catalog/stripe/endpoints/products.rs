use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use serde::Serialize;

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

/// Response for product access endpoint
#[derive(Debug, Serialize)]
pub struct ProductAccessResponse {
    pub object: String,
    pub product_id: String,
    pub access_granted: bool,
    pub message: String,
}

/// GET /v1/products/{id}/access - Access a product (x402 gated)
///
/// This endpoint is gated by x402 payment. The client must include an X-Payment header
/// with a valid payment. If no payment is provided, returns 402 with payment requirements.
///
/// After successful payment, returns access confirmation.
pub async fn get_product_access(
    State(state): State<ProviderState>,
    Path(product_id): Path<String>,
) -> impl IntoResponse {
    // Find the product
    let product = state.products.iter().find(|p| p.id == product_id);

    match product {
        Some(product) => {
            let product_name = product.name.clone().unwrap_or_else(|| product_id.clone());
            Json(ProductAccessResponse {
                object: "product_access".to_string(),
                product_id: product_id.clone(),
                access_granted: true,
                message: format!("Access granted to {}", product_name),
            })
        }
        None => Json(ProductAccessResponse {
            object: "product_access".to_string(),
            product_id: product_id.clone(),
            access_granted: false,
            message: "Product not found".to_string(),
        }),
    }
}
