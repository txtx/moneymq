use axum::{Extension, Json};
use serde_json::{Value, json};

use crate::{
    billing::recipient::{
        LocalManagedRecipient, MoneyMqManagedRecipient, Recipient, RemoteManagedRecipient,
    },
    facilitator::FacilitatorState,
};

/// GET /sandbox/accounts - List local network accounts
pub async fn list_accounts(
    Extension(state): Extension<Option<FacilitatorState>>,
) -> Result<Json<Value>, Json<Value>> {
    let Some(state) = state else {
        return Ok(Json(json!({})));
    };

    let networks_config = &state.networks_config;

    let mut res = json!({});
    for (network_name, config) in &networks_config.configs {
        let network = networks_config
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

    Ok(Json(res))
}
