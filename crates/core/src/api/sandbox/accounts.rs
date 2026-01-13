//! Sandbox accounts endpoint

use axum::{Extension, Json};
use moneymq_types::{
    AccountRole, Keychain,
    x402::{
        LocalManagedRecipient, MixedAddress, MoneyMqManagedRecipient, Recipient,
        RemoteManagedRecipient,
    },
};
use serde_json::{Value, json};
use solana_keypair::Signer;
use solana_pubkey::Pubkey;

use crate::api::{payment::PaymentApiConfig, sandbox::NetworksConfig};

/// GET /sandbox/accounts - List local network accounts and operators
pub async fn list_accounts(
    Extension(payment_config): Extension<PaymentApiConfig>,
    Extension(networks_config): Extension<NetworksConfig>,
) -> Result<Json<Value>, Json<Value>> {
    let accounts_config = &payment_config.accounts;

    let mut res = json!({});

    // Add operator accounts from AccountsConfig
    let operators: Vec<Value> = accounts_config
        .iter()
        .filter_map(|(id, account)| {
            if let AccountRole::Operator(op) = &account.role {
                let secret = match &op.keychain {
                    Keychain::Base58(base58) => Some(base58.secret.clone()),
                    Keychain::Turnkey(_) => None,
                };

                // Derive address from secret key if available
                let address = secret.as_ref().map(|s| {
                    let keypair = solana_keypair::Keypair::from_base58_string(s);
                    keypair.pubkey().to_string()
                });

                Some(json!({
                    "id": id,
                    "name": account.name,
                    "address": address,
                    "secretKey": secret,
                    "role": "operator",
                }))
            } else {
                None
            }
        })
        .collect();

    res["operators"] = json!(operators);

    // Add network-specific user accounts (legacy format for backwards compatibility)
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
