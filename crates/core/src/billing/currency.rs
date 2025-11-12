use moneymq_types::x402::MixedAddress;
use solana_pubkey::Pubkey;

/// Represents a currency used for billing across different blockchains
#[derive(Debug, Clone)]
pub enum Currency {
    /// Represents a currency on the Solana blockchain
    Solana(SolanaCurrency),
}

impl Currency {
    pub async fn from_symbol_and_network(
        symbol: &str,
        network: &moneymq_types::x402::Network,
    ) -> Result<Self, String> {
        match network {
            moneymq_types::x402::Network::Solana => {
                let solana_currency = SolanaCurrency::from_symbol(symbol).await?;
                Ok(Currency::Solana(solana_currency))
            }
        }
    }

    pub fn address(&self) -> MixedAddress {
        match self {
            Currency::Solana(solana_currency) => solana_currency.mixed_address(),
        }
    }

    pub fn decimals(&self) -> u8 {
        match self {
            Currency::Solana(solana_currency) => solana_currency.decimals,
        }
    }

    pub fn solana_currency(&self) -> Option<&SolanaCurrency> {
        match self {
            Currency::Solana(solana_currency) => Some(solana_currency),
        }
    }
}

/// Represents a currency on the Solana blockchain for billing purposes
#[derive(Debug, Clone)]
pub struct SolanaCurrency {
    /// The symbol of the currency (e.g., "USDC")
    pub symbol: String,
    /// The mint address of the currency
    pub mint: Pubkey,
    /// The token program associated with the currency
    pub token_program: Pubkey,
    /// Number of decimal places for the currency
    pub decimals: u8,
}

impl SolanaCurrency {
    pub async fn from_symbol(symbol: &str) -> Result<Self, String> {
        // TODO Placeholder implementation - in real code, this would look up the mint and token program
        Ok(SolanaCurrency {
            symbol: symbol.to_string(),
            mint: Pubkey::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
            token_program: spl_token_interface::id(),
            decimals: 6,
        })
    }

    pub fn mixed_address(&self) -> MixedAddress {
        MixedAddress::Solana(self.mint)
    }
}
