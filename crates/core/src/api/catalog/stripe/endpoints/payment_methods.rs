use axum::{Extension, Form, Json, extract::Path, http::StatusCode, response::IntoResponse};

use crate::api::catalog::{
    CatalogState,
    stripe::{
        types::{
            AttachPaymentMethodRequest, CreatePaymentMethodRequest, StripeCard, StripePaymentMethod,
        },
        utils::generate_stripe_id,
    },
};

/// POST /v1/payment_methods - Create a payment method
pub async fn create_payment_method(
    Extension(_state): Extension<CatalogState>,
    Form(request): Form<CreatePaymentMethodRequest>,
) -> impl IntoResponse {
    // Generate a mock payment method ID
    let pm_id = generate_stripe_id("pm");
    let created = chrono::Utc::now().timestamp();

    // Mock card details
    let card = if request.payment_type == "card" {
        Some(StripeCard {
            brand: "visa".to_string(),
            last4: "4242".to_string(),
            exp_month: 12,
            exp_year: 2028,
        })
    } else {
        None
    };

    let payment_method = StripePaymentMethod {
        id: pm_id,
        object: "payment_method".to_string(),
        payment_type: request.payment_type,
        created,
        card,
        customer: None,
    };

    (StatusCode::OK, Json(payment_method)).into_response()
}

/// POST /v1/payment_methods/:id/attach - Attach payment method to customer
pub async fn attach_payment_method(
    Extension(_state): Extension<CatalogState>,
    Path(payment_method_id): Path<String>,
    Form(request): Form<AttachPaymentMethodRequest>,
) -> impl IntoResponse {
    let created = chrono::Utc::now().timestamp();

    // Return a mock attached payment method
    let payment_method = StripePaymentMethod {
        id: payment_method_id,
        object: "payment_method".to_string(),
        payment_type: "card".to_string(),
        created,
        card: Some(StripeCard {
            brand: "visa".to_string(),
            last4: "4242".to_string(),
            exp_month: 12,
            exp_year: 2028,
        }),
        customer: Some(request.customer),
    };

    (StatusCode::OK, Json(payment_method)).into_response()
}
