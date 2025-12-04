use moneymq_types::{RecurringScheme, x402::Currency};
use solana_keypair::Signer;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_system_interface::instruction::transfer;
use solana_transaction::versioned::VersionedTransaction;
use swig_sdk::{
    ClientRole, Ed25519ClientRole, Permission, RecurringConfig, SwigError, SwigWallet,
    types::UpdateAuthorityData,
};

pub const SLOTS_PER_WEEK: u64 = 1_512_000; // 7 days * 24 hours * 60 minutes * 60 seconds / 2.5 slots per second
pub const SLOTS_PER_YEAR: u64 = SLOTS_PER_WEEK * 52;
pub const SLOTS_PER_MONTH: u64 = SLOTS_PER_YEAR / 12;

#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("Wallet creation failed: {0}")]
    CreationFailed(SwigError),
    #[error("Invalid authority: {0}")]
    InvalidAuthority(String),
    #[error("Smart wallet instruction error: {0}")]
    SwigInstructionError(SwigError),
    #[error("Failed to sign transaction with smart wallet authority: {0}")]
    SwigPartialSignFailed(SwigError),
    #[error("RPC client error: {0}")]
    ClientError(#[from] solana_client::client_error::ClientError),
    #[error("Smart wallet error: {0}")]
    SwigError(#[from] SwigError),
}

pub type WalletResult<T> = Result<T, WalletError>;

pub struct MoneyMqWallet<'a> {
    swig: SwigWallet<'a>,
    fee_payer_pubkey: Pubkey,
    rpc_client: solana_client::rpc_client::RpcClient,
}

impl<'a> MoneyMqWallet<'a> {
    pub fn new_ed25519_wallet(
        swig_id: [u8; 32],
        authority: &'a dyn Signer,
        rpc_url: &'a str,
        fee_payer_pubkey: Pubkey,
    ) -> WalletResult<Self> {
        let authority_pubkey = authority.pubkey();
        let client_role = Box::new(Ed25519ClientRole::new(authority_pubkey)) as Box<dyn ClientRole>;

        let wallet = SwigWallet::new(
            swig_id,
            client_role,
            None,
            Some(fee_payer_pubkey),
            rpc_url.into(),
            Some(authority),
            authority,
        )
        .map_err(WalletError::CreationFailed)?;

        Ok(MoneyMqWallet {
            swig: wallet,
            fee_payer_pubkey,
            rpc_client: solana_client::rpc_client::RpcClient::new(rpc_url.to_string()),
        })
    }

    pub fn get_swig_wallet_address(&self) -> WalletResult<Pubkey> {
        Ok(self.swig.get_swig_wallet_address()?)
    }

    pub fn display_swig(&self) -> WalletResult<()> {
        Ok(self.swig.display_swig()?)
    }

    pub fn get_balance(&mut self) -> WalletResult<u64> {
        Ok(self.swig.get_balance()?)
    }

    pub fn get_sub_account(&mut self) -> WalletResult<Option<Pubkey>> {
        Ok(self.swig.get_sub_account()?)
    }

    pub fn get_create_sub_account_transaction(&mut self) -> WalletResult<VersionedTransaction> {
        let create_sub_account_ixs = self
            .swig
            .create_sub_account_instructions()
            .map_err(WalletError::SwigInstructionError)?;
        self.create_tx(create_sub_account_ixs)
    }

    pub fn get_transfer_tx(
        &mut self,
        from: &Pubkey,
        to: &Pubkey,
        lamports: u64,
    ) -> WalletResult<VersionedTransaction> {
        let transfer_tx = self.get_transaction(vec![transfer(from, to, lamports)])?;
        Ok(transfer_tx)
    }

    pub fn get_transfer_sub_account_tx(
        &mut self,
        from: &Pubkey,
        to: &Pubkey,
        lamports: u64,
    ) -> WalletResult<VersionedTransaction> {
        let transfer_tx =
            self.get_transaction_with_sub_account(vec![transfer(from, to, lamports)])?;
        Ok(transfer_tx)
    }

    pub fn get_transaction(
        &mut self,
        instructions: Vec<solana_transaction::Instruction>,
    ) -> WalletResult<VersionedTransaction> {
        let instructions = self
            .swig
            .get_sign_v2_instructions(instructions)
            .map_err(WalletError::SwigInstructionError)?;
        self.create_tx(instructions)
    }

    pub fn get_transaction_with_sub_account(
        &mut self,
        instructions: Vec<solana_transaction::Instruction>,
    ) -> WalletResult<VersionedTransaction> {
        let instructions = self
            .swig
            .get_sign_instructions_with_sub_account(instructions)
            .map_err(WalletError::SwigInstructionError)?;
        self.create_tx(instructions)
    }

    pub fn get_set_permissions_transaction(
        &mut self,
        supported_currencies: Vec<Currency>,
        recurring_scheme: RecurringScheme,
        allowed_amount: u64,
        destination_pubkey: Option<Pubkey>,
    ) -> WalletResult<VersionedTransaction> {
        let role_id = self.swig.get_current_role_id()?;

        let recurring = match recurring_scheme {
            RecurringScheme::PerUnit => None,
            RecurringScheme::Monthly(n) => Some(RecurringConfig::new(SLOTS_PER_MONTH * n as u64)),
            RecurringScheme::Weekly(n) => Some(RecurringConfig::new(SLOTS_PER_WEEK * n as u64)),
            RecurringScheme::Yearly(n) => Some(RecurringConfig::new(SLOTS_PER_YEAR * n as u64)),
        };

        let mut permissions = vec![
            Permission::ManageAuthority,
            Permission::Sol {
                amount: 100_000_000_000,
                recurring: None,
            },
            Permission::Program {
                program_id: solana_system_interface::program::id(),
            },
            Permission::SubAccount {
                sub_account: [0; 32], // Blank sub account - will be populated on creation
            },
        ];
        permissions.append(
            &mut supported_currencies
                .into_iter()
                .map(|currency| match currency {
                    Currency::Solana(solana_currency) => {
                        if let Some(destination_pubkey) = destination_pubkey {
                            Permission::TokenDestination {
                                mint: solana_currency.mint,
                                amount: allowed_amount,
                                recurring: recurring.clone(),
                                destination: destination_pubkey,
                            }
                        } else {
                            Permission::Token {
                                mint: solana_currency.mint,
                                amount: allowed_amount,
                                recurring: recurring.clone(),
                            }
                        }
                    }
                })
                .collect(),
        );
        let update_data = UpdateAuthorityData::ReplaceAll(permissions);

        let transaction = self
            .swig
            .get_update_authority_instructions(role_id, update_data)?;

        let signed_tx = self.create_tx(transaction)?;

        Ok(signed_tx)
    }

    /// Creates a transaction with the given instructions, partially signed by the wallet's authority.
    fn create_tx(
        &mut self,
        instructions: Vec<solana_transaction::Instruction>,
    ) -> WalletResult<VersionedTransaction> {
        let blockhash = self.rpc_client.get_latest_blockhash()?;
        let message =
            Message::new_with_blockhash(&instructions, Some(&self.fee_payer_pubkey), &blockhash);
        let required_sigs = message.header.num_required_signatures as usize;
        let mut transaction = VersionedTransaction {
            message: solana_message::VersionedMessage::Legacy(message),
            signatures: vec![solana_keypair::Signature::default(); required_sigs],
        };
        self.swig
            .partial_sign_transaction(&mut transaction)
            .map_err(WalletError::SwigPartialSignFailed)?;
        Ok(transaction)
    }
}

#[cfg(test)]
mod test;
