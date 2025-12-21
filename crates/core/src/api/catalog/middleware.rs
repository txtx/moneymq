use axum::{
    body::Body,
    extract::State,
    handler::Handler,
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{MethodRouter, get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use moneymq_types::x402::{
    ExactPaymentPayload, FacilitatorErrorReason, PaymentPayload, PaymentRequirements,
    SupportedResponse,
};
use serde_json::json;
use tracing::{debug, error, info, warn};

use crate::api::{
    catalog::{
        CatalogState,
        stripe::endpoints::{
            billing::BillingMeterEventRequest, subscriptions::SubscriptionRequest,
        },
    },
    payment::{
        endpoints::FacilitatorExtraContext, networks::solana::extract_customer_from_transaction,
    },
};

#[derive(thiserror::Error, Debug)]
pub enum X402FacilitatorRequestError {
    #[error("Failed to contact facilitator: {0}")]
    FailedToContactFacilitator(reqwest::Error),
    #[error("Facilitator error: {0}")]
    FacilitatorError(reqwest::Error),
    #[error("Failed to parse facilitator response: {0}")]
    FacilitatorResponseParseError(reqwest::Error),
    #[error("Payment verification failed: {0}")]
    PaymentVerificationFailed(FacilitatorErrorReason),
    #[error("Payment settlement failed{}", .0.as_ref().map(|r| format!(": {:?}", r)).unwrap_or_default())]
    PaymentSettlementFailed(Option<FacilitatorErrorReason>),
}

#[derive(thiserror::Error, Debug)]
pub enum X402MiddlewareError {
    #[error("Failed to fetch /supported from facilitator: {0}")]
    SupportedFetchError(X402FacilitatorRequestError),
    #[error("Failed to verify with facilitator: {0}")]
    VerifyError(X402FacilitatorRequestError),
    #[error("Failed to settle with facilitator: {0}")]
    SettleError(X402FacilitatorRequestError),
    #[error("Payment required to access this resource. Please include X-Payment header.")]
    PaymentRequired(Vec<PaymentRequirements>),
    #[error("Invalid X-Payment header: {0}")]
    InvalidPaymentHeader(String),
}

impl Into<Response> for X402MiddlewareError {
    fn into(self) -> Response {
        // For PaymentRequired, return x402 protocol standard format
        if let X402MiddlewareError::PaymentRequired(requirements) = &self {
            let body = json!({
                "x402Version": 1,
                "accepts": requirements,
            });
            return (StatusCode::PAYMENT_REQUIRED, axum::Json(body)).into_response();
        }

        // For other errors, return standard error format
        let (status, code, err_type) = match &self {
            X402MiddlewareError::SupportedFetchError(_) => (
                StatusCode::BAD_GATEWAY,
                "get_supported_failed",
                "invalid_request_error",
            ),
            X402MiddlewareError::VerifyError(_) => (
                StatusCode::PAYMENT_REQUIRED,
                "payment_verification_failed",
                "invalid_request_error",
            ),
            X402MiddlewareError::SettleError(_) => (
                StatusCode::PAYMENT_REQUIRED,
                "payment_settlement_failed",
                "invalid_request_error",
            ),
            X402MiddlewareError::PaymentRequired(_) => unreachable!(),
            X402MiddlewareError::InvalidPaymentHeader(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_payment_header",
                "invalid_request_error",
            ),
        };

        let message = self.to_string();
        let body = json!({
            "error": {
                "code": code,
                "message": message,
                "type": err_type,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

async fn fetch_supported(
    state: &CatalogState,
) -> Result<SupportedResponse, X402FacilitatorRequestError> {
    let supported_url = format!("{}supported", state.facilitator_url);

    let client = reqwest::Client::new();
    let response = client
        .get(&supported_url)
        .send()
        .await
        .map_err(|e| X402FacilitatorRequestError::FailedToContactFacilitator(e))?;

    if !response.status().is_success() {
        let error = response.error_for_status().unwrap_err();
        return Err(X402FacilitatorRequestError::FacilitatorError(error));
    }

    let supported: SupportedResponse = response
        .json()
        .await
        .map_err(|e| X402FacilitatorRequestError::FacilitatorResponseParseError(e))?;

    debug!(
        "Fetched supported payment kinds from facilitator: {:?}",
        supported.kinds
    );

    Ok(supported)
}

/// Extract and validate payment payload from request headers
async fn extract_payment_payload(
    headers: &HeaderMap,
    payment_requirements: &[PaymentRequirements],
) -> Result<PaymentPayload, X402MiddlewareError> {
    // Check for X-Payment header
    let payment_header = headers.get("X-Payment");
    debug!("Payment Header => {:?}", payment_header);

    match payment_header {
        None => {
            // No payment header - return 402 with requirements
            info!("Required - No X-Payment header found");

            Err(X402MiddlewareError::PaymentRequired(
                payment_requirements.to_vec(),
            ))
        }
        Some(payment_header) => {
            info!("Verifying - X-Payment header found");
            // Parse the payment payload
            let header_str = payment_header.to_str().map_err(|_| {
                X402MiddlewareError::InvalidPaymentHeader(
                    "Header contains invalid characters".to_string(),
                )
            })?;

            // Decode base64 and parse JSON
            let decoded = BASE64.decode(header_str.as_bytes()).map_err(|_| {
                X402MiddlewareError::InvalidPaymentHeader("Header is not valid base64".to_string())
            })?;

            let payment_payload: PaymentPayload =
                serde_json::from_slice(&decoded).map_err(|e| {
                    X402MiddlewareError::InvalidPaymentHeader(format!(
                        "Failed to parse payment payload: {}",
                        e
                    ))
                })?;

            Ok(payment_payload)
        }
    }
}

/// Verify payment with the facilitator by calling its /verify endpoint
async fn verify_payment_with_facilitator(
    state: &CatalogState,
    payment_payload: &PaymentPayload,
    payment_requirements: &PaymentRequirements,
) -> Result<moneymq_types::x402::MixedAddress, X402FacilitatorRequestError> {
    use moneymq_types::x402::{VerifyRequest, VerifyResponse, X402Version};

    // Construct the verify request
    let verify_request = VerifyRequest {
        x402_version: X402Version::V1,
        payment_payload: payment_payload.clone(),
        payment_requirements: payment_requirements.clone(),
    };

    // Build the facilitator verify URL
    let verify_url = format!("{}verify", state.facilitator_url);

    // Make HTTP request to facilitator
    let client = reqwest::Client::new();
    let response = client
        .post(&verify_url)
        .json(&verify_request)
        .send()
        .await
        .map_err(X402FacilitatorRequestError::FailedToContactFacilitator)?;

    // Check status code
    if !response.status().is_success() {
        let error = response.error_for_status().unwrap_err();
        return Err(X402FacilitatorRequestError::FacilitatorError(error));
    }

    // Get response text for debug data
    let response_text = response
        .text()
        .await
        .map_err(X402FacilitatorRequestError::FacilitatorResponseParseError)?;

    // Parse response
    let verify_response: VerifyResponse =
        serde_json::from_str(&response_text).expect("Failed to parse verify response");

    // Check if payment is valid
    match verify_response {
        VerifyResponse::Valid { payer } => Ok(payer),
        VerifyResponse::Invalid { reason, .. } => Err(
            X402FacilitatorRequestError::PaymentVerificationFailed(reason),
        ),
    }
}

/// Settle payment with the facilitator by calling its /settle endpoint
async fn settle_payment_with_facilitator(
    state: &CatalogState,
    payment_payload: &PaymentPayload,
    payment_requirements: &PaymentRequirements,
) -> Result<(), X402FacilitatorRequestError> {
    use moneymq_types::x402::{SettleRequest, SettleResponse, X402Version};

    // Construct the settle request (identical structure to verify request)
    let settle_request = SettleRequest {
        x402_version: X402Version::V1,
        payment_payload: payment_payload.clone(),
        payment_requirements: payment_requirements.clone(),
    };

    // Build the facilitator settle URL
    let settle_url = format!("{}settle", state.facilitator_url);

    // Make HTTP request to facilitator
    let client = reqwest::Client::new();
    let response = client
        .post(&settle_url)
        .json(&settle_request)
        .send()
        .await
        .map_err(X402FacilitatorRequestError::FailedToContactFacilitator)?;

    // Check status code
    if !response.status().is_success() {
        let error = response.error_for_status().unwrap_err();
        return Err(X402FacilitatorRequestError::FacilitatorError(error));
    }

    // Get response as text for debug purposes
    let response_text = response
        .text()
        .await
        .map_err(X402FacilitatorRequestError::FacilitatorResponseParseError)?;

    // Parse response
    let settle_response: SettleResponse =
        serde_json::from_str(&response_text).expect("Failed to parse settle response");

    // Check if settlement was successful
    if settle_response.success {
        if let Some(ref tx_hash) = settle_response.transaction {
            debug!("  Transaction hash: {}", tx_hash);
        }
        Ok(())
    } else {
        Err(X402FacilitatorRequestError::PaymentSettlementFailed(
            settle_response.error_reason,
        ))
    }
}

/// Extract payment amount and description from request
/// Returns (amount_in_cents, description)
/// Returns (amount, description, product_id)
fn extract_payment_details(state: &CatalogState, req_path: &str) -> Option<(i64, String, String)> {
    // Check if this is a payment intent confirm request
    // Support both /payment_intents/{id}/confirm (nested under /catalog/v1)
    // and /v1/payment_intents/{id}/confirm (legacy)
    let is_confirm_request = req_path.ends_with("/confirm")
        && (req_path.starts_with("/payment_intents/")
            || req_path.starts_with("/v1/payment_intents/"));

    if is_confirm_request {
        // Extract payment intent ID from path
        let parts: Vec<&str> = req_path.split('/').collect();
        // For /payment_intents/{id}/confirm -> parts = ["", "payment_intents", "{id}", "confirm"]
        // For /v1/payment_intents/{id}/confirm -> parts = ["", "v1", "payment_intents", "{id}", "confirm"]
        let payment_intent_id = if req_path.starts_with("/v1/") {
            parts.get(3).copied()
        } else {
            parts.get(2).copied()
        };

        if let Some(payment_intent_id) = payment_intent_id {
            // Look up the payment intent from state
            if let Ok(payment_intents) = state.payment_intents.lock() {
                if let Some(intent) = payment_intents.get(payment_intent_id) {
                    let description = intent
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("Payment intent {}", payment_intent_id));

                    // Build product quantities JSON from line_items (e.g., {"surfnet-max": 1, "other": 2})
                    let product_quantities = intent
                        .metadata
                        .get("line_items")
                        .and_then(|line_items_json| {
                            // Parse line_items JSON array and build product -> quantity map
                            serde_json::from_str::<Vec<serde_json::Value>>(line_items_json)
                                .ok()
                                .map(|items| {
                                    let mut quantities: std::collections::HashMap<String, i64> =
                                        std::collections::HashMap::new();
                                    for item in items {
                                        // Try to get product_id from item.price.product (checkout session format)
                                        // or from item.product_id (legacy format)
                                        let product_id = item
                                            .get("price")
                                            .and_then(|p| p.get("product"))
                                            .and_then(|v| v.as_str())
                                            .or_else(|| {
                                                item.get("product_id").and_then(|v| v.as_str())
                                            });

                                        let quantity = item
                                            .get("quantity")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(1);

                                        if let Some(pid) = product_id {
                                            *quantities.entry(pid.to_string()).or_insert(0) +=
                                                quantity;
                                        }
                                    }
                                    serde_json::to_string(&quantities)
                                        .unwrap_or_else(|_| "{}".to_string())
                                })
                        })
                        // Fallback to legacy product_id field (single product, quantity 1)
                        .or_else(|| {
                            intent.metadata.get("product_id").map(|pid| {
                                let mut map = std::collections::HashMap::new();
                                map.insert(pid.clone(), 1i64);
                                serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
                            })
                        })
                        // Final fallback to payment intent ID
                        .unwrap_or_else(|| {
                            let mut map = std::collections::HashMap::new();
                            map.insert(payment_intent_id.to_string(), 1i64);
                            serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
                        });

                    // Return amount in cents - the middleware will do the conversion to token amount
                    return Some((intent.amount, description, product_quantities));
                } else {
                    println!(
                        "WARN: Payment intent {} not found in state",
                        payment_intent_id
                    );
                }
            }
        }
    }
    None
}

/// Middleware to handle payment requirements for meter events
pub async fn payment_middleware(
    State(state): State<CatalogState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    use moneymq_types::x402::{Network, Scheme, TokenAmount};

    let (parts, body) = req.into_parts();
    let request_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .unwrap_or_default();

    let mut req = Request::from_parts(parts, Body::from(request_bytes.clone()));

    let (description, amount, _is_margin, product_id) = {
        let billing_event = BillingMeterEventRequest::parse(&request_bytes);
        if let Some(event_name) = billing_event.event_name {
            debug!("Parsed billing event name from request: {}", event_name);

            let billing_event = state.meters.iter().find(|m| m.event_name == event_name);
            if let Some(billing_event) = billing_event {
                (
                    billing_event
                        .display_name
                        .clone()
                        .unwrap_or_else(|| billing_event.event_name.clone()),
                    // TODO: need to figure out price for billing events
                    100,
                    false,
                    // Use meter ID for tracking
                    billing_event.id.clone(),
                )
            } else {
                // Unknown billing event, default amount
                ("Meter Event".into(), 100, false, "unknown-meter".into())
            }
        } else {
            let subscription_req = SubscriptionRequest::parse(&request_bytes);
            if let Some(_) = subscription_req.customer {
                debug!(
                    "Parsed subscription request from request, price IDs: {:?}",
                    subscription_req.price_ids
                );

                let active_price = state
                    .products
                    .iter()
                    .flat_map(|product| {
                        product.prices.iter().filter_map(|price| {
                            let price_id = if state.use_sandbox {
                                price.sandboxes.get("default")
                            } else {
                                price.deployed_id.as_ref()
                            }
                            .unwrap_or(&price.id);
                            if subscription_req.price_ids.contains(&price_id) && price.active {
                                // Note: "margin" pricing type is not currently supported
                                let is_margin = false;
                                let price = price.unit_amount.clone().unwrap_or(1);
                                let description =
                                    product.statement_descriptor.clone().unwrap_or_else(|| {
                                        product.name.clone().unwrap_or("Product".to_string())
                                    });
                                // Use product ID for tracking, not the display name
                                let product_id = product.id.clone();
                                Some((description, price, is_margin, product_id))
                            } else {
                                None
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                let (description, price, is_margin, product_id) =
                    active_price.first().cloned().unwrap_or((
                        "Unknown Product".to_string(),
                        1,
                        false,
                        "unknown".to_string(),
                    )); // TODO: handle multiple prices properly

                (description, price, is_margin, product_id)
            } else {
                // Try payment intent first, then product access path
                if let Some((price, description, product_id)) =
                    extract_payment_details(&state, req.uri().path())
                {
                    (description, price, false, product_id)
                } else if let Some((price, description, product_id)) =
                    extract_product_from_path(&state, req.uri().path())
                {
                    // Product access path (e.g., /products/{id}/access)
                    (description, price, false, product_id)
                } else {
                    // No payment context found, pass through without gating
                    return next.run(req).await;
                }
            }
        }
    };

    debug!(
        "Creating payment requirements for resource: {}, Amount: {}",
        description, amount
    );

    let supported = match fetch_supported(&state).await {
        Ok(supported) => supported,
        Err(e) => {
            debug!("Failed to fetch supported payment kinds: {}", e);
            return X402MiddlewareError::SupportedFetchError(e).into();
        }
    };

    let network = Network::Solana; // TODO: Determine network based on request / product
    let Some(network_config) = state.networks_config.get_config_for_network(&network) else {
        panic!("No billing config for network {:?}", network); // TODO: Handle this error properly
    };

    // TODO: probably need some sort of filtering here based on product being accessed
    let assets = network_config
        .currencies()
        .iter()
        .map(|currency| (currency.address(), currency.decimals()))
        .collect::<Vec<_>>();

    let recipient = network_config.recipient();

    // TODO: allow pay to to be overridden by product
    // TODO: consider allowing the assets allowed to be overridden by product

    let payment_requirements = assets
        .into_iter()
        .map(|(asset, decimals)| {
            let token_amount =
                TokenAmount((amount * 10_i64.pow((decimals as u32).saturating_sub(2))).to_string());
            debug!(
                "  Payment Requirement - Asset: {}, Amount (raw): {}",
                asset, token_amount.0
            );
            PaymentRequirements {
                scheme: Scheme::Exact,
                network: network.clone(),
                max_amount_required: token_amount,
                resource: state.facilitator_url.clone(), // TODO: I think this should actually be the resource being accessed
                // TODO: our price is in cents, but USDC has 6 decimals.
                // We need a consistent conversion between these rather than assuming 2 decimals
                description: format!("Payment for {}", description),
                mime_type: "application/json".to_string(),
                output_schema: None,
                pay_to: recipient.address(),
                max_timeout_seconds: 300,
                asset,
                extra: Some(json!({
                    "feePayer": supported
                    .kinds
                    .iter()
                    .find(|kind| kind.network == network)
                    .and_then(|kind| kind.extra.as_ref().map(|e| e.fee_payer.clone())),
                    "product": product_id,
                })),
            }
        })
        .collect::<Vec<_>>();

    let mut selected_payment_requirement = payment_requirements[0].clone(); // For now, just use the first one

    let headers = req.headers();

    match extract_payment_payload(headers, &payment_requirements).await {
        Ok(payment_payload) => {
            // Payment header found and valid
            info!("Received - Valid payment payload received");
            debug!(
                "  Payment scheme: {:?}, network: {:?}",
                payment_payload.scheme, payment_payload.network
            );

            // Extract customer pubkey from the transaction
            let customer_pubkey = match &payment_payload.payload {
                ExactPaymentPayload::Solana(solana_payload) => {
                    match extract_customer_from_transaction(&solana_payload.transaction) {
                        Ok(pubkey) => {
                            info!("Extracted customer pubkey: {}", pubkey);
                            Some(pubkey)
                        }
                        Err(e) => {
                            warn!("Failed to extract customer pubkey: {}", e);
                            None
                        }
                    }
                }
            };

            debug!("  Customer: {:?}", customer_pubkey);

            // Find the customer label by matching the customer pubkey with user accounts
            let customer_label = customer_pubkey.and_then(|customer| {
                network_config
                    .user_accounts()
                    .iter()
                    .find(|account| {
                        if let moneymq_types::x402::MixedAddress::Solana(addr) = account.address() {
                            addr == customer
                        } else {
                            false
                        }
                    })
                    .and_then(|account| account.label())
            });

            // Extract currency from asset
            let currency = network_config
                .currencies()
                .iter()
                .find(|c| c.address() == selected_payment_requirement.asset)
                .and_then(|c| c.solana_currency())
                .map(|sc| sc.symbol.clone())
                .unwrap_or_else(|| "USDC".to_string());

            let current_extra = selected_payment_requirement
                .extra
                .take()
                .unwrap_or_default();
            let mut new_extra: FacilitatorExtraContext =
                serde_json::from_value(current_extra).unwrap();
            new_extra.customer_address = customer_pubkey.as_ref().map(|c| c.to_string());
            new_extra.customer_label = customer_label.clone();
            new_extra.currency = Some(currency.clone());
            selected_payment_requirement.extra = Some(serde_json::to_value(new_extra).unwrap());

            // Verify payment with facilitator
            match verify_payment_with_facilitator(
                &state,
                &payment_payload,
                &selected_payment_requirement,
            )
            .await
            {
                Ok(payer) => {
                    info!("Verified - Payment verified by facilitator");
                    debug!("  Payer: {:?}", payer);

                    // Store payer and customer in request extensions for use by handler
                    req.extensions_mut().insert(payer.clone());

                    // Continue to the handler
                    let response = next.run(req).await;

                    // Post-process the response
                    if response.status().is_success() {
                        println!("\x1b[32m$ Success\x1b[0m - Payment completed successfully");

                        // Call facilitator /settle endpoint to finalize payment
                        match settle_payment_with_facilitator(
                            &state,
                            &payment_payload,
                            &selected_payment_requirement,
                        )
                        .await
                        {
                            Ok(_) => {
                                info!("Settled - Payment settled on-chain");
                            }
                            Err(error_message) => {
                                error!("Settlement Failed - {}", error_message);
                                // Note: We don't fail the request here since the service was already provided
                            }
                        }
                    }

                    response
                }
                Err(error_message) => {
                    error!("Payment verification failed: {}", error_message);
                    X402MiddlewareError::VerifyError(error_message).into()
                }
            }
        }
        Err(error) => {
            warn!("Payment extraction failed: {}", error);
            // No payment or invalid payment
            error.into()
        }
    }
}

/// Helper function to create a POST route with payment middleware
///
/// # Example
/// ```
/// use crate::api::payment::endpoints::middleware::x402_post;
///
/// let route = x402_post(my_handler, state.clone());
/// ```
pub fn x402_post<H, T>(handler: H, state: Option<CatalogState>) -> MethodRouter<CatalogState>
where
    H: Handler<T, CatalogState>,
    T: 'static,
{
    let Some(state) = state else {
        return post(|| async { StatusCode::NOT_FOUND });
    };
    post(handler).layer(middleware::from_fn_with_state(state, payment_middleware))
}

/// Helper function to create a GET route with product access payment middleware
///
/// This is for raw x402 gating - the product and price are determined from the URL path.
/// The endpoint will return 402 with payment requirements if no valid payment is provided.
///
/// # Example
/// ```
/// use crate::api::payment::endpoints::middleware::x402_get;
///
/// let route = x402_get(my_handler, state.clone());
/// ```
pub fn x402_get<H, T>(handler: H, state: Option<CatalogState>) -> MethodRouter<CatalogState>
where
    H: Handler<T, CatalogState>,
    T: 'static,
{
    let Some(state) = state else {
        return get(|| async { StatusCode::NOT_FOUND });
    };
    // Use the unified payment_middleware - it handles product access via extract_product_from_path
    get(handler).layer(middleware::from_fn_with_state(state, payment_middleware))
}

/// Extract product info from path like /products/{product_id}/access
/// Returns (amount_in_cents, description, product_id)
fn extract_product_from_path(state: &CatalogState, path: &str) -> Option<(i64, String, String)> {
    // Match pattern: /products/{product_id}/access
    let parts: Vec<&str> = path.split('/').collect();

    // Find the "products" segment and get the next one as product_id
    let product_id = parts
        .iter()
        .position(|&p| p == "products")
        .and_then(|idx| parts.get(idx + 1))
        .map(|s| s.to_string())?;

    // Check this is an access request
    if !path.ends_with("/access") {
        return None;
    }

    // Look up the product and its default price
    let product = state.products.iter().find(|p| p.id == product_id)?;

    // Get the default/first active price
    let price = product.prices.iter().find(|p| p.active)?;

    let amount = price.unit_amount.unwrap_or(0);
    let description = product.name.clone().unwrap_or_else(|| product_id.clone());

    Some((amount, description, product_id))
}
