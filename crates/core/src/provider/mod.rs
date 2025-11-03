use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use moneymq_types::{Meter, Product};
use serde::{Deserialize, Serialize};

/// Stripe-compatible list response
#[derive(Debug, Serialize)]
struct ListResponse<T> {
    object: String,
    data: Vec<T>,
    has_more: bool,
    url: String,
}

/// Stripe-compatible product response
#[derive(Debug, Serialize)]
struct StripeProduct {
    id: String,
    object: String,
    active: bool,
    created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    statement_descriptor: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<i64>,
}

/// Stripe-compatible price response
#[derive(Debug, Serialize)]
struct StripePrice {
    id: String,
    object: String,
    active: bool,
    currency: String,
    created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nickname: Option<String>,
    product: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    recurring: Option<StripeRecurring>,
    #[serde(rename = "type")]
    pricing_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_amount: Option<i64>,
}

#[derive(Debug, Serialize)]
struct StripeRecurring {
    interval: String,
    interval_count: i64,
}

/// Query parameters for list endpoints
#[derive(Debug, Deserialize)]
struct ListParams {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    starting_after: Option<String>,
    #[serde(default)]
    product: Option<String>, // For prices filtered by product
}

/// Stripe-compatible billing meter response
#[derive(Debug, Serialize)]
struct StripeBillingMeter {
    id: String,
    object: String,
    created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    customer_mapping: Option<StripeMeterCustomerMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_aggregation: Option<StripeMeterAggregation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value_settings: Option<StripeMeterValueSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<i64>,
}

#[derive(Debug, Serialize)]
struct StripeMeterCustomerMapping {
    #[serde(rename = "type")]
    mapping_type: String,
    event_payload_key: String,
}

#[derive(Debug, Serialize)]
struct StripeMeterAggregation {
    formula: String,
}

#[derive(Debug, Serialize)]
struct StripeMeterValueSettings {
    event_payload_key: String,
}

/// Application state
#[derive(Clone)]
pub struct ProviderState {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
    pub use_sandbox: bool,
}

impl ProviderState {
    pub fn new(products: Vec<Product>, meters: Vec<Meter>, use_sandbox: bool) -> Self {
        Self {
            products: Arc::new(products),
            meters: Arc::new(meters),
            use_sandbox,
        }
    }
}

/// Convert MoneyMQ Product to Stripe Product
fn to_stripe_product(product: &Product, use_sandbox: bool) -> StripeProduct {
    let external_id = if use_sandbox {
        product.sandboxes.get("default")
    } else {
        product.deployed_id.as_ref()
    };

    StripeProduct {
        id: external_id.cloned().unwrap_or_else(|| product.id.clone()),
        object: "product".to_string(),
        active: product.active,
        created: product.created_at.timestamp(),
        description: product.description.clone(),
        images: if product.images.is_empty() {
            None
        } else {
            Some(product.images.clone())
        },
        metadata: if product.metadata.is_empty() {
            None
        } else {
            serde_json::to_value(&product.metadata).ok()
        },
        name: product.name.clone(),
        statement_descriptor: product.statement_descriptor.clone(),
        product_type: product.product_type.clone(),
        unit_label: product.unit_label.clone(),
        updated: product.updated_at.map(|dt| dt.timestamp()),
    }
}

/// Convert MoneyMQ Price to Stripe Price
fn to_stripe_price(
    price: &moneymq_types::Price,
    product: &Product,
    use_sandbox: bool,
) -> StripePrice {
    let external_id = if use_sandbox {
        price.sandboxes.get("default")
    } else {
        price.deployed_id.as_ref()
    };

    let product_external_id = if use_sandbox {
        product.sandboxes.get("default")
    } else {
        product.deployed_id.as_ref()
    };

    let recurring = if price.pricing_type == "recurring" {
        Some(StripeRecurring {
            interval: price.recurring_interval.clone().unwrap_or_default(),
            interval_count: price.recurring_interval_count.unwrap_or(1),
        })
    } else {
        None
    };

    StripePrice {
        id: external_id.cloned().unwrap_or_else(|| price.id.clone()),
        object: "price".to_string(),
        active: price.active,
        currency: price.currency.clone(),
        created: price.created_at.timestamp(),
        metadata: if price.metadata.is_empty() {
            None
        } else {
            serde_json::to_value(&price.metadata).ok()
        },
        nickname: price.nickname.clone(),
        product: product_external_id
            .cloned()
            .unwrap_or_else(|| product.id.clone()),
        recurring,
        pricing_type: price.pricing_type.clone(),
        unit_amount: price.unit_amount,
    }
}

