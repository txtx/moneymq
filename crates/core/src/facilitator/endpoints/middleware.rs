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
use moneymq_types::x402::{PaymentPayload, PaymentRequirements};
use serde_json::json;

use crate::provider::ProviderState;

/// Extract and validate payment payload from request headers
async fn extract_payment_payload(
    headers: &HeaderMap,
    payment_requirements: &[PaymentRequirements],
) -> Result<PaymentPayload, (StatusCode, axum::Json<serde_json::Value>)> {
    // Check for X-Payment header
    let payment_header = headers.get("X-Payment");
    println!("=> {:?}", payment_header);

    match payment_header {
        None => {
            // No payment header - return 402 with requirements
            println!("\x1b[31m$ Required\x1b[0m - No X-Payment header found");
            let error_response = json!({
                "error": {
                    "code": "payment_required",
                    "message": "Payment required to access this resource. Please include X-Payment header.",
                    "type": "invalid_request_error",
                    "payment_requirements": payment_requirements
                }
            });
            Err((StatusCode::PAYMENT_REQUIRED, axum::Json(error_response)))
        }
        Some(payment_header) => {
            println!("\x1b[33m$ Verifying\x1b[0m - X-Payment header found");
            // Parse the payment payload
            let header_str = payment_header.to_str().map_err(|_| {
                let error_response = json!({
                    "error": {
                        "code": "invalid_payment_header",
                        "message": "X-Payment header contains invalid characters",
                        "type": "invalid_request_error"
                    }
                });
                (StatusCode::BAD_REQUEST, axum::Json(error_response))
            })?;

            // Decode base64 and parse JSON
            let decoded = BASE64.decode(header_str.as_bytes()).map_err(|_| {
                let error_response = json!({
                    "error": {
                        "code": "invalid_payment_header",
                        "message": "X-Payment header is not valid base64",
                        "type": "invalid_request_error"
                    }
                });
                (StatusCode::BAD_REQUEST, axum::Json(error_response))
            })?;

            let payment_payload: PaymentPayload =
                serde_json::from_slice(&decoded).map_err(|e| {
                    let error_response = json!({
                        "error": {
                            "code": "invalid_payment_payload",
                            "message": format!("Failed to parse payment payload: {}", e),
                            "type": "invalid_request_error"
                        }
                    });
                    (StatusCode::BAD_REQUEST, axum::Json(error_response))
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
) -> Result<moneymq_types::x402::MixedAddress, String> {
    use moneymq_types::x402::{VerifyRequest, VerifyResponse, X402Version};

    // Construct the verify request
    let verify_request = VerifyRequest {
        x402_version: X402Version::V1,
        payment_payload: payment_payload.clone(),
        payment_requirements: payment_requirements.clone(),
    };

    // Build the facilitator verify URL
    let verify_url = format!("{}verify", state.facilitator_config.url);

    // Make HTTP request to facilitator
    let client = reqwest::Client::new();
    let response = client
        .post(&verify_url)
        .json(&verify_request)
        .send()
        .await
        .map_err(|e| format!("Failed to contact facilitator: {}", e))?;

    // Check status code
    if !response.status().is_success() {
        return Err(format!(
            "Facilitator returned error status: {}",
            response.status()
        ));
    }

    // Parse response
    let verify_response: VerifyResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse facilitator response: {}", e))?;

    // Check if payment is valid
    match verify_response {
        VerifyResponse::Valid { payer } => Ok(payer),
        VerifyResponse::Invalid { reason, .. } => {
            Err(format!("Payment verification failed: {:?}", reason))
        }
    }
}

/// Settle payment with the facilitator by calling its /settle endpoint
async fn settle_payment_with_facilitator(
    state: &ProviderState,
    payment_payload: &PaymentPayload,
    payment_requirements: &PaymentRequirements,
) -> Result<(), String> {
    use moneymq_types::x402::{SettleRequest, SettleResponse, X402Version};

    // Construct the settle request (identical structure to verify request)
    let settle_request = SettleRequest {
        x402_version: X402Version::V1,
        payment_payload: payment_payload.clone(),
        payment_requirements: payment_requirements.clone(),
    };

    // Build the facilitator settle URL
    let settle_url = format!("{}settle", state.facilitator_config.url);

    // Make HTTP request to facilitator
    let client = reqwest::Client::new();
    let response = client
        .post(&settle_url)
        .json(&settle_request)
        .send()
        .await
        .map_err(|e| format!("Failed to contact facilitator: {}", e))?;

    // Check status code
    if !response.status().is_success() {
        return Err(format!(
            "Facilitator returned error status: {}",
            response.status()
        ));
    }

    // Parse response
    let settle_response: SettleResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse facilitator response: {}", e))?;

    // Check if settlement was successful
    if settle_response.success {
        if let Some(tx_hash) = settle_response.transaction {
            println!("  Transaction hash: {}", tx_hash);
        }
        Ok(())
    } else {
        Err(format!(
            "Payment settlement failed: {:?}",
            settle_response.error_reason
        ))
    }
}

/// Middleware to handle payment requirements for meter events
pub async fn payment_middleware(
    State(state): State<ProviderState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    use moneymq_types::x402::{MixedAddress, Network, Scheme, TokenAmount};
    use solana_keypair::Pubkey;
    use std::str::FromStr;

    // TODO: Get payment requirements from state/config
    // For now, create a mock payment requirement for testing

    // Determine network based on sandbox mode
    // let network = if state.use_sandbox {
    //     Network::SolanaSurfnet
    // } else {
    //     Network::SolanaMainnet
    // };

    // Get the facilitator's fee payer info for ATA creation
    // Try to find any Solana network config (could be under "solana", "solana-mainnet", etc.)
    let extra = state
        .facilitator_config
        .networks
        .values()
        .find_map(|config| {
            // Check if this is a Solana network
            match config {
                moneymq_types::x402::config::facilitator::FacilitatorNetworkConfig::SolanaMainnet(_)
                | moneymq_types::x402::config::facilitator::FacilitatorNetworkConfig::SolanaSurfnet(_) => {
                    // Get the extra field with fee_payer from the network config
                    config.extra().and_then(|e| serde_json::to_value(e).ok())
                }
            }
        });

    let payment_requirements: Vec<PaymentRequirements> = vec![PaymentRequirements {
        scheme: Scheme::Exact,
        network: Network::SolanaMainnet,
        max_amount_required: TokenAmount("1000000".to_string()), // 1 USDC (6 decimals)
        resource: state.facilitator_config.url.clone(),
        description: "Payment for meter event".to_string(),
        mime_type: "application/json".to_string(),
        output_schema: None,
        pay_to: MixedAddress::Solana(
            Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap(),
        ),
        max_timeout_seconds: 300,
        asset: MixedAddress::Solana(
            Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(),
        ),
        extra,
    }];

    let headers = req.headers();

    match extract_payment_payload(headers, &payment_requirements).await {
        Ok(payment_payload) => {
            // Payment header found and valid
            println!("\x1b[32m$ Received\x1b[0m - Valid payment payload received");
            println!(
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
                    println!("\x1b[32m$ Verified\x1b[0m - Payment verified by facilitator");
                    println!("  Payer: {:?}", payer);

                    // Store payer in request extensions for use by handler
                    req.extensions_mut().insert(payer.clone());

                    // Continue to the handler
                    let response = next.run(req).await;

                    // Post-process the response
                    if response.status().is_success() {
                        println!("\x1b[32m$ Success\x1b[0m - Request completed successfully");
                        println!("  Response status: {}", response.status());

                        // Call facilitator /settle endpoint to finalize payment
                        match settle_payment_with_facilitator(
                            &state,
                            &payment_payload,
                            &payment_requirements[0],
                        )
                        .await
                        {
                            Ok(()) => {
                                println!("\x1b[32m$ Settled\x1b[0m - Payment settled on-chain");
                            }
                            Err(error_message) => {
                                println!("\x1b[31m$ Settlement Failed\x1b[0m - {}", error_message);
                                // Note: We don't fail the request here since the service was already provided
                            }
                        }
                    }

                    response
                }
                Err(error_message) => {
                    println!("\x1b[31m$ Verification Failed\x1b[0m - {}", error_message);
                    let error_response = json!({
                        "error": {
                            "code": "payment_verification_failed",
                            "message": error_message,
                            "type": "invalid_request_error"
                        }
                    });
                    (StatusCode::PAYMENT_REQUIRED, axum::Json(error_response)).into_response()
                }
            }
        }
        Err((status, json_error)) => {
            // No payment or invalid payment
            (status, json_error).into_response()
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
