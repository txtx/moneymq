use axum::{response::IntoResponse, Json};

pub async fn handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "moneymq-facilitator"
    }))
}
