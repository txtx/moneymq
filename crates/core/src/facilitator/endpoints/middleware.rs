use axum::{
    body::Body,
    extract::State,
    handler::Handler,
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{MethodRouter, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use moneymq_types::x402::{
    ExactPaymentPayload, FacilitatorErrorReason, PaymentPayload, PaymentRequirements,
    SupportedResponse,
    transactions::{FacilitatedTransaction, TransactionStatus},
};
use serde_json::json;
use tracing::{debug, error, info, warn};

use crate::{
    facilitator::networks::solana::extract_customer_from_transaction,
    provider::{ProviderState, stripe::utils::generate_stripe_id},
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
            X402MiddlewareError::PaymentRequired(_) => (
                StatusCode::PAYMENT_REQUIRED,
                "payment_required",
                "invalid_request_error",
            ),
            X402MiddlewareError::InvalidPaymentHeader(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_payment_header",
                "invalid_request_error",
            ),
        };

        let some_payment_requirements = match &self {
            X402MiddlewareError::PaymentRequired(items) => Some(items),
            _ => None,
        };

        let message = self.to_string();
        let body = json!({
            "error": {
                "code": code,
                "message": message,
                "type": err_type,
                "payment_requirements": some_payment_requirements,

            }
        });

        (status, axum::Json(body)).into_response()
    }
}

async fn fetch_supported(
    state: &ProviderState,
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
    state: &ProviderState,
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

    // Parse response
    let verify_response: VerifyResponse = response
        .json()
        .await
        .map_err(X402FacilitatorRequestError::FacilitatorResponseParseError)?;

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
    state: &ProviderState,
    payment_payload: &PaymentPayload,
    payment_requirements: &PaymentRequirements,
) -> Result<moneymq_types::x402::SettleResponse, X402FacilitatorRequestError> {
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

    // Parse response
    let settle_response: SettleResponse = response
        .json()
        .await
        .map_err(X402FacilitatorRequestError::FacilitatorResponseParseError)?;

    // Check if settlement was successful
    if settle_response.success {
        if let Some(ref tx_hash) = settle_response.transaction {
            debug!("  Transaction hash: {}", tx_hash);
        }
        Ok(settle_response)
    } else {
        Err(X402FacilitatorRequestError::PaymentSettlementFailed(
            settle_response.error_reason,
        ))
    }
}

/// Middleware to handle payment requirements for meter events
pub async fn payment_middleware(
    State(state): State<ProviderState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    use moneymq_types::x402::{Network, Scheme, TokenAmount};

    let supported = match fetch_supported(&state).await {
        Ok(supported) => supported,
        Err(e) => {
            debug!("Failed to fetch supported payment kinds: {}", e);
            return X402MiddlewareError::SupportedFetchError(e).into();
        }
    };

    let network = Network::Solana; // TODO: Determine network based on request / product
    let Some(billing_config) = state.billing_manager.get_config_for_network(&network) else {
        panic!("No billing config for network {:?}", network); // TODO: Handle this error properly
    };

    // TODO: probably need some sort of filtering here based on product being accessed
    let assets = billing_config
        .currencies()
        .iter()
        .map(|currency| currency.address())
        .collect::<Vec<_>>();

    let recipient = billing_config.recipient();

    // TODO: allow pay to to be overridden by product
    // TODO: consider allowing the assets allowed to be overridden by product

    // TODO: Get payment requirements from state/config
    // For now, create a mock payment requirement for testing
    let payment_requirements = assets
        .into_iter()
        .map(|asset| {
            PaymentRequirements {
                scheme: Scheme::Exact,
                network: network.clone(),
                max_amount_required: TokenAmount("1000000".to_string()), // 1 USDC (6 decimals)
                resource: state.facilitator_url.clone(), // TODO: I think this should actually be the resource being accessed
                description: "Payment for meter event".to_string(),
                mime_type: "application/json".to_string(),
                output_schema: None,
                pay_to: recipient.address(),
                max_timeout_seconds: 300,
                asset,
                extra: supported
                    .kinds
                    .iter()
                    .find(|kind| kind.network == network)
                    .and_then(|kind| Some(serde_json::to_value(&kind.extra).unwrap())),
            }
        })
        .collect::<Vec<_>>();

    let headers = req.headers();

    match extract_payment_payload(headers, &payment_requirements).await {
        Ok(payment_payload) => {
            // Payment header found and valid
            info!("Received - Valid payment payload received");
            debug!(
                "  Payment scheme: {:?}, network: {:?}",
                payment_payload.scheme, payment_payload.network
            );

            // Verify payment with facilitator
            match verify_payment_with_facilitator(
                &state,
                &payment_payload,
                &payment_requirements[0], // Use the first payment requirement
            )
            .await
            {
                Ok(payer) => {
                    info!("Verified - Payment verified by facilitator");
                    debug!("  Payer: {:?}", payer);

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

                    // Store payer and customer in request extensions for use by handler
                    req.extensions_mut().insert(payer.clone());

                    // Continue to the handler
                    let response = next.run(req).await;

                    // Post-process the response
                    if response.status().is_success() {
                        println!("\x1b[32m$ Success\x1b[0m - Request completed successfully");

                        // Call facilitator /settle endpoint to finalize payment
                        match settle_payment_with_facilitator(
                            &state,
                            &payment_payload,
                            &payment_requirements[0],
                        )
                        .await
                        {
                            Ok(settle_response) => {
                                info!("Settled - Payment settled on-chain");
                                // Find the customer label by matching the customer pubkey with user accounts
                                let customer_label = customer_pubkey.and_then(|customer| {
                                    billing_config
                                        .user_accounts()
                                        .iter()
                                        .find(|account| {
                                            if let moneymq_types::x402::MixedAddress::Solana(addr) =
                                                account.address()
                                            {
                                                addr == customer
                                            } else {
                                                false
                                            }
                                        })
                                        .and_then(|account| account.label())
                                });

                                // Extract currency from asset
                                let currency = billing_config
                                    .currencies()
                                    .iter()
                                    .find(|c| c.address() == payment_requirements[0].asset)
                                    .and_then(|c| c.solana_currency())
                                    .map(|sc| sc.symbol.clone())
                                    .unwrap_or_else(|| "USDC".to_string());

                                // Convert amount from raw token amount to decimal string
                                let amount = &payment_requirements[0].max_amount_required.0;

                                let transaction = FacilitatedTransaction::new(
                                    generate_stripe_id("txn"),
                                    chrono::Utc::now().timestamp(),
                                    payment_requirements[0].description.clone(),
                                    customer_label,
                                    customer_pubkey
                                        .map(|c| c.to_string())
                                        .unwrap_or_else(|| payer.to_string()),
                                    amount.clone(),
                                    currency,
                                    TransactionStatus::Completed,
                                    settle_response.transaction.map(|tx| match tx {
                                        moneymq_types::x402::TransactionHash::Solana(bytes) => {
                                            bs58::encode(bytes).into_string()
                                        }
                                    }),
                                );

                                if let Ok(mut transactions) = state.transactions.lock() {
                                    transactions.insert(0, transaction); // Insert at beginning for latest-first ordering
                                }
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
