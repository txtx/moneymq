use serde_json::json;
use solana_client::{rpc_client::RpcClient, rpc_request::RpcRequest};
use solana_keypair::Pubkey;
use tracing::debug;

/// Sets account data for a given account on the Surfnet local validator
pub fn surfnet_set_account(rpc_client: &RpcClient, pubkey: &Pubkey) -> Result<(), String> {
    let account_data = json!({
        "lamports": 1_000_000_000,
    });
    let params = json!([pubkey.to_string(), account_data,]);

    let _ = rpc_client
        .send::<serde_json::Value>(
            RpcRequest::Custom {
                method: "surfnet_setAccount",
            },
            params,
        )
        .map_err(|e| format!("Failed to set account data for {}: {}", pubkey, e))?;
    Ok(())
}

/// Sets token account data for a given account on the Surfnet local validator
pub fn surfnet_set_token_account(
    rpc_client: &RpcClient,
    pubkey: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> Result<(), String> {
    let account_data = json!({
        "amount": 1_000_000_000,
    });
    let params = json!([
        pubkey.to_string(),
        mint.to_string(),
        account_data,
        token_program.to_string()
    ]);

    debug!("Setting token account with payload: {:?}", params);

    let _ = rpc_client
        .send::<serde_json::Value>(
            RpcRequest::Custom {
                method: "surfnet_setTokenAccount",
            },
            params,
        )
        .map_err(|e| {
            format!(
                "Failed to set token account data for {} with mint {}: {}",
                pubkey, mint, e
            )
        })?;
    Ok(())
}
