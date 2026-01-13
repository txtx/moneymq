use axum::{Extension, Json, extract::Query, response::IntoResponse};
use moneymq_types::x402::USDC_MINT;
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

use crate::api::payment::PaymentApiConfig;

/// Payout configuration for x402 payments
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayoutConfig {
    /// Recipient address for payments (wallet address)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_address: Option<String>,
    /// Recipient's USDC token account address (ATA)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_token_account: Option<String>,
    /// Token mint address (e.g., USDC)
    pub token_address: String,
}

/// Facilitator configuration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilitatorConfig {
    /// Facilitator address (fee payer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

/// Solana network configuration for x402
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolanaX402Config {
    /// Payout configuration
    pub payout: PayoutConfig,
    /// Facilitator configuration
    pub facilitator: FacilitatorConfig,
}

/// x402 protocol configuration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X402Config {
    /// Solana network configuration
    pub solana: SolanaX402Config,
}

/// Stack (merchant/store) configuration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackConfig {
    /// Stack/store name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Stack/store image URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

/// Studio configuration (only included when attrs=studio)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioConfig {
    /// RPC endpoint URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_url: Option<String>,
    /// WebSocket endpoint URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_url: Option<String>,
}

/// Response for the /config endpoint
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    /// Whether the facilitator is running in sandbox mode
    pub is_sandbox: bool,
    /// x402 protocol configuration
    pub x402: X402Config,
    /// Stack (merchant) configuration
    pub stack: StackConfig,
    /// Studio configuration (only present if ?attrs=studio)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub studio: Option<StudioConfig>,
}

/// Query parameters for /config endpoint
#[derive(Debug, Deserialize)]
pub struct ConfigQuery {
    /// Comma-separated list of additional attributes to include
    #[serde(default)]
    pub attrs: Option<String>,
}

/// GET /config endpoint - returns facilitator configuration
pub async fn handler(
    Extension(state): Extension<PaymentApiConfig>,
    Query(query): Query<ConfigQuery>,
) -> impl IntoResponse {
    // Check if studio attrs requested
    let include_studio = query
        .attrs
        .as_ref()
        .map(|a| a.split(',').any(|attr| attr.trim() == "studio"))
        .unwrap_or(false);

    // Get network config for RPC/WS URLs
    let network_config = state.facilitator_config.networks.values().next();
    let rpc_url = network_config.map(|n| n.rpc_url().to_string());

    // Build WebSocket URL from RPC URL (replace http with ws, adjust port if needed)
    let ws_url = rpc_url.as_ref().map(|url| {
        url.replace("http://", "ws://")
            .replace("https://", "wss://")
    });

    // Get payout recipient address from state if available
    let recipient_address = state.payout_recipient_address.clone();
    let facilitator_address = state.facilitator_address.clone();

    // Compute the recipient's USDC token account (ATA) if recipient address is available
    let recipient_token_account = recipient_address.as_ref().and_then(|addr| {
        addr.parse::<Pubkey>().ok().map(|owner_pubkey| {
            spl_associated_token_account::get_associated_token_address(&owner_pubkey, &USDC_MINT)
                .to_string()
        })
    });

    Json(ConfigResponse {
        is_sandbox: state.is_sandbox,
        x402: X402Config {
            solana: SolanaX402Config {
                payout: PayoutConfig {
                    recipient_address,
                    recipient_token_account,
                    token_address: USDC_MINT.to_string(),
                },
                facilitator: FacilitatorConfig {
                    address: facilitator_address,
                },
            },
        },
        stack: StackConfig {
            name: state.stack_name.clone(),
            image_url: state.stack_image_url.clone(),
        },
        studio: if include_studio {
            Some(StudioConfig { rpc_url, ws_url })
        } else {
            None
        },
    })
}
