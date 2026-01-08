use std::time::{SystemTime, UNIX_EPOCH};

use axum::{Extension, Json, extract::Path, http::StatusCode, response::IntoResponse};
use uuid::Uuid;

use crate::api::catalog::{
    CatalogState,
    stripe::types::{
        CheckoutLineItem, CheckoutLineItemList, CheckoutLineItemPrice, CheckoutSessionStatus,
        CreateCheckoutSessionRequest, PaymentIntentStatus, PaymentStatus, StripeCheckoutSession,
        StripePaymentIntent,
    },
};

/// POST /checkout/sessions - Create a new checkout session
pub async fn create_checkout_session(
    Extension(state): Extension<CatalogState>,
    Json(request): Json<CreateCheckoutSessionRequest>,
) -> impl IntoResponse {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Generate session ID
    let session_id = format!("cs_{}", &Uuid::new_v4().to_string().replace("-", "")[..24]);

    // Process line items and calculate totals by looking up products from catalog
    let mut line_items: Vec<CheckoutLineItem> = Vec::new();
    let mut amount_subtotal: i64 = 0;
    let mut currency = "usdc".to_string();

    for item in request.line_items.iter() {
        let line_item_id = format!("li_{}", &Uuid::new_v4().to_string().replace("-", "")[..24]);

        // Look up product from catalog by product_id
        // If experiment_id is provided, use that variant; otherwise use base product
        let lookup_id = item.experiment_id.as_ref().unwrap_or(&item.product_id);
        let product = state.products.iter().find(|p| p.id == *lookup_id);

        let (unit_amount, item_currency, product_description, product_id, experiment_id) =
            if let Some(product) = product {
                let price = product.prices.first();
                let unit_amount = price.and_then(|p| p.unit_amount).unwrap_or(0);
                let item_currency = price
                    .map(|p| p.currency.as_str().to_string())
                    .unwrap_or_else(|| "usdc".to_string());
                (
                    unit_amount,
                    item_currency,
                    product.description.clone(),
                    item.product_id.clone(),
                    item.experiment_id.clone(),
                )
            } else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": {
                            "message": format!("Product {} not found in catalog", lookup_id),
                            "type": "invalid_request_error"
                        }
                    })),
                )
                    .into_response();
            };

        // Set currency from first item
        if line_items.is_empty() {
            currency = item_currency.clone();
        }

        let quantity = item.quantity;
        let subtotal = unit_amount * quantity;
        amount_subtotal += subtotal;

        // Generate price ID from product
        let price_id = format!(
            "price_{}",
            &Uuid::new_v4().to_string().replace("-", "")[..24]
        );

        line_items.push(CheckoutLineItem {
            id: line_item_id,
            object: "item".to_string(),
            amount_subtotal: subtotal,
            amount_total: subtotal, // No tax/discount for now
            currency: item_currency.clone(),
            quantity,
            description: product_description,
            price: CheckoutLineItemPrice {
                id: price_id,
                object: "price".to_string(),
                currency: item_currency,
                unit_amount,
                product: Some(product_id.clone()),
                experiment_id: experiment_id.clone(),
                nickname: None,
                price_type: "one_time".to_string(),
            },
            amount_discount: None,
            amount_tax: None,
        });
    }

    let amount_total = amount_subtotal; // No tax/discount for now

    // Create the underlying payment intent
    let payment_intent_id = format!("pi_{}", &Uuid::new_v4().to_string().replace("-", "")[..24]);
    let client_secret = format!(
        "{}_secret_{}",
        payment_intent_id,
        &Uuid::new_v4().to_string().replace("-", "")[..24]
    );

    // Build description from line items
    let description = line_items
        .iter()
        .map(|li| li.price.product.clone().unwrap_or_else(|| li.id.clone()))
        .collect::<Vec<_>>()
        .join(", ");

    let payment_intent = StripePaymentIntent {
        id: payment_intent_id.clone(),
        object: "payment_intent".to_string(),
        amount: amount_total,
        currency: currency.clone(),
        status: PaymentIntentStatus::RequiresConfirmation,
        created: now,
        customer: request.customer.clone(),
        payment_method: None,
        description: Some(format!("Purchase - {}", description)),
        metadata: {
            let mut meta = request.metadata.clone();
            // Store line items as JSON for the middleware to extract
            meta.insert("checkout_session_id".to_string(), session_id.clone());
            meta.insert(
                "line_items".to_string(),
                serde_json::to_string(&line_items).unwrap_or_default(),
            );
            meta
        },
        latest_charge: None,
        client_secret: Some(client_secret.clone()),
    };

    // Store the payment intent
    state
        .payment_intents
        .lock()
        .unwrap()
        .insert(payment_intent_id.clone(), payment_intent);

    // Create the checkout session
    let checkout_session = StripeCheckoutSession {
        id: session_id.clone(),
        object: "checkout.session".to_string(),
        status: CheckoutSessionStatus::Open,
        payment_status: PaymentStatus::Unpaid,
        currency: currency.clone(),
        amount_total,
        amount_subtotal,
        created: now,
        expires_at: Some(now + 1800), // 30 minutes
        customer: request.customer,
        customer_email: request.customer_email,
        payment_intent: Some(payment_intent_id),
        client_secret: Some(client_secret),
        line_items: CheckoutLineItemList {
            object: "list".to_string(),
            data: line_items,
            has_more: false,
            url: format!("/v1/checkout/sessions/{}/line_items", session_id),
        },
        metadata: request.metadata,
        success_url: request.success_url,
        cancel_url: request.cancel_url,
    };

    // Store the checkout session
    state
        .checkout_sessions
        .lock()
        .unwrap()
        .insert(session_id.clone(), checkout_session.clone());

    (StatusCode::OK, Json(checkout_session)).into_response()
}

