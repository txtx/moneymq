pub mod endpoints;
pub mod networks;

use axum::{
    Router,
    routing::{get, post},
};
use moneymq_types::x402::Network;
use std::sync::Arc;
use tracing::info;

/// Configuration for the facilitator
#[derive(Clone)]
pub struct FacilitatorConfig {
    /// RPC URL
    pub rpc_url: String,
    /// Default network to support
    pub network: Network,
}

/// Shared state for the facilitator
#[derive(Clone)]
pub struct FacilitatorState {
    pub config: Arc<FacilitatorConfig>,
}

impl FacilitatorState {
    pub fn new(config: FacilitatorConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

/// Create the facilitator router
pub fn create_router(state: FacilitatorState) -> Router {
    Router::new()
        .route("/health", get(endpoints::health::handler))
        .route("/verify", post(endpoints::verify::handler))
        .route("/settle", post(endpoints::settle::handler))
        .route("/supported", get(endpoints::supported::handler))
        .with_state(state)
}

/// Start the facilitator server
pub async fn start_facilitator(
    config: FacilitatorConfig,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = FacilitatorState::new(config);
    let app = create_router(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("🚀 Facilitator server starting on {}", addr);
    info!("📍 Endpoints:");
    info!("  GET  http://localhost:{}/health", port);
    info!("  POST http://localhost:{}/verify", port);
    info!("  POST http://localhost:{}/settle", port);
    info!("  GET  http://localhost:{}/supported", port);

    axum::serve(listener, app).await?;

    Ok(())
}
