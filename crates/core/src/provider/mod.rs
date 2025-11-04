pub mod stripe;

use std::sync::Arc;

use crate::facilitator::endpoints::middleware::x402_post;
use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use moneymq_types::{Meter, Product};

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
        // Health check
        .route("/health", get(health_check))
        // Product endpoints
        .route("/v1/products", get(stripe::list_products))
        .route("/v1/prices", get(stripe::list_prices))
        // Billing endpoints
        .route("/v1/billing/meters", get(stripe::list_meters))
        .route(
            "/v1/billing/meter_events",
            x402_post(stripe::create_meter_event, state.clone()),
        )
        // Customer endpoints
        .route("/v1/customers", post(stripe::create_customer))
        .route("/v1/customers/{id}", post(stripe::update_customer))
        // Payment method endpoints
        .route("/v1/payment_methods", post(stripe::create_payment_method))
        .route(
            "/v1/payment_methods/{id}/attach",
            post(stripe::attach_payment_method),
        )
        // Subscription endpoints
        .route("/v1/subscriptions", post(stripe::create_subscription))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ provider server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
