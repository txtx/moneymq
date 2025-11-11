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

    /// Returns the label associated with the [Recipient], if any
    pub fn label(&self) -> Option<String> {
        match self {
            Recipient::UserManaged(_) => None,
            Recipient::MoneyMqManaged(managed) => managed.label(),
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
    pub async fn instantiate_with_index(
        network: &Network,
        recipient_str: Option<&String>,
        is_sandbox: bool,
        index: Option<usize>,
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
                    let managed_address =
                        MoneyMqManagedRecipient::generate_with_index(is_sandbox, index)
                            .await
                            .map_err(RecipientError::ManagedAddressGenerationError)?;
                    Ok(Recipient::MoneyMqManaged(managed_address))
                }
            },
        }
    }

    pub async fn instantiate(
        network: &Network,
        recipient_str: Option<&String>,
        is_sandbox: bool,
    ) -> Result<Recipient, RecipientError> {
        Self::instantiate_with_index(network, recipient_str, is_sandbox, None).await
    }

    pub async fn instantiate_payment_recipient(
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
                    let managed_address =
                        MoneyMqManagedRecipient::generate_payment_recipient(is_sandbox)
                            .await
                            .map_err(RecipientError::ManagedAddressGenerationError)?;
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
    pub async fn generate_with_index(
        is_sandbox: bool,
        index: Option<usize>,
    ) -> Result<Self, String> {
        match is_sandbox {
            true => {
                let local = LocalManagedRecipient::generate_with_index(index).await?;
                Ok(MoneyMqManagedRecipient::Local(local))
            }
            false => {
                // Placeholder for remote managed recipient generation
                Err("Remote managed recipient generation not implemented".to_string())
            }
        }
    }

    pub async fn generate(is_sandbox: bool) -> Result<Self, String> {
        Self::generate_with_index(is_sandbox, None).await
    }

    pub async fn generate_payment_recipient(is_sandbox: bool) -> Result<Self, String> {
        match is_sandbox {
            true => {
                let local = LocalManagedRecipient::generate_payment_recipient().await?;
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

    /// Returns the label associated with the [MoneyMqManagedRecipient], if any
    pub fn label(&self) -> Option<String> {
        match self {
            MoneyMqManagedRecipient::Local(local) => local.label.clone(),
            MoneyMqManagedRecipient::Remote(_) => None,
        }
    }
}

/// Represents a Local MoneyMQ-managed recipient address
#[derive(Debug, Clone)]
pub struct LocalManagedRecipient {
    /// The [MixedAddress] being managed by MoneyMQ
    pub address: MixedAddress,
    pub keypair_bytes: [u8; 64],
    pub label: Option<String>,
}

impl LocalManagedRecipient {
    /// Generates a new MoneyMQ-managed recipient address
    /// If `index` is provided, generates a deterministic keypair with a label
    pub async fn generate_with_index(index: Option<usize>) -> Result<Self, String> {
        let (keypair, label) = match index {
            Some(i) => {
                // Deterministic keypair generation using a seed derived from index
                let seed = Self::generate_seed_for_index(i);
                let keypair = solana_keypair::Keypair::new_from_array(seed);
                let label = Self::label_for_index(i);
                (keypair, Some(label))
            }
            None => {
                // Random keypair for backward compatibility
                (solana_keypair::Keypair::new(), None)
            }
        };

        let pubkey = keypair.pubkey();
        let keypair_bytes = keypair.to_bytes();

        Ok(Self {
            address: MixedAddress::Solana(pubkey),
            keypair_bytes,
            label,
        })
    }

    /// Legacy method for backward compatibility
    pub async fn generate() -> Result<Self, String> {
        Self::generate_with_index(None).await
    }

    /// Generate a deterministic payment recipient
    pub async fn generate_payment_recipient() -> Result<Self, String> {
        let seed = Self::generate_seed_for_payment_recipient();
        let keypair = solana_keypair::Keypair::new_from_array(seed);
        let pubkey = keypair.pubkey();
        let keypair_bytes = keypair.to_bytes();

        Ok(Self {
            address: MixedAddress::Solana(pubkey),
            keypair_bytes,
            label: Some("provider".to_string()),
        })
    }

    /// Generate a deterministic seed for a given index
    fn generate_seed_for_index(index: usize) -> [u8; 32] {
        Self::generate_seed_with_prefix(b"moneymq-user-account", index)
    }

    /// Generate a deterministic seed for the payment recipient
    fn generate_seed_for_payment_recipient() -> [u8; 32] {
        Self::generate_seed_with_prefix(b"moneymq-payment-recipient", 0)
    }

    /// Generate a deterministic seed with a custom prefix
    fn generate_seed_with_prefix(prefix: &[u8], index: usize) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(prefix);
        hasher.update(index.to_le_bytes());
        let result = hasher.finalize();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&result);
        seed
    }

    /// Get a label for a given index (alice, bob, charlie, etc.)
    fn label_for_index(index: usize) -> String {
        const LABELS: &[&str] = &[
            "alice", "bob", "charlie", "david", "eve", "frank", "grace", "heidi", "ivan", "judy",
            "kevin", "laura", "michael", "nancy", "oscar", "peggy", "quinn", "rachel", "steve",
            "trent", "ursula", "victor", "wendy", "xavier", "yvonne", "zach",
        ];

        if index < LABELS.len() {
            LABELS[index].to_string()
        } else {
            format!("user_{}", index)
        }
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
