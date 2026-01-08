use std::collections::HashMap;

use axum::{Extension, Json, extract::Path, response::IntoResponse};
use rand::Rng;
use serde::Serialize;

use crate::api::catalog::{
    CatalogState,
    stripe::types::{ListParams, ListResponse, StripeProduct},
};

/// Get the minimum price for a product (used for sorting)
fn get_min_price(product: &moneymq_types::Product) -> i64 {
    product
        .prices
        .iter()
        .filter_map(|p| p.unit_amount)
        .min()
        .unwrap_or(i64::MAX) // Products without prices go last
}

/// Select an experiment variant based on exposure percentages
/// Returns the selected experiment or None if no experiment should be selected
fn select_experiment<'a>(
    experiments: &[&'a moneymq_types::Product],
) -> Option<&'a moneymq_types::Product> {
    if experiments.is_empty() {
        return None;
    }

    let mut rng = rand::rng();
    let roll: f64 = rng.random(); // 0.0 to 1.0

    // Sort experiments by exposure (deterministic selection)
    let mut sorted_experiments: Vec<_> = experiments.to_vec();
    sorted_experiments.sort_by(|a, b| {
        let exp_a = a.experiment.as_ref().map(|e| e.exposure).unwrap_or(0.0);
        let exp_b = b.experiment.as_ref().map(|e| e.exposure).unwrap_or(0.0);
        exp_a
            .partial_cmp(&exp_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Cumulative probability selection
    let mut cumulative = 0.0;
    for exp in &sorted_experiments {
        let exposure = exp.experiment.as_ref().map(|e| e.exposure).unwrap_or(0.0);
        cumulative += exposure;
        if roll < cumulative {
            return Some(exp);
        }
    }

    // If total exposure < 1.0, there's a chance no experiment is selected
    // In that case, return the first experiment as fallback
    sorted_experiments.first().copied()
}

/// GET /v1/products - List products (sorted by price ASC)
///
/// Experiment variants are materialized: instead of returning all experiment variants,
/// the server randomly selects one based on exposure percentages and returns it with:
/// - id: the parent product ID (e.g., "surfnet-lite")
/// - experiment_id: the selected experiment variant ID (e.g., "surfnet-lite#a")
pub async fn list_products(
    Extension(state): Extension<CatalogState>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(100) as usize;

    // Separate products into:
    // 1. Regular products (no experiment config)
    // 2. Experiment variants (have experiment config and parent_id)
    let mut regular_products: Vec<&moneymq_types::Product> = Vec::new();
    let mut experiments_by_parent: HashMap<String, Vec<&moneymq_types::Product>> = HashMap::new();

    for product in state.products.iter() {
        if product.experiment.is_some() {
            // This is an experiment variant - group by parent_id
            if let Some(ref parent_id) = product.parent_id {
                experiments_by_parent
                    .entry(parent_id.clone())
                    .or_default()
                    .push(product);
            }
        } else {
            // Regular product
            regular_products.push(product);
        }
    }

    // Build final product list with materialized experiments
    let mut final_products: Vec<StripeProduct> = Vec::new();

    for product in &regular_products {
        // Check if this product has experiment variants
        if let Some(experiments) = experiments_by_parent.get(&product.id) {
            // Select an experiment variant
            if let Some(selected_experiment) = select_experiment(experiments) {
                // Return materialized product with experiment_id
                final_products.push(StripeProduct::from_product_with_experiment(
                    product,
                    selected_experiment,
                    state.use_sandbox,
                ));
            } else {
                // No experiment selected, return regular product
                final_products.push(StripeProduct::from_product(product, state.use_sandbox));
            }
        } else {
            // No experiments for this product
            final_products.push(StripeProduct::from_product(product, state.use_sandbox));
        }
    }

    // Sort by minimum price (ascending)
    final_products.sort_by_key(|p| {
        // Find original product to get price
        state
            .products
            .iter()
            .find(|prod| {
                let external_id = if state.use_sandbox {
                    prod.sandboxes.get("default")
                } else {
                    prod.deployed_id.as_ref()
                };
                external_id.map(|id| id == &p.id).unwrap_or(false) || prod.id == p.id
            })
            .map(|prod| get_min_price(prod))
            .unwrap_or(i64::MAX)
    });

    // Find starting position
    let start_idx = if let Some(starting_after) = params.starting_after {
        final_products
            .iter()
            .position(|p| p.id == starting_after)
            .map(|idx| idx + 1)
            .unwrap_or(0)
    } else {
        0
    };

    let end_idx = (start_idx + limit).min(final_products.len());
    let products_slice = final_products[start_idx..end_idx].to_vec();

    let has_more = end_idx < final_products.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: products_slice,
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
    Extension(state): Extension<CatalogState>,
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