/// GET /checkout/sessions/:id - Retrieve a checkout session
pub async fn retrieve_checkout_session(
    Extension(state): Extension<CatalogState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let sessions = state.checkout_sessions.lock().unwrap();

    if let Some(session) = sessions.get(&session_id) {
        // Check if we need to update status based on payment intent
        let mut session = session.clone();

        if let Some(pi_id) = &session.payment_intent
            && let Some(pi) = state.payment_intents.lock().unwrap().get(pi_id)
        {
            session.payment_status = match pi.status {
                PaymentIntentStatus::Succeeded => PaymentStatus::Paid,
                _ => PaymentStatus::Unpaid,
            };
            if session.payment_status == PaymentStatus::Paid {
                session.status = CheckoutSessionStatus::Complete;
            }
        }

        (StatusCode::OK, Json(session)).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {
                    "message": format!("No such checkout session: '{}'", session_id),
                    "type": "invalid_request_error"
                }
            })),
        )
            .into_response()
    }
}

/// GET /checkout/sessions/:id/line_items - Get line items for a checkout session
pub async fn list_checkout_session_line_items(
    Extension(state): Extension<CatalogState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let sessions = state.checkout_sessions.lock().unwrap();

    if let Some(session) = sessions.get(&session_id) {
        (StatusCode::OK, Json(&session.line_items)).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {
                    "message": format!("No such checkout session: '{}'", session_id),
                    "type": "invalid_request_error"
                }
            })),
        )
            .into_response()
    }
}

/// POST /checkout/sessions/:id/expire - Expire a checkout session
pub async fn expire_checkout_session(
    Extension(state): Extension<CatalogState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let mut sessions = state.checkout_sessions.lock().unwrap();

    if let Some(session) = sessions.get_mut(&session_id) {
        if session.status != CheckoutSessionStatus::Open {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "message": "Only open checkout sessions can be expired",
                        "type": "invalid_request_error"
                    }
                })),
            )
                .into_response();
        }

        session.status = CheckoutSessionStatus::Expired;
        (StatusCode::OK, Json(session.clone())).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {
                    "message": format!("No such checkout session: '{}'", session_id),
                    "type": "invalid_request_error"
                }
            })),
        )
            .into_response()
    }
}
