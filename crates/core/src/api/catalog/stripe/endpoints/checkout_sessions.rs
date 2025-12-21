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
    let session_id = format!(
        "cs_{}",
        Uuid::new_v4().to_string().replace("-", "")[..24].to_string()
    );

    // Process line items and calculate totals
    let mut line_items: Vec<CheckoutLineItem> = Vec::new();
    let mut amount_subtotal: i64 = 0;
    let currency = request
        .line_items
        .first()
        .and_then(|item| item.price_data.as_ref())
        .map(|pd| pd.currency.clone())
        .unwrap_or_else(|| "usd".to_string());

    for (index, item) in request.line_items.iter().enumerate() {
        let line_item_id = format!(
            "li_{}",
            Uuid::new_v4().to_string().replace("-", "")[..24].to_string()
        );

        // Get price info from price_data or look up by price ID
        let (unit_amount, item_currency, product_name, product_description, product_id) =
            if let Some(price_data) = &item.price_data {
                (
                    price_data.unit_amount,
                    price_data.currency.clone(),
                    price_data.product_data.name.clone(),
                    price_data.product_data.description.clone(),
                    // Generate product ID from name if not provided
                    price_data
                        .product_data
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("product_id").cloned())
                        .unwrap_or_else(|| format!("prod_{}", index)),
                )
            } else if let Some(price_id) = &item.price {
                // Look up price from catalog
                if let Some(product) = state
                    .products
                    .iter()
                    .find(|p| p.prices.iter().any(|pr| pr.id == *price_id))
                {
                    let price = product.prices.iter().find(|pr| pr.id == *price_id).unwrap();
                    (
                        price.unit_amount.unwrap_or(0),
                        price.currency.as_str().to_string(),
                        product.name.clone().unwrap_or_else(|| product.id.clone()),
                        product.description.clone(),
                        product.id.clone(),
                    )
                } else {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": {
                                "message": format!("Price {} not found", price_id),
                                "type": "invalid_request_error"
                            }
                        })),
                    )
                        .into_response();
                }
            } else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": {
                            "message": "Either price or price_data must be provided",
                            "type": "invalid_request_error"
                        }
                    })),
                )
                    .into_response();
            };

        let quantity = item.quantity;
        let subtotal = unit_amount * quantity;
        amount_subtotal += subtotal;

        line_items.push(CheckoutLineItem {
            id: line_item_id,
            object: "item".to_string(),
            amount_subtotal: subtotal,
            amount_total: subtotal, // No tax/discount for now
            currency: item_currency.clone(),
            quantity,
            description: product_description,
            price: CheckoutLineItemPrice {
                id: item
                    .price
                    .clone()
                    .unwrap_or_else(|| format!("price_{}", index)),
                object: "price".to_string(),
                currency: item_currency,
                unit_amount,
                product: Some(product_id),
                nickname: None,
                price_type: "one_time".to_string(),
            },
            amount_discount: None,
            amount_tax: None,
        });
    }

    let amount_total = amount_subtotal; // No tax/discount for now

    // Create the underlying payment intent
    let payment_intent_id = format!(
        "pi_{}",
        Uuid::new_v4().to_string().replace("-", "")[..24].to_string()
    );
    let client_secret = format!(
        "{}_secret_{}",
        payment_intent_id,
        Uuid::new_v4().to_string().replace("-", "")[..24].to_string()
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

        if let Some(pi_id) = &session.payment_intent {
            if let Some(pi) = state.payment_intents.lock().unwrap().get(pi_id) {
                session.payment_status = match pi.status {
                    PaymentIntentStatus::Succeeded => PaymentStatus::Paid,
                    _ => PaymentStatus::Unpaid,
                };
                if session.payment_status == PaymentStatus::Paid {
                    session.status = CheckoutSessionStatus::Complete;
                }
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
