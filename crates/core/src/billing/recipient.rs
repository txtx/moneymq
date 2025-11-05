use std::str::FromStr;

use moneymq_types::x402::{MixedAddress, Network};
use solana_keypair::Pubkey;

/// Represents a billing recipient, which can be either user-managed or MoneyMQ-managed
#[derive(Debug, Clone)]
pub enum Recipient {
    /// Represents a user-managed recipient address, where the user has provided the address
    UserManaged(MixedAddress),
    /// Represents a MoneyMQ-managed recipient address, where MoneyMQ generates and manages the address
    MoneyMqManaged(MoneyMqManagedRecipient),
}

impl Recipient {
    /// Returns the [MixedAddress] associated with the [Recipient]
    pub fn address(&self) -> MixedAddress {
        match self {
            Recipient::UserManaged(addr) => addr.clone(),
            Recipient::MoneyMqManaged(managed) => managed.recipient_address.clone(),
        }
    }
    pub fn is_managed(&self) -> bool {
        matches!(self, Recipient::MoneyMqManaged(_))
    }
}

impl Recipient {
    /// Instantiates a [Recipient] based on the provided network and optional recipient string
    /// If `recipient_str` is provided, it creates a `UserManaged` recipient.
    /// If not provided, it generates a `MoneyMqManaged` recipient.
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

/// Represents a MoneyMQ-managed recipient address
#[derive(Debug, Clone)]
pub struct MoneyMqManagedRecipient {
    /// The [MixedAddress] being managed by MoneyMQ
    pub recipient_address: MixedAddress,
    // Todo: additional fields as needed
}

impl MoneyMqManagedRecipient {
    /// Generates a new MoneyMQ-managed recipient address
    pub async fn generate() -> Result<Self, String> {
        // Placeholder implementation - in real code, this would generate or fetch a managed address
        Ok(Self {
            recipient_address: MixedAddress::Solana(Pubkey::new_unique()),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientError {
    #[error("Failed to parse provided recipient address ({0}): {1}")]
    InvalidProvidedAddress(String, String),
    #[error("Failed to generate managed address: {0}")]
    ManagedAddressGenerationError(String),
}
