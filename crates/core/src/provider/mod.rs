pub mod config;
pub mod stripe;
pub mod x402;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use moneymq_types::{Meter, Product, x402::transactions::FacilitatedTransaction};
use reqwest::{
    Method,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use tower_http::cors::{Any, CorsLayer};
use url::Url;

use crate::{billing::BillingManager, facilitator::endpoints::middleware::x402_post};

/// Application state
#[derive(Clone)]
pub struct ProviderState {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
    pub use_sandbox: bool,
    pub facilitator_url: Url,
    pub billing_manager: BillingManager,
    pub catalog_path: PathBuf,
    pub catalog_name: Option<String>,
    pub catalog_description: Option<String>,
    pub facilitator_pubkey: Option<String>,
    pub validator_rpc_url: Option<Url>,
    pub transactions: Arc<Mutex<Vec<FacilitatedTransaction>>>,
}

/// Application state
#[derive(Clone)]
pub struct Facilitator {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
    pub use_sandbox: bool,
}

impl ProviderState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        products: Vec<Product>,
        meters: Vec<Meter>,
        use_sandbox: bool,
        facilitator_url: Url,
        billing_manager: BillingManager,
        catalog_path: PathBuf,
        catalog_name: Option<String>,
        catalog_description: Option<String>,
        facilitator_pubkey: Option<String>,
        validator_rpc_url: Option<Url>,
    ) -> Self {
        Self {
            products: Arc::new(products),
            meters: Arc::new(meters),
            use_sandbox,
            facilitator_url,
            billing_manager,
            catalog_path,
            catalog_name,
            catalog_description,
            facilitator_pubkey,
            validator_rpc_url,
            transactions: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Start the provider server
#[allow(clippy::too_many_arguments)]
pub async fn start_provider(
    products: Vec<Product>,
    meters: Vec<Meter>,
    facilitator_url: Url,
    port: u16,
    use_sandbox: bool,
    billing_manager: BillingManager,
    catalog_path: PathBuf,
    catalog_name: Option<String>,
    catalog_description: Option<String>,
    facilitator_pubkey: Option<String>,
    validator_rpc_url: Option<Url>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ProviderState::new(
        products,
        meters,
        use_sandbox,
        facilitator_url,
        billing_manager,
        catalog_path,
        catalog_name,
        catalog_description,
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
        // x402 dev endpoints
        .route("/x402/accounts", get(x402::list_accounts))
        // Config endpoint
        .route("/x402/config", get(config::get_config))
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
        .route(
            "/v1/subscriptions",
            x402_post(stripe::create_subscription, state.clone()),
        )
        .layer(cors_layer)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ provider server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
