//! Sandbox accounts endpoint

use axum::{Extension, Json};
use moneymq_types::x402::{
    LocalManagedRecipient, MixedAddress, MoneyMqManagedRecipient, Recipient, RemoteManagedRecipient,
};
use serde_json::{Value, json};
use solana_pubkey::Pubkey;

use crate::api::catalog::CatalogState;

/// GET /sandbox/accounts - List local network accounts
pub async fn list_accounts(
    Extension(state): Extension<CatalogState>,
) -> Result<Json<Value>, Json<Value>> {
    let networks_config = &state.networks_config;

    let mut res = json!({});
    for (network_name, config) in &networks_config.configs {
        let network = networks_config
            .get_network_for_name(network_name)
            .expect("expected network to be configured");
        let address = config.recipient().address();

        // Get USDC mint address if available
        let usdc_mint = config
            .currencies()
            .iter()
            .find(|c| {
                if let Some(solana_currency) = c.solana_currency() {
                    solana_currency.symbol.to_lowercase() == "usdc"
                } else {
                    false
                }
            })
            .and_then(|c| match c.address() {
                MixedAddress::Solana(mint) => Some(mint),
                MixedAddress::Offchain(_) => None,
            });

        let user_addresses = config
            .user_accounts()
            .iter()
            .map(|r| {
                let mut account_json = match r {
                    Recipient::UserManaged(addr) => json!({
                        "address": addr,
                    }),
                    Recipient::MoneyMqManaged(MoneyMqManagedRecipient::Remote(
                        RemoteManagedRecipient { recipient_address },
                    )) => json!({
                        "address": recipient_address,
                    }),
                    Recipient::MoneyMqManaged(MoneyMqManagedRecipient::Local(
                        LocalManagedRecipient {
                            address,
                            keypair_bytes,
                            label,
                        },
                    )) => json!({
                        "address": address,
                        "secretKeyHex": bs58::encode(keypair_bytes).into_string(),
                        "label": label,
                    }),
                };

                // Add stablecoins with USDC ATA if we have a USDC mint
                if let Some(mint) = usdc_mint
                    && let Ok(owner_pubkey) = r.address().to_string().parse::<Pubkey>()
                {
                    let usdc_ata = spl_associated_token_account::get_associated_token_address(
                        &owner_pubkey,
                        &mint,
                    );
                    account_json["stablecoins"] = json!({
                        "usdc": usdc_ata.to_string(),
                    });
                }

                account_json
            })
            .collect::<Vec<_>>();

        res[network_name] = json!({
            "network": network,
            "payTo": address,
            "userAccounts": user_addresses,
        });
    }

    Ok(Json(res))
}
