use axum::{Json, extract::State, response::IntoResponse};
use serde_json::json;

use crate::{
    billing::recipient::{
        LocalManagedRecipient, MoneyMqManagedRecipient, Recipient, RemoteManagedRecipient,
    },
    provider::ProviderState,
};

/// GET /v1/accounts - List local network accounts
pub async fn list_accounts(State(state): State<ProviderState>) -> impl IntoResponse {
    let billing_manager = &state.billing_manager;

    let mut res = json!({});
    for (network_name, config) in &billing_manager.configs {
        let network = billing_manager
            .get_network_for_name(&network_name)
            .expect("expected network to be configured");
        let address = config.recipient().address();
        let user_addresses = config
            .user_accounts()
            .iter()
            .map(|r| match r {
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
            })
            .collect::<Vec<_>>();
        res[network_name] = json!({
            "network": network,
            "payTo": address,
            "userAccounts": user_addresses,
        });
    }

    Json(res)
}
