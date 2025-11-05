use std::str::FromStr;

use moneymq_types::x402::{MixedAddress, Network};
use solana_keypair::Pubkey;

#[derive(Debug, Clone)]
pub enum Recipient {
    UserManaged(MixedAddress),
    MoneyMqManaged(MixedAddress),
}

impl Recipient {
    pub fn address(&self) -> MixedAddress {
        match self {
            Recipient::UserManaged(addr) => addr.clone(),
            Recipient::MoneyMqManaged(addr) => addr.clone(),
        }
    }
}

impl Recipient {
    pub async fn instantiate(
        network: &Network,
        recipient_str: Option<&String>,
    ) -> Result<Recipient, RecipientError> {
        match recipient_str {
            Some(address_str) => {
                let mixed_address = match network {
                    Network::Solana => {
                        let pubkey = Pubkey::from_str(&address_str).map_err(|e| {
                            RecipientError::InvalidProvidedAddress(
                                address_str.to_string(),
                                e.to_string(),
                            )
                        })?;
                        MixedAddress::Solana(pubkey)
                    }
                };
                Ok(Recipient::UserManaged(mixed_address))
            }
            None => match network {
                Network::Solana => {
                    let managed_address = MoneyMqManagedRecipient::generate()
                        .await
                        .map_err(|e| RecipientError::ManagedAddressGenerationError(e))?;
                    Ok(Recipient::MoneyMqManaged(managed_address))
                }
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct MoneyMqManagedRecipient {
    pub recipient_address: MixedAddress,
    // Todo: additional fields as needed
}

impl MoneyMqManagedRecipient {
    pub async fn generate() -> Result<MixedAddress, String> {
        // Placeholder implementation - in real code, this would generate or fetch a managed address
        Ok(MixedAddress::Solana(Pubkey::new_unique()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientError {
    #[error("Failed to parse provided recipient address ({0}): {1}")]
    InvalidProvidedAddress(String, String),

    #[error("Failed to generate managed address: {0}")]
    ManagedAddressGenerationError(String),
}
