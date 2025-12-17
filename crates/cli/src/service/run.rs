use indexmap::IndexMap;
use moneymq_core::api::{NetworksConfig, payment::FacilitatorState};
// TODO: Re-enable when refactoring X402 facilitator
// use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::x402::MoneyMqNetwork;
use moneymq_types::x402::config::facilitator::ValidatorsConfig;
use url::Url;

// use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};
use crate::{
    manifest::{Manifest, x402::PaymentConfig},
    service::{BillingNetworksMap, ServiceCommand},
};

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct RunCommand {
    /// Port to run the server on
    #[arg(long, default_value = "8488")]
    pub port: u16,
}

impl ServiceCommand for RunCommand {
    const SANDBOX: bool = false;
    fn port(&self) -> u16 {
        self.port
    }

    fn billing_networks(
        &self,
        manifest: &Manifest,
    ) -> Result<BillingNetworksMap, super::RunCommandError> {
        let billing_networks = manifest
            .payments
            .iter()
            .flat_map(|(_name, payment_config)| match payment_config {
                PaymentConfig::X402(x402_config) => {
                    // Get networks from accepted config
                    x402_config
                        .accepted
                        .iter()
                        .map(|(network_id, network)| {
                            (
                                network_id.to_string(),
                                (
                                    MoneyMqNetwork::SolanaSurfnet,
                                    network.recipient.clone(),
                                    network.currencies.clone(),
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                }
            })
            .collect::<IndexMap<_, _>>();

        if billing_networks.is_empty() {
            Err(super::RunCommandError::NoBillingNetworksConfigured)
        } else {
            Ok(billing_networks)
        }
    }

    fn networks_config(
        &self,
        _billing_networks: BillingNetworksMap,
    ) -> Result<NetworksConfig, super::RunCommandError> {
        todo!()
    }

    async fn setup_facilitator(
        &self,
        _payments: &IndexMap<String, PaymentConfig>,
        _networks_config: &NetworksConfig,
    ) -> Result<(Url, String, ValidatorsConfig, Option<FacilitatorState>), super::RunCommandError>
    {
        // In non-sandbox mode, use external facilitator (no local FacilitatorState)
        todo!("Non-sandbox mode not yet implemented")
    }
}
