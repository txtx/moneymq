use std::process::{Child, Stdio};

use serde_json::json;
use solana_client::{rpc_client::RpcClient, rpc_request::RpcRequest};
use url::Url;

pub struct SolanaValidatorConfig {
    /// RPC API URL for the local Solana validator
    pub rpc_api_url: Url,

    pub facilitator_pubkey: String,
}

fn check_if_validator_running(rpc_client: &RpcClient) -> bool {
    match rpc_client.get_health() {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub fn start_local_solana_validator(
    config: SolanaValidatorConfig,
) -> Result<Option<Child>, Box<dyn std::error::Error>> {
    let rpc_url = config.rpc_api_url.clone();

    let rpc_client = RpcClient::new(rpc_url.as_str());

    // Check if validator is already running at the rpc url
    if check_if_validator_running(&rpc_client) {
        return Ok(None);
    }

    let host = rpc_url
        .host_str()
        .ok_or_else(|| format!("Invalid RPC URL, missing host: {}", config.rpc_api_url))?;
    let port = rpc_url
        .port()
        .ok_or_else(|| format!("Invalid RPC URL, missing port: {}", config.rpc_api_url))?;

    let test_validator_stdout = Stdio::null();
    let test_validator_stderr = Stdio::null();

    let flags = vec![
        "--no-tui".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--host".to_string(),
        host.to_string(),
        "--airdrop".to_string(),
        config.facilitator_pubkey.clone(),
    ];

    let mut validator_handle = std::process::Command::new("surfpool")
        .arg("start")
        .args(flags)
        .stdout(test_validator_stdout)
        .stderr(test_validator_stderr)
        .spawn()
        .map_err(|e| format!("Failed to spawn `surfpool`: {e}"))?;

    let ms_wait = 5000;
    let mut count = 0;
    while count < ms_wait {
        if check_if_validator_running(&rpc_client) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        count += 500;
    }

    if count >= ms_wait {
        validator_handle.kill().ok();
        return Err(format!(
            "Timed out waiting for local Solana validator to start at {}",
            config.rpc_api_url
        )
        .into());
    }

    // Todo: Set up token account for facilitator
    let _: serde_json::Value = rpc_client
        .send(
            RpcRequest::Custom {
                method: "surfnet_setTokenAccount",
            },
            json!([
                config.facilitator_pubkey,
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                {
                    "amount": 1000000000
                }
            ]),
        )
        .unwrap_or_default();

    Ok(Some(validator_handle))
}
