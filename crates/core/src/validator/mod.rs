use moneymq_types::x402::config::facilitator::FacilitatorRpcConfig;
use serde_json::Value;
use solana_client::{rpc_client::RpcClient, rpc_response::RpcVersionInfo};
use solana_pubkey::Pubkey;
#[cfg(feature = "embedded_validator")]
use surfpool_core::{rpc::minimal::SurfpoolRpcVersionInfo, surfnet::svm::SurfnetSvm};
#[cfg(feature = "embedded_validator")]
use surfpool_types::{RpcConfig, SimnetCommand, SimnetConfig, SimnetEvent, SurfpoolConfig};
use tracing::{error, info};

use crate::{
    billing::{SolanaSurfnetConfig, currency::SolanaCurrency},
    validator::surfnet_utils::{
        SetAccountRequest, SetTokenAccountRequest, surfnet_set_account, surfnet_set_token_account,
    },
};

pub mod surfnet_utils;
pub const DEFAULT_FACILITATOR_LAMPORTS: u64 = 1_000_000_000;

pub struct SolanaValidatorConfig {
    /// RPC API URL for the local Solana validator
    pub rpc_config: FacilitatorRpcConfig,
    /// Public key of the facilitator account used for funding and transactions
    pub facilitator_pubkey: Pubkey,
}

#[cfg(feature = "embedded_validator")]
/// Checks if a validator is running and if it is a surfnet
fn check_if_validator_running(rpc_client: &RpcClient) -> (bool, bool) {
    match rpc_client.send::<Value>(
        solana_client::rpc_request::RpcRequest::GetVersion,
        Value::Null,
    ) {
        Ok(version) => {
            let Ok(_) = serde_json::from_value::<SurfpoolRpcVersionInfo>(version.clone()) else {
                return match serde_json::from_value::<RpcVersionInfo>(version) {
                    Ok(_) => (true, false),
                    Err(_) => (false, false),
                };
            };
            (true, true)
        }
        Err(_) => (false, false),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SurfpoolError {
    #[error("Invalid network configuration: {0}")]
    InvalidNetworkConfig(String),
    #[error("Surfnet startup failed: {0}")]
    FailedStartup(String),
    #[error("Failed to spawn Surfnet thread: {0}")]
    SpawnSurfnetError(#[from] std::io::Error),
}

#[cfg(feature = "embedded_validator")]
pub fn start_surfpool(
    config: SolanaValidatorConfig,
    network_config: Option<&SolanaSurfnetConfig>,
) -> Result<Option<crossbeam::channel::Sender<SimnetCommand>>, SurfpoolError> {
    let host = config
        .rpc_config
        .bind_host
        .or_else(|| config.rpc_config.rpc_url.host_str().map(|s| s.to_string()))
        .ok_or(SurfpoolError::InvalidNetworkConfig(
            "invalid rpc url host".into(),
        ))?;
    let rpc_port = config
        .rpc_config
        .rpc_port
        .or_else(|| config.rpc_config.rpc_url.port())
        .ok_or(SurfpoolError::InvalidNetworkConfig(
            "invalid rpc url port".into(),
        ))?;
    let ws_port = config.rpc_config.ws_port.ok_or_else(|| {
        SurfpoolError::InvalidNetworkConfig("missing ws port in rpc config".into())
    })?;

    let rpc_client = RpcClient::new(config.rpc_config.rpc_url.as_str());

    let (validator_running, is_surfnet) = check_if_validator_running(&rpc_client);
    if validator_running {
        if is_surfnet {
            info!("Surfnet already running at {}", config.rpc_config.rpc_url);
            let _ = surfnet_set_account(
                &rpc_client,
                SetAccountRequest::new(config.facilitator_pubkey)
                    .lamports(DEFAULT_FACILITATOR_LAMPORTS),
            );
            fund_facilitator_accounts(&rpc_client, config.facilitator_pubkey, network_config);
            return Ok(None);
        } else {
            error!(
                "Local Solana validator was already running at {}, but it is not a surfnet. The accounts will not be funded.",
                config.rpc_config.rpc_url
            );
        }
        return Ok(None);
    }

    let (surfnet_svm, simnet_events_rx, geyser_events_rx) = SurfnetSvm::new();
    let (simnet_commands_tx, simnet_commands_rx) = crossbeam::channel::unbounded();
    let (subgraph_commands_tx, _) = crossbeam::channel::unbounded();
    let _ = surfnet_svm.simnet_events_tx.clone();

    let surfpool_config = SurfpoolConfig {
        simnets: vec![SimnetConfig {
            airdrop_addresses: vec![config.facilitator_pubkey],
            airdrop_token_amount: DEFAULT_FACILITATOR_LAMPORTS,
            offline_mode: false,
            ..Default::default()
        }],
        rpc: RpcConfig {
            bind_host: host,
            bind_port: rpc_port,
            ws_port,
        },
        ..Default::default()
    };

    let simnet_commands_tx_copy = simnet_commands_tx.clone();
    let svm_locker = surfpool_core::surfnet::locker::SurfnetSvmLocker::new(surfnet_svm);
    let svm_locker_clone = svm_locker.clone();

    let _handle = hiro_system_kit::thread_named("surfnet").spawn(move || {
        let future = surfpool_core::runloops::start_local_surfnet_runloop(
            svm_locker_clone,
            surfpool_config,
            subgraph_commands_tx,
            simnet_commands_tx_copy,
            simnet_commands_rx,
            geyser_events_rx,
        );
        if let Err(e) = hiro_system_kit::nestable_block_on(future) {
            error!("Surfnet exited with error: {e}");
        }
    })?;

    loop {
        match simnet_events_rx.recv() {
            Ok(SimnetEvent::Aborted(error)) => return Err(SurfpoolError::FailedStartup(error)),
            Ok(SimnetEvent::Shutdown) => {
                return Err(SurfpoolError::FailedStartup(
                    "Surfnet shut down during startup".into(),
                ));
            }
            Ok(SimnetEvent::Ready) => break,
            _other => continue,
        }
    }

    fund_facilitator_accounts(&rpc_client, config.facilitator_pubkey, network_config);

    Ok(Some(simnet_commands_tx))
}

#[cfg(feature = "embedded_validator")]
fn fund_facilitator_accounts(
    rpc_client: &RpcClient,
    facilitator_pubkey: Pubkey,
    network_config: Option<&SolanaSurfnetConfig>,
) {
    info!(
        "Funding token accounts for facilitator {} on local Solana validator",
        facilitator_pubkey
    );
    for currency in network_config
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
        info!("Funding currency {} (mint {})", symbol, mint);
        let _ = surfnet_set_token_account(
            &rpc_client,
            SetTokenAccountRequest::new(facilitator_pubkey.clone(), *mint, *token_program),
        );
    }
}
