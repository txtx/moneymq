pub mod config;
pub mod stripe;
pub mod x402;

use std::sync::Arc;
use std::path::PathBuf;

use crate::{billing::BillingManager, facilitator::endpoints::middleware::x402_post};
use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use moneymq_types::{Meter, Product};
use reqwest::{
    Method,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use tower_http::cors::{Any, CorsLayer};
use url::Url;

/// Application state
#[derive(Clone)]
pub struct ProviderState {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
    pub use_sandbox: bool,
    pub facilitator_url: Url,
    pub billing_manager: BillingManager,
    pub manifest_path: PathBuf,
    pub provider_name: Option<String>,
    pub provider_description: Option<String>,
    pub facilitator_pubkey: Option<String>,
    pub validator_rpc_url: Option<Url>,
}

/// Application state
#[derive(Clone)]
pub struct Facilitator {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
    pub use_sandbox: bool,
}

impl ProviderState {
    pub fn new(
        products: Vec<Product>,
        meters: Vec<Meter>,
        use_sandbox: bool,
        facilitator_url: Url,
        billing_manager: BillingManager,
        manifest_path: PathBuf,
        provider_name: Option<String>,
        provider_description: Option<String>,
        facilitator_pubkey: Option<String>,
        validator_rpc_url: Option<Url>,
    ) -> Self {
        Self {
            products: Arc::new(products),
            meters: Arc::new(meters),
            use_sandbox,
            facilitator_url,
            billing_manager,
            manifest_path,
            provider_name,
            provider_description,
            facilitator_pubkey,
            validator_rpc_url,
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
    facilitator_url: Url,
    port: u16,
    use_sandbox: bool,
    billing_manager: BillingManager,
    manifest_path: PathBuf,
    provider_name: Option<String>,
    provider_description: Option<String>,
    facilitator_pubkey: Option<String>,
    validator_rpc_url: Option<Url>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ProviderState::new(
        products,
        meters,
        use_sandbox,
        facilitator_url,
        billing_manager,
        manifest_path,
        provider_name,
        provider_description,
        facilitator_pubkey,
        validator_rpc_url,
    );

    let cors_layer = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        .route("/v1/accounts", get(x402::list_accounts))
        // Config endpoint
        .route("/config", get(config::get_config))
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
        .layer(cors_layer)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ provider server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
