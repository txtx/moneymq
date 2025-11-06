use std::process::{Child, Stdio};

use solana_client::rpc_client::RpcClient;
use solana_keypair::Pubkey;
use tracing::info;
use url::Url;

use crate::{
    billing::{SolanaSurfnetBillingConfig, currency::SolanaCurrency},
    validator::surfnet_utils::surfnet_set_token_account,
};

pub mod surfnet_utils;

pub struct SolanaValidatorConfig {
    /// RPC API URL for the local Solana validator
    pub rpc_api_url: Url,
    /// Public key of the facilitator account used for funding and transactions
    pub facilitator_pubkey: Pubkey,
}

fn check_if_validator_running(rpc_client: &RpcClient) -> bool {
    match rpc_client.get_health() {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub fn start_local_solana_validator(
    config: SolanaValidatorConfig,
    billing_config: Option<&SolanaSurfnetBillingConfig>,
) -> Result<Option<Child>, Box<dyn std::error::Error>> {
    let rpc_url = config.rpc_api_url.clone();

    let rpc_client = RpcClient::new(rpc_url.as_str());

    // Check if validator is already running at the rpc url
    if check_if_validator_running(&rpc_client) {
        info!(
            "Local Solana validator already running at {}",
            config.rpc_api_url
        );
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
        config.facilitator_pubkey.to_string(),
        "--log-level".to_string(),
        "debug".to_string(),
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

    info!("Local Solana validator started at {}", config.rpc_api_url);

    for currency in billing_config
        .map(|c| c.currencies.iter())
        .unwrap_or_default()
    {
        let Some(SolanaCurrency {
            symbol,
            mint,
            token_program,
            ..
        }) = currency.solana_currency()
        else {
            continue;
        };
        info!(
            "Setting up token account for currency {} with mint {}",
            symbol, mint
        );
        let _ =
            surfnet_set_token_account(&rpc_client, &config.facilitator_pubkey, mint, token_program);
    }

    info!(
        "Set up token account for facilitator {} on local Solana validator",
        config.facilitator_pubkey
    );

    Ok(Some(validator_handle))
}
