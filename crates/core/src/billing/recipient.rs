use std::str::FromStr;

use moneymq_types::x402::{MixedAddress, Network};
use solana_keypair::{Pubkey, Signer};

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
            Recipient::MoneyMqManaged(managed) => managed.recipient_address().clone(),
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
        is_sandbox: bool,
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
                    let managed_address = MoneyMqManagedRecipient::generate(is_sandbox)
                        .await
                        .map_err(|e| RecipientError::ManagedAddressGenerationError(e))?;
                    Ok(Recipient::MoneyMqManaged(managed_address))
                }
            },
        }
    }
}

/// Represents a MoneyMQ-managed recipient
#[derive(Debug, Clone)]
pub enum MoneyMqManagedRecipient {
    Local(LocalManagedRecipient),
    Remote(RemoteManagedRecipient),
}

impl MoneyMqManagedRecipient {
    pub async fn generate(is_sandbox: bool) -> Result<Self, String> {
        match is_sandbox {
            true => {
                let local = LocalManagedRecipient::generate().await?;
                Ok(MoneyMqManagedRecipient::Local(local))
            }
            false => {
                // Placeholder for remote managed recipient generation
                Err("Remote managed recipient generation not implemented".to_string())
            }
        }
    }
    /// Returns the [MixedAddress] associated with the [MoneyMqManagedRecipient]
    pub fn recipient_address(&self) -> MixedAddress {
        match self {
            MoneyMqManagedRecipient::Local(local) => local.address.clone(),
            MoneyMqManagedRecipient::Remote(remote) => remote.recipient_address.clone(),
        }
    }
}

/// Represents a Local MoneyMQ-managed recipient address
#[derive(Debug, Clone)]
pub struct LocalManagedRecipient {
    /// The [MixedAddress] being managed by MoneyMQ
    pub address: MixedAddress,
    pub keypair_bytes: Vec<u8>,
}

impl LocalManagedRecipient {
    /// Generates a new MoneyMQ-managed recipient address
    pub async fn generate() -> Result<Self, String> {
        let keypair = solana_keypair::Keypair::new();
        let pubkey = keypair.pubkey();
        let keypair_bytes = keypair.to_bytes().to_vec();
        // Placeholder implementation - in real code, this would generate or fetch a managed address
        Ok(Self {
            address: MixedAddress::Solana(pubkey),
            keypair_bytes,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RemoteManagedRecipient {
    /// The [MixedAddress] being managed by MoneyMQ
    pub recipient_address: MixedAddress,
}

impl RemoteManagedRecipient {
    /// Placeholder for remote managed recipient creation
    pub async fn generate() -> Result<Self, String> {
        // Placeholder implementation - in real code, this would interact with a remote service
        Err("Remote managed recipient generation not implemented".to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientError {
    #[error("Failed to parse provided recipient address ({0}): {1}")]
    InvalidProvidedAddress(String, String),
    #[error("Failed to generate managed address: {0}")]
    ManagedAddressGenerationError(String),
}