/// List products endpoint (GET /v1/products)
async fn list_products(
    State(state): State<ProviderState>,
    Query(params): Query<ListParams>,
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
        .map(|p| to_stripe_product(p, state.use_sandbox))
        .collect();

    let has_more = end_idx < state.products.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: stripe_products,
        has_more,
        url: "/v1/products".to_string(),
    })
}

/// List prices endpoint (GET /v1/prices)
async fn list_prices(
    State(state): State<ProviderState>,
    Query(params): Query<ListParams>,
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
        .map(|(price, product)| to_stripe_price(price, product, state.use_sandbox))
        .collect();

    let has_more = end_idx < all_prices.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: stripe_prices,
        has_more,
        url: "/v1/prices".to_string(),
    })
}

/// Convert MoneyMQ Meter to Stripe Billing Meter
fn to_stripe_meter(meter: &Meter, use_sandbox: bool) -> StripeBillingMeter {
    let external_id = if use_sandbox {
        meter.sandboxes.get("default")
    } else {
        meter.deployed_id.as_ref()
    };

    StripeBillingMeter {
        id: external_id.cloned().unwrap_or_else(|| meter.id.clone()),
        object: "billing.meter".to_string(),
        created: meter.created_at.timestamp(),
        display_name: meter.display_name.clone(),
        event_name: meter.event_name.clone(),
        status: meter.status.clone(),
        customer_mapping: meter
            .customer_mapping
            .as_ref()
            .map(|cm| StripeMeterCustomerMapping {
                mapping_type: cm.mapping_type.clone(),
                event_payload_key: cm.event_payload_key.clone(),
            }),
        default_aggregation: meter
            .default_aggregation
            .as_ref()
            .map(|da| StripeMeterAggregation {
                formula: da.formula.clone(),
            }),
        value_settings: meter
            .value_settings
            .as_ref()
            .map(|vs| StripeMeterValueSettings {
                event_payload_key: vs.event_payload_key.clone(),
            }),
        updated: meter.updated_at.map(|dt| dt.timestamp()),
    }
}

/// List billing meters endpoint (GET /v1/billing/meters)
async fn list_meters(
    State(state): State<ProviderState>,
    Query(params): Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(100) as usize;

    // Find starting position
    let start_idx = if let Some(starting_after) = params.starting_after {
        state
            .meters
            .iter()
            .position(|m| {
                let external_id = if state.use_sandbox {
                    m.sandboxes.get("default")
                } else {
                    m.deployed_id.as_ref()
                };
                external_id.map(|id| id == &starting_after).unwrap_or(false)
            })
            .map(|idx| idx + 1)
            .unwrap_or(0)
    } else {
        0
    };

    let end_idx = (start_idx + limit).min(state.meters.len());
    let meters_slice = &state.meters[start_idx..end_idx];

    let stripe_meters: Vec<StripeBillingMeter> = meters_slice
        .iter()
        .map(|m| to_stripe_meter(m, state.use_sandbox))
        .collect();

    let has_more = end_idx < state.meters.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: stripe_meters,
        has_more,
        url: "/v1/billing/meters".to_string(),
    })
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Start the provider server
pub async fn start_provider(
    products: Vec<Product>,
    meters: Vec<Meter>,
    port: u16,
    use_sandbox: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ProviderState::new(products, meters, use_sandbox);

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/products", get(list_products))
        .route("/v1/prices", get(list_prices))
        .route("/v1/billing/meters", get(list_meters))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ provider server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
