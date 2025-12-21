use std::fs;

use axum::{Json, extract::State, response::IntoResponse};
use moneymq_types::x402::{MixedAddress, Network, config::facilitator::ValidatorNetworkConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use solana_pubkey::Pubkey;

use crate::api::{catalog::CatalogState, payment::PaymentApiConfig};

/// Configuration response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402: Option<X402Config>,
}

/// X402 protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payout_account: Option<TokenAccountConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facilitator: Option<FacilitatorConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validator: Option<ValidatorNetworkConfig>,
}

/// Account configuration including branding
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_color: Option<String>,
}

/// Token account details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenAccountConfig {
    pub currency: String,
    pub decimals: u8,
    pub address: String,       // ATA address
    pub token_address: String, // Mint address
}

/// Operator account configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorAccountConfig {
    pub out: String, // Payer pubkey
    #[serde(rename = "in")]
    pub in_account: TokenAccountConfig,
}

/// Facilitator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilitatorConfig {
    pub operator_account: OperatorAccountConfig,
    pub url: String,
}

/// Config endpoint - returns provider configuration including branding and x402 settings
pub async fn get_config(
    State(catalog_state): State<CatalogState>,
    State(payment_api_config): State<PaymentApiConfig>,
) -> impl IntoResponse {
    let network = Network::Solana;
    // Build account configuration
    let mut account = AccountConfig {
        name: catalog_state.catalog_name.clone(),
        description: catalog_state.catalog_description.clone(),
        logo: None,
        primary_color: None,
        secondary_color: None,
    };

    // Load branding assets if provider name is available
    let assets_path = catalog_state.catalog_path.join("assets");

    // Load logo as base64
    let logo_path = assets_path.join("logo.png");
    if logo_path.exists() {
        if let Ok(logo_bytes) = fs::read(&logo_path) {
            let logo_base64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &logo_bytes);
            account.logo = Some(format!("data:image/png;base64,{}", logo_base64));
        }
    }

    // Load style.json
    let style_path = assets_path.join("style.json");
    if style_path.exists() {
        if let Ok(style_content) = fs::read_to_string(&style_path) {
            if let Ok(style_json) = serde_json::from_str::<Value>(&style_content) {
                if let Some(style_obj) = style_json.as_object() {
                    account.primary_color = style_obj
                        .get("primary_color")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    account.secondary_color = style_obj
                        .get("secondary_color")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
        }
    }

    // Build payout account configuration
    let payout_account = catalog_state
        .networks_config
        .configs
        .iter()
        .next()
        .and_then(|(_, network_config)| {
            let recipient_address = network_config.recipient().address();
            let currency = network_config.currencies().first()?;

            match (recipient_address, currency.address()) {
                (MixedAddress::Solana(owner), MixedAddress::Solana(mint)) => {
                    let ata =
                        spl_associated_token_account::get_associated_token_address(&owner, &mint);

                    // Get currency details
                    let solana_currency = currency.solana_currency()?;

                    Some(TokenAccountConfig {
                        currency: solana_currency.symbol.to_lowercase(),
                        decimals: solana_currency.decimals,
                        address: ata.to_string(),        // ATA address
                        token_address: mint.to_string(), // Mint address
                    })
                }
                _ => None,
            }
        });

    // Build facilitator configuration
    let facilitator = {
        // Parse the payer pubkey and compute its ATA
        let in_account = catalog_state
            .networks_config
            .configs
            .iter()
            .next()
            .and_then(|(_, network_config)| {
                let currency = network_config.currencies().first()?;

                // Parse the facilitator's pubkey (the payer)
                let facilitator_pubkey_str = payment_api_config
                    .facilitator_config
                    .get_facilitator_pubkey("solana")?;
                let payer_pubkey = facilitator_pubkey_str.parse::<Pubkey>().ok()?;

                match currency.address() {
                    MixedAddress::Solana(mint) => {
                        // Compute ATA for the PAYER (facilitator), not the recipient
                        let ata = spl_associated_token_account::get_associated_token_address(
                            &payer_pubkey,
                            &mint,
                        );

                        // Get currency details
                        let solana_currency = currency.solana_currency()?;

                        Some(TokenAccountConfig {
                            currency: solana_currency.symbol.to_lowercase(),
                            decimals: solana_currency.decimals,
                            address: ata.to_string(),
                            token_address: mint.to_string(),
                        })
                    }
                    _ => None,
                }
            });

        let facilitator_pubkey = payment_api_config
            .facilitator_config
            .get_facilitator_pubkey("solana");

        match (in_account, facilitator_pubkey) {
            (Some(in_acc), Some(pubkey)) => Some(FacilitatorConfig {
                operator_account: OperatorAccountConfig {
                    out: pubkey,
                    in_account: in_acc,
                },
                url: catalog_state.facilitator_url.to_string(),
            }),
            _ => None,
        }
    };

    let validator = payment_api_config
        .validators
        .networks
        .get(&network.to_string())
        .cloned();

    let x402 = X402Config {
        payout_account,
        facilitator,
        validator,
    };

    let config = ConfigResponse {
        account: Some(account),
        x402: Some(x402),
    };

    Json(config)
}
