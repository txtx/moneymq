//! Payment configuration types for MoneyMQ manifest.
//!
//! This module defines what payments your application accepts.
//! It specifies the blockchain network (chain) and which stablecoins
//! are accepted for payment.
//!
//! # Overview
//!
//! The payments configuration is separate from environment configuration:
//!
//! - **Payments** (`payments:`) - *What* to accept (chain, currencies)
//! - **Environments** (`environments:`) - *How* to deploy (sandbox, production)
//!
//! This separation allows you to define payment acceptance once and
//! deploy it across multiple environments.
//!
//! # Example
//!
//! ```yaml
//! payments:
//!   networks:
//!     chain: Solana
//!     stablecoins:
//!       - USDC
//! ```
//!
//! # Supported Stablecoins
//!
//! On Solana, the following stablecoins are supported:
//!
//! | Symbol | Name | Mint Address |
//! |--------|------|--------------|
//! | USDC | USD Coin | `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v` |

use serde::{Deserialize, Serialize};

use crate::manifest::environments::Chain;

/// Payment acceptance configuration.
///
/// Defines what payments your application accepts, including the
/// blockchain network and which stablecoins are supported.
///
/// This is separate from [`EnvironmentConfig`](super::EnvironmentConfig)
/// which defines *how* to deploy, while `PaymentsConfig` defines
/// *what* to accept.
///
/// # Example
///
/// ```yaml
/// payments:
///   networks:
///     chain: Solana
///     stablecoins:
///       - USDC
/// ```
///
/// # Default
///
/// By default, accepts USDC on Solana:
///
/// ```yaml
/// payments:
///   networks:
///     chain: Solana
///     stablecoins:
///       - USDC
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaymentsConfig {
    /// Network and currency configuration.
    ///
    /// Specifies which blockchain and stablecoins to accept.
    #[serde(default)]
    pub networks: NetworksPaymentConfig,
}

impl PaymentsConfig {
    /// Returns the blockchain network for payments.
    ///
    /// Currently only Solana is supported.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let chain = config.payments.chain();
    /// assert_eq!(chain, Chain::Solana);
    /// ```
    pub fn chain(&self) -> Chain {
        self.networks.chain
    }

    /// Returns the list of accepted stablecoins.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let coins = config.payments.stablecoins();
    /// assert!(coins.contains(&"USDC".to_string()));
    /// ```
    pub fn stablecoins(&self) -> &[String] {
        &self.networks.stablecoins
    }
}

/// Network and stablecoin configuration for payments.
///
/// Specifies which blockchain network to use and which stablecoins
/// are accepted for payment.
///
/// # Fields
///
/// - `chain` - The blockchain network (currently only Solana)
/// - `stablecoins` - List of accepted stablecoin symbols (e.g., "USDC")
///
/// # Example
///
/// ```yaml
/// networks:
///   chain: Solana
///   stablecoins:
///     - USDC
/// ```
///
/// # Defaults
///
/// - `chain`: `Solana`
/// - `stablecoins`: `["USDC"]`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworksPaymentConfig {
    /// Blockchain network for payments.
    ///
    /// Currently only Solana is supported. More chains may be
    /// added in the future.
    #[serde(default)]
    pub chain: Chain,

    /// List of accepted stablecoin symbols.
    ///
    /// Common values:
    /// - `"USDC"` - USD Coin (default)
    ///
    /// Defaults to `["USDC"]` if not specified.
    #[serde(default = "default_stablecoins")]
    pub stablecoins: Vec<String>,
}

impl Default for NetworksPaymentConfig {
    fn default() -> Self {
        Self {
            chain: Chain::default(),
            stablecoins: default_stablecoins(),
        }
    }
}

/// Returns the default list of accepted stablecoins.
///
/// Currently defaults to USDC only.
fn default_stablecoins() -> Vec<String> {
    vec!["USDC".to_string()]
}
