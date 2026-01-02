use axum::{
    Extension, Form, Json, body::Bytes, extract::Path, http::StatusCode, response::IntoResponse,
};

use crate::api::catalog::{
    CatalogState,
    stripe::{
        types::{CreateCustomerRequest, StripeCustomer},
        utils::generate_stripe_id,
    },
};

/// POST /v1/customers - Create a new customer
pub async fn create_customer(
    Extension(_state): Extension<CatalogState>,
    Form(request): Form<CreateCustomerRequest>,
) -> impl IntoResponse {
    // Generate a mock customer ID
    let customer_id = generate_stripe_id("cus");
    let created = chrono::Utc::now().timestamp();

    let metadata = request.metadata.and_then(|m| serde_json::to_value(m).ok());

    let customer = StripeCustomer {
        id: customer_id,
        object: "customer".to_string(),
        email: request.email,
        name: request.name,
        metadata,
        created,
        description: None,
        phone: None,
    };

    (StatusCode::OK, Json(customer)).into_response()
}

/// POST /v1/customers/:id - Update a customer
pub async fn update_customer(
    Extension(_state): Extension<CatalogState>,
    Path(customer_id): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    let created = chrono::Utc::now().timestamp();

    // Parse form-encoded body manually to handle nested structures like invoice_settings[default_payment_method]
    let body_str = String::from_utf8_lossy(&body);

    // Extract default_payment_method from invoice_settings[default_payment_method]
    let default_pm = body_str
        .split('&')
        .find(|part| part.contains("invoice_settings") && part.contains("default_payment_method"))
        .and_then(|part| part.split('=').nth(1))
        .map(|pm| urlencoding::decode(pm).unwrap_or_default().to_string());

    // Return a mock updated customer with the payment method set
    let mut metadata = std::collections::HashMap::new();
    if let Some(pm) = default_pm {
        metadata.insert("default_payment_method".to_string(), pm);
    }

    let customer = StripeCustomer {
        id: customer_id,
        object: "customer".to_string(),
        email: "john.doe@example.com".to_string(),
        name: Some("John Doe".to_string()),
        metadata: Some(serde_json::to_value(metadata).unwrap()),
        created,
        description: None,
        phone: None,
    };

    (StatusCode::OK, Json(customer)).into_response()
}
