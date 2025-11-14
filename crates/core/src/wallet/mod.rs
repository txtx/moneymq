use solana_keypair::Keypair;
use swig_sdk::{ClientRole, SwigWallet};

#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("Wallet creation failed: {0}")]
    CreationFailed(#[from] SwigError),
    #[error("Invalid authority: {0}")]
    InvalidAuthority(String),
}

pub type WalletResult<T> = Result<T, WalletError>;

pub fn create_swig_wallet(
    swig_id: [u8; 32],
    authority: String,
    authority_kp: String,
    rpc_url: &str,
) -> WalletResult<SwigWallet> {
    let (client_role, fee_payer) = {
        let authority_kp = {
            let mut buf = [0u8; 64];
            bs58::decode(&authority_kp).onto(&mut buf).map_err(|e| {
                WalletError::InvalidAuthority(format!("Failed to decode authority keypair: {}", e))
            })?;
            Keypair::try_from(&buf[..]).map_err(|e| {
                WalletError::InvalidAuthority(format!(
                    "Failed to create keypair from decoded bytes: {}",
                    e
                ))
            })?
        };
        let authority = Pubkey::from_str(&authority)?;

        (
            Box::new(Ed25519ClientRole::new(authority)) as Box<dyn ClientRole>,
            authority_kp,
        )
    };

    // Use Box::leak to create static references (similar to interactive mode)
    let fee_payer_static: &mut Keypair = Box::leak(Box::new(fee_payer));
    let authority_keypair_static: &mut Keypair =
        Box::leak(Box::new(fee_payer_static.insecure_clone()));
    let wallet = SwigWallet::new(
        swig_id,
        client_role,
        fee_payer_static,
        rpc_url.into(),
        Some(authority_keypair_static),
    )?;
    Ok(wallet)
}
