use axum::{
    body::Body,
    extract::State,
    handler::Handler,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{MethodRouter, post},
};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::provider::ProviderState;

// Track if we should return 402 (for testing payment handling)
static RETURN_402_ONCE: AtomicBool = AtomicBool::new(true);

/// Middleware to handle payment requirements for meter events
pub async fn payment_middleware(
    State(_state): State<ProviderState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // For testing: return 402 Payment Required on first call
    if RETURN_402_ONCE.swap(false, Ordering::SeqCst) {
        println!("\x1b[31m$ Required\x1b[0m - Access to /v1/billing/meter_events denied");
        let error_response = serde_json::json!({
            "error": {
                "code": "payment_required",
                "message": "Payment required to record meter events. Please complete payment to continue.",
                "type": "invalid_request_error",
                "payment_url": "https://example.com/pay"
            }
        });
        return (StatusCode::PAYMENT_REQUIRED, axum::Json(error_response)).into_response();
    }

    // Payment completed - processing request
    println!("\x1b[32m$ Received\x1b[0m - Access to /v1/billing/meter_events granted");

    // Continue to the handler
    next.run(req).await
}

/// Helper function to create a POST route with payment middleware
///
/// # Example
/// ```
/// use crate::facilitator::endpoints::middleware::x402_post;
///
/// let route = x402_post(my_handler, state.clone());
/// ```
pub fn x402_post<H, T>(handler: H, state: ProviderState) -> MethodRouter<ProviderState>
where
    H: Handler<T, ProviderState>,
    T: 'static,
{
    post(handler).layer(middleware::from_fn_with_state(state, payment_middleware))
}
