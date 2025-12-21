use std::collections::HashMap;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use crate::api::catalog::{
    CatalogState,
    stripe::{
        types::{PaymentIntentStatus, StripePaymentIntent},
        utils::generate_stripe_id,
    },
};

/// POST /v1/payment_intents - Create a payment intent
pub async fn create_payment_intent(
    State(state): State<CatalogState>,
    body: Bytes,
) -> impl IntoResponse {
    // Try to parse as JSON first, then fall back to form-encoded
    let body_str = String::from_utf8_lossy(&body);

    let (amount, currency, customer, payment_method, description, confirm, metadata) =
        if body_str.trim().starts_with('{') {
            // Parse as JSON
            let json_value: serde_json::Value = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(e) => {
                    println!("ERROR: Failed to parse JSON body: {}", e);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": {
                                "message": format!("Invalid JSON: {}", e),
                                "type": "invalid_request_error"
                            }
                        })),
                    )
                        .into_response();
                }
            };

            let amount = json_value
                .get("amount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let currency = json_value
                .get("currency")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "usd".to_string());
            let customer = json_value
                .get("customer")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let payment_method = json_value
                .get("payment_method")
                .or_else(|| json_value.get("paymentMethod"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let description = json_value
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let confirm = json_value
                .get("confirm")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let metadata = json_value
                .get("metadata")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();

            (
                amount,
                currency,
                customer,
                payment_method,
                description,
                confirm,
                metadata,
            )
        } else {
            // Parse as form-encoded
            let mut params: HashMap<String, String> = HashMap::new();

            for part in body_str.split('&') {
                if let Some((key, value)) = part.split_once('=') {
                    let decoded_key = urlencoding::decode(key).unwrap_or_default().to_string();
                    let decoded_value = urlencoding::decode(value).unwrap_or_default().to_string();
                    params.insert(decoded_key, decoded_value);
                }
            }

            let amount = params
                .get("amount")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let currency = params
                .get("currency")
                .cloned()
                .unwrap_or_else(|| "usd".to_string());
            let customer = params.get("customer").cloned();
            let payment_method = params.get("payment_method").cloned();
            let description = params.get("description").cloned();
            let confirm = params
                .get("confirm")
                .and_then(|s| s.parse::<bool>().ok())
                .unwrap_or(false);

            // Extract metadata (metadata[key]=value format)
            let metadata: HashMap<String, String> = params
                .iter()
                .filter_map(|(k, v)| {
                    if k.starts_with("metadata[") && k.ends_with(']') {
                        let key = k
                            .strip_prefix("metadata[")
                            .and_then(|s| s.strip_suffix(']'))
                            .unwrap_or(k);
                        Some((key.to_string(), v.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            (
                amount,
                currency,
                customer,
                payment_method,
                description,
                confirm,
                metadata,
            )
        };

    let payment_intent_id = generate_stripe_id("pi");
    let created = chrono::Utc::now().timestamp();

    // Determine initial status
    let status = if confirm {
        PaymentIntentStatus::Succeeded
    } else if payment_method.is_some() {
        PaymentIntentStatus::RequiresConfirmation
    } else {
        PaymentIntentStatus::RequiresPaymentMethod
    };

    let client_secret = Some(format!(
        "{}_secret_{}",
        payment_intent_id,
        generate_stripe_id("")
    ));

    let latest_charge = if status == PaymentIntentStatus::Succeeded {
        Some(generate_stripe_id("ch"))
    } else {
        None
    };

    let payment_intent = StripePaymentIntent {
        id: payment_intent_id.clone(),
        object: "payment_intent".to_string(),
        amount,
        currency,
        status,
        created,
        customer,
        payment_method,
        description,
        metadata,
        latest_charge,
        client_secret,
    };

    // Store payment intent in state
    state
        .payment_intents
        .lock()
        .unwrap()
        .insert(payment_intent_id, payment_intent.clone());

    (StatusCode::OK, Json(payment_intent)).into_response()
}

/// GET /v1/payment_intents/:id - Retrieve a payment intent
pub async fn retrieve_payment_intent(
    State(state): State<CatalogState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let payment_intents = state.payment_intents.lock().unwrap();
    let payment_intent = payment_intents.get(&id).cloned().unwrap_or_else(|| {
        // Fallback for backward compatibility
        StripePaymentIntent {
            id: id.clone(),
            object: "payment_intent".to_string(),
            amount: 4990,
            currency: "usd".to_string(),
            status: PaymentIntentStatus::RequiresPaymentMethod,
            created: chrono::Utc::now().timestamp(),
            customer: None,
            payment_method: None,
            description: None,
            metadata: HashMap::new(),
            latest_charge: None,
            client_secret: Some(format!("{}_secret_{}", id, generate_stripe_id(""))),
        }
    });

    (StatusCode::OK, Json(payment_intent))
}

/// POST /v1/payment_intents/:id/confirm - Confirm a payment intent
pub async fn confirm_payment_intent(
    State(state): State<CatalogState>,
    Path(id): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    // Parse form-encoded body
    let body_str = String::from_utf8_lossy(&body);
    let mut params: HashMap<String, String> = HashMap::new();

    for part in body_str.split('&') {
        if let Some((key, value)) = part.split_once('=') {
            let decoded_value = urlencoding::decode(value).unwrap_or_default().to_string();
            params.insert(key.to_string(), decoded_value);
        }
    }

    let payment_method = params.get("payment_method").cloned();

    // Look up the payment intent from state
    let mut payment_intents = state.payment_intents.lock().unwrap();
    let mut payment_intent = payment_intents.get(&id).cloned().unwrap_or_else(|| {
        // Fallback for backward compatibility
        StripePaymentIntent {
            id: id.clone(),
            object: "payment_intent".to_string(),
            amount: 4990,
            currency: "usd".to_string(),
            status: PaymentIntentStatus::RequiresConfirmation,
            created: chrono::Utc::now().timestamp(),
            customer: None,
            payment_method: None,
            description: None,
            metadata: HashMap::new(),
            latest_charge: None,
            client_secret: Some(format!("{}_secret_{}", id, generate_stripe_id(""))),
        }
    });

    // Update the payment intent status to succeeded
    payment_intent.status = PaymentIntentStatus::Succeeded;
    payment_intent.latest_charge = Some(generate_stripe_id("ch"));
    if let Some(pm) = payment_method {
        payment_intent.payment_method = Some(pm);
    }

    // Store the updated payment intent
    payment_intents.insert(id, payment_intent.clone());

    (StatusCode::OK, Json(payment_intent))
}

/// POST /v1/payment_intents/:id/cancel - Cancel a payment intent
pub async fn cancel_payment_intent(
    State(_state): State<CatalogState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let payment_intent = StripePaymentIntent {
        id: id.clone(),
        object: "payment_intent".to_string(),
        amount: 4990, // Would come from storage
        currency: "usd".to_string(),
        status: PaymentIntentStatus::Canceled,
        created: chrono::Utc::now().timestamp(),
        customer: None,
        payment_method: None,
        description: None,
        metadata: HashMap::new(),
        latest_charge: None,
        client_secret: Some(format!("{}_secret_{}", id, generate_stripe_id(""))),
    };

    (StatusCode::OK, Json(payment_intent))
}
