use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use tokio::task::JoinHandle;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::{error, info};
use x402_rs::{
    facilitator::Facilitator,
    facilitator_local::FacilitatorLocal,
    provider_cache::ProviderCache,
    types::{SettleRequest, VerifyRequest},
};

pub struct FacilitatorConfig {
    /// Facilitator service host (e.g., "localhost")
    pub host: String,
    /// Facilitator service port (e.g., 8080)
    pub port: u16,
    /// Facilitator provider cache
    pub provider_cache: Option<ProviderCache>,
}

pub async fn start_local_facilitator(
    config: &FacilitatorConfig,
) -> Result<
    JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    // init_tracing();
    let addr = format!("{}:{}", config.host, config.port);
    info!("Starting local Facilitator on {}", addr);

    let facilitator = FacilitatorLocal::new(
        config
            .provider_cache
            .clone()
            .expect("Provider cache expected"),
    );

    let state = AppState {
        facilitator: Arc::new(facilitator),
    };
    let app = Router::new()
        .route(
            "/verify",
            post(verify_handler::<FacilitatorLocal<ProviderCache>>),
        )
        .route(
            "/settle",
            post(settle_handler::<FacilitatorLocal<ProviderCache>>),
        )
        .route(
            "/supported",
            get(supported_handler::<FacilitatorLocal<ProviderCache>>),
        )
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true))
                .on_response(DefaultOnResponse::new().include_headers(true)),
        )
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let handle =
        tokio::spawn(async move { axum::serve(listener, app).await.map_err(|e| e.into()) });
    Ok(handle)
}

// fn init_tracing() {
//     let filter = EnvFilter::try_from_default_env()
//         .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,axum::rejection=trace"));
//     fmt()
//         .with_env_filter(filter)
//         .with_target(false)
//         .with_level(true)
//         .init();
// }

#[derive(Clone)]
struct AppState<F: Facilitator> {
    facilitator: Arc<F>,
}

async fn verify_handler<F: Facilitator>(
    State(state): State<AppState<F>>,
    Json(req): Json<VerifyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    info!(target: "x402", "Incoming verify request: {:?}", req);
    match state.facilitator.verify(&req).await {
        Ok(resp) => {
            info!(target: "x402", "Verify success: {:?}", resp);
            (StatusCode::OK, Json(serde_json::json!(resp)))
        }
        Err(e) => {
            error!(target: "x402", "Verify failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        }
    }
}

async fn settle_handler<F: Facilitator>(
    State(state): State<AppState<F>>,
    Json(req): Json<SettleRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    info!(target: "x402", "Incoming settle request: {:?}", req);
    match state.facilitator.settle(&req).await {
        Ok(resp) => {
            info!(target: "x402", "Settle success: {:?}", resp);
            (StatusCode::OK, Json(serde_json::json!(resp)))
        }
        Err(e) => {
            error!(target: "x402", "Settle failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        }
    }
}

async fn supported_handler<F: Facilitator>(
    State(state): State<AppState<F>>,
) -> (StatusCode, Json<serde_json::Value>) {
    info!(target: "x402", "Supported kinds request");
    match state.facilitator.supported().await {
        Ok(resp) => {
            info!(target: "x402", "Supported kinds: {:?}", resp.kinds);
            (StatusCode::OK, Json(serde_json::json!(resp)))
        }
        Err(e) => {
            error!(target: "x402", "Supported kinds failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        }
    }
}
