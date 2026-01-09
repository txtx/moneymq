use std::sync::Arc;

use axum::{
    Extension,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, Network, SettleRequest, SettleResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use tracing::{error, info};

use crate::{
    api::payment::{
        PaymentApiConfig,
        endpoints::{
            channels::{ChannelEvent, PaymentFailedData, PaymentSettledData},
            serialize_to_base64,
        },
        networks,
    },
    events::{
        PaymentFlow, PaymentSettlementFailedData, PaymentSettlementSucceededData,
        create_payment_settlement_failed_event, create_payment_settlement_succeeded_event,
    },
};

/// POST /settle endpoint - settle a payment on-chain
pub async fn handler(
    Extension(state): Extension<PaymentApiConfig>,
    Json(request): Json<SettleRequest>,
) -> impl IntoResponse {
    info!(
        "Received settle request for network: {:?}",
        request.payment_requirements.network
    );

    // Verify network matches
    let Some(network_config) =
        state
            .facilitator_config
            .networks
            .iter()
            .find_map(|(_, network_config)| {
                network_config
                    .network()
                    .eq(&request.payment_requirements.network)
                    .then_some(network_config)
            })
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(SettleResponse {
                success: false,
                error_reason: Some(FacilitatorErrorReason::InvalidNetwork),
                payer: request.payment_requirements.pay_to.clone(),
                transaction: None,
                network: request.payment_requirements.network.clone(),
            }),
        );
    };

    // Verify payment payload network matches requirements
    if request.payment_payload.network != request.payment_requirements.network {
        return (
            StatusCode::BAD_REQUEST,
            Json(SettleResponse {
                success: false,
                error_reason: Some(FacilitatorErrorReason::InvalidNetwork),
                payer: request.payment_requirements.pay_to.clone(),
                transaction: None,
                network: request.payment_requirements.network.clone(),
            }),
        );
    }

    // Delegate to network-specific settlement
    let (status_code, response) = match network_config.network() {
        Network::Solana => {
            let rpc_client = Arc::new(RpcClient::new_with_commitment(
                network_config.rpc_url().to_string(),
                CommitmentConfig::confirmed(),
            ));
            match networks::solana::settle_solana_payment(
                &request,
                network_config,
                &rpc_client,
                &state.kora_config,
                &state.signer_pool,
            )
            .await
            {
                Ok(response) => (StatusCode::OK, response),
                Err(e) => {
                    error!("Settlement failed: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        SettleResponse {
                            success: false,
                            error_reason: Some(FacilitatorErrorReason::FreeForm(e.to_string())),
                            payer: request.payment_requirements.pay_to.clone(),
                            transaction: None,
                            network: request.payment_requirements.network.clone(),
                        },
                    )
                }
            }
        }
    };

    let settle_request_base64 = serialize_to_base64(&request);
    let settle_response_base64 = serialize_to_base64(&response);

    let status: String = if response.success {
        "completed".into()
    } else {
        "failed".into()
    };
    let signature = response
        .transaction
        .as_ref()
        .map(|tx_hash| tx_hash.to_string());

    // Extract transaction for payment_hash lookup
    let transaction_str = match &request.payment_payload.payload {
        moneymq_types::x402::ExactPaymentPayload::Solana(payload) => &payload.transaction,
    };

    // Find transaction by payment_hash for idempotent settlement updates
    match state
        .db_manager
        .find_transaction_id_by_payment_hash(transaction_str)
    {
        Ok(Some(tx_id)) => {
            if let Err(e) = state.db_manager.update_transaction_after_settlement(
                tx_id,
                Some(status.clone()),
                signature.clone(),
                Some(settle_request_base64),
                Some(settle_response_base64),
            ) {
                error!("Failed to update transaction after settlement: {}", e);
            }
        }
        Ok(None) => {
            // Check if transaction is already settled (idempotent behavior)
            match state
                .db_manager
                .is_transaction_already_settled(transaction_str)
            {
                Ok(true) => {
                    tracing::debug!(
                        "Transaction already settled for payment_hash (idempotent settle request)"
                    );
                }
                Ok(false) => {
                    error!("No matching transaction found to update after settlement");
                }
                Err(e) => {
                    error!("Error checking if transaction is settled: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Error finding transaction for settlement: {}", e);
        }
    }

    // Extract product_id and transaction_id from payment requirements extra metadata
    let product_id = request
        .payment_requirements
        .extra
        .as_ref()
        .and_then(|extra| extra.get("product"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Get transaction ID from extra context (same as verify)
    let transaction_id = request
        .payment_requirements
        .extra
        .as_ref()
        .and_then(|extra| extra.get("transactionId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Emit CloudEvent for settlement result (legacy event stream)
    if let Some(ref sender) = state.event_sender {
        let event = if response.success {
            let event_data = PaymentSettlementSucceededData {
                payer: response.payer.to_string(),
                amount: request.payment_requirements.max_amount_required.0.clone(),
                network: format!("{:?}", request.payment_requirements.network),
                transaction_signature: signature.clone(),
                product_id: product_id.clone(),
                payment_flow: PaymentFlow::X402,
                transaction_id: transaction_id.clone(),
            };
            create_payment_settlement_succeeded_event(event_data)
        } else {
            let event_data = PaymentSettlementFailedData {
                payer: Some(response.payer.to_string()),
                amount: request.payment_requirements.max_amount_required.0.clone(),
                network: format!("{:?}", request.payment_requirements.network),
                reason: response
                    .error_reason
                    .as_ref()
                    .map(|r| format!("{:?}", r))
                    .unwrap_or_else(|| "Unknown error".to_string()),
                product_id: product_id.clone(),
                payment_flow: PaymentFlow::X402,
            };
            create_payment_settlement_failed_event(event_data)
        };
        if let Err(e) = sender.send(event) {
            error!("Failed to send settlement event: {}", e);
        }
    }

    // Publish to channel (new channel-based event system)
    if let (Some(channel_manager), Some(tx_id)) = (&state.channel_manager, &transaction_id) {
        if response.success {
            // Extract currency and features from extra context
            let currency = request
                .payment_requirements
                .extra
                .as_ref()
                .and_then(|extra| extra.get("currency"))
                .and_then(|v| v.as_str())
                .unwrap_or("USDC")
                .to_string();

            // Emit payment:settled first - this notifies processors
            // The processor will send transaction:attach, which triggers transaction:completed with attachments
            let settled_event = ChannelEvent::payment_settled(PaymentSettledData {
                payer: response.payer.to_string(),
                amount: request.payment_requirements.max_amount_required.0.clone(),
                currency: currency.clone(),
                network: format!("{:?}", request.payment_requirements.network),
                transaction_signature: signature.clone(),
                product_id: product_id.clone(),
            });
            channel_manager.publish(tx_id, settled_event);
        } else {
            let channel_event = ChannelEvent::payment_failed(PaymentFailedData {
                payer: Some(response.payer.to_string()),
                amount: request.payment_requirements.max_amount_required.0.clone(),
                network: format!("{:?}", request.payment_requirements.network),
                reason: response.error_reason.as_ref().map(|r| format!("{:?}", r)),
                product_id: product_id.clone(),
            });
            channel_manager.publish(tx_id, channel_event);
        }
    }

    (status_code, Json(response))
}
