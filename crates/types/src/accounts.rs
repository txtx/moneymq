//! Account configuration types for MoneyMQ.
//!
//! This module defines account types used for payment processing:
//! - **Payout** - Receives settled payments (merchant's settlement account)
//! - **Operator** - Manages transaction signing/sponsoring (facilitator)
//! - **Fanout** - Distributes payments to multiple recipients
//! - **Operated** - Account controlled by an operator
//!
//! # Loading Accounts
//!
//! Accounts are loaded from YAML files in the `accounts/` directory:
//!
//! ```text
//! billing/v1/accounts/
//! ├── payout-1.yaml      # id: "payout_1"
//! ├── operator.yaml      # id: "operator"
//! └── fanout.yaml        # id: "fanout"
//! ```
//!
//! The `id` field defaults to the snake_cased filename if not specified.
//!
//! # Example YAML
//!
//! ```yaml
//! # billing/v1/accounts/payout-1.yaml
//! name: Payout account 1
//! role:
//!   type: payout
//!   recipient_address: DEznE3SWxvzHVvME3hqxdip4qDPn5j2XN7CNYhgMiqr6
//!   network: solana
//! currency_mapping:
//!   usd:
//!     - USDC
//! ```

use std::path::Path;

use indexmap::IndexMap;
#[cfg(feature = "schemars")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Collection of accounts indexed by account ID
pub type AccountsConfig = IndexMap<String, AccountConfig>;

/// Account configuration loaded from a YAML file
///
/// The `id` field defaults to the snake_cased filename if not specified.
/// e.g., `billing/v1/accounts/payout-1.yaml` → id: "payout_1"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct AccountConfig {
    /// Unique identifier for the account
    /// Defaults to the filename (snake_cased) if not specified
    #[serde(default)]
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Account role and role-specific configuration
    pub role: AccountRole,

    /// Currency mapping (fiat -> stablecoins)
    /// e.g., { "usd": ["USDC"] }
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub currency_mapping: IndexMap<String, Vec<String>>,
}

impl AccountConfig {
    /// Set the ID from filename if not already set
    pub fn with_id_from_filename(mut self, filename: &str) -> Self {
        if self.id.is_empty() {
            self.id = to_snake_case(filename);
        }
        self
    }

    /// Returns true if this is a payout account
    pub fn is_payout(&self) -> bool {
        matches!(self.role, AccountRole::Payout(_))
    }

    /// Returns true if this is an operator account
    pub fn is_operator(&self) -> bool {
        matches!(self.role, AccountRole::Operator(_))
    }

    /// Returns true if this is a fanout account
    pub fn is_fanout(&self) -> bool {
        matches!(self.role, AccountRole::Fanout(_))
    }

    /// Get payout role if this is a payout account
    pub fn payout_role(&self) -> Option<&PayoutRole> {
        match &self.role {
            AccountRole::Payout(role) => Some(role),
            _ => None,
        }
    }

    /// Get operator role if this is an operator account
    pub fn operator_role(&self) -> Option<&OperatorRole> {
        match &self.role {
            AccountRole::Operator(role) => Some(role),
            _ => None,
        }
    }

    /// Get fanout role if this is a fanout account
    pub fn fanout_role(&self) -> Option<&FanoutRole> {
        match &self.role {
            AccountRole::Fanout(role) => Some(role),
            _ => None,
        }
    }
}

/// Account role types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountRole {
    /// Payout account - receives settled payments
    Payout(PayoutRole),

    /// Operator account - manages transaction signing/sponsoring
    Operator(OperatorRole),

    /// Fanout account - distributes to multiple recipients
    Fanout(FanoutRole),

    /// Operated account - controlled by an operator
    Operated(OperatedRole),
}

/// Payout role - account that receives settled payments
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct PayoutRole {
    /// The recipient address for payments
    pub recipient_address: String,

    /// Network (defaults to "solana")
    #[serde(default = "default_network")]
    pub network: String,
}

/// Operator role - account that manages/sponsors transactions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct OperatorRole {
    /// Key management configuration
    #[serde(default)]
    pub keychain: Keychain,
}

/// Key management configuration for operator accounts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Keychain {
    /// Turnkey managed keys
    Turnkey(TurnkeyKeychain),

    /// Base58-encoded secret key (Solana keypair format)
    Base58(Base58Keychain),
}

impl Default for Keychain {
    fn default() -> Self {
        Keychain::Turnkey(TurnkeyKeychain::default())
    }
}

/// Turnkey key management configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct TurnkeyKeychain {
    /// Turnkey API secret reference (env var name or secret ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

/// Base58-encoded secret key
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct Base58Keychain {
    /// Base58-encoded secret key or env var reference
    pub secret: String,
}

/// Fanout role - distributes payments to multiple recipients
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct FanoutRole {
    /// Operator account ID that manages this fanout
    pub operator: String,

    /// Distribution recipients
    #[serde(default)]
    pub recipients: Vec<FanoutRecipient>,
}

/// Fanout recipient configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct FanoutRecipient {
    /// Reference to another account by ID
    pub account: String,

    /// Fixed amount in smallest unit (mutually exclusive with percentage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_amount: Option<u64>,

    /// Percentage 0-100 (mutually exclusive with fixed_amount)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f64>,
}

/// Operated role - account controlled by an operator
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct OperatedRole {
    /// Operator account ID that controls this account
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
}

fn default_network() -> String {
    "solana".to_string()
}

/// Convert string to snake_case (for ID generation from filename)
///
/// Examples:
/// - "payout-1" -> "payout_1"
/// - "PayoutMain" -> "payout_main"
/// - "my-account" -> "my_account"
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c == '-' {
            result.push('_');
        } else if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Load accounts from a directory of YAML files
///
/// Each `.yaml` or `.yml` file in the directory is loaded as an account.
/// The account ID defaults to the snake_cased filename if not specified in the YAML.
///
/// # Arguments
/// * `accounts_dir` - Path to the accounts directory (e.g., `billing/v1/accounts`)
///
/// # Returns
/// * `Ok(AccountsConfig)` - IndexMap of account ID -> AccountConfig
/// * `Err(String)` - Error message if loading fails
pub fn load_accounts_from_dir(accounts_dir: &Path) -> Result<AccountsConfig, String> {
    let mut accounts = IndexMap::new();

    if !accounts_dir.exists() {
        return Ok(accounts);
    }

    for entry in std::fs::read_dir(accounts_dir)
        .map_err(|e| format!("Failed to read accounts directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path
            .extension()
            .map_or(false, |ext| ext == "yaml" || ext == "yml")
        {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            let account: AccountConfig = serde_yml::from_str(&content)
                .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

            // Get filename without extension for ID
            let filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("Invalid filename: {}", path.display()))?;

            // Apply ID from filename if not set
            let account = account.with_id_from_filename(filename);

            // Use the account ID as the key
            accounts.insert(account.id.clone(), account);
        }
    }

    Ok(accounts)
}

/// Extension trait for AccountsConfig
pub trait AccountsConfigExt {
    /// Get the primary payout account (first payout-type account)
    fn primary_payout(&self) -> Option<&AccountConfig>;

    /// Get all payout accounts
    fn payouts(&self) -> Vec<&AccountConfig>;

    /// Get all operator accounts
    fn operators(&self) -> Vec<&AccountConfig>;

    /// Get account by ID
    fn get_by_id(&self, id: &str) -> Option<&AccountConfig>;
}

impl AccountsConfigExt for AccountsConfig {
    fn primary_payout(&self) -> Option<&AccountConfig> {
        self.values().find(|acc| acc.is_payout())
    }

    fn payouts(&self) -> Vec<&AccountConfig> {
        self.values().filter(|acc| acc.is_payout()).collect()
    }

    fn operators(&self) -> Vec<&AccountConfig> {
        self.values().filter(|acc| acc.is_operator()).collect()
    }

    fn get_by_id(&self, id: &str) -> Option<&AccountConfig> {
        self.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("payout-1"), "payout_1");
        assert_eq!(to_snake_case("PayoutMain"), "payout_main");
        assert_eq!(to_snake_case("my-account"), "my_account");
        assert_eq!(to_snake_case("operator"), "operator");
    }

    #[test]
    fn test_parse_payout_account() {
        let yaml = r#"
name: Payout account 1
role:
  type: payout
  recipient_address: DEznE3SWxvzHVvME3hqxdip4qDPn5j2XN7CNYhgMiqr6
  network: solana
currency_mapping:
  usd:
    - USDC
"#;

        let account: AccountConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(account.name, "Payout account 1");
        assert!(account.is_payout());

        let payout = account.payout_role().unwrap();
        assert_eq!(
            payout.recipient_address,
            "DEznE3SWxvzHVvME3hqxdip4qDPn5j2XN7CNYhgMiqr6"
        );
        assert_eq!(payout.network, "solana");
    }

    #[test]
    fn test_parse_operator_account_turnkey() {
        let yaml = r#"
name: Operating account
role:
  type: operator
  keychain:
    type: turnkey
    secret: TURNKEY_SECRET
"#;

        let account: AccountConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(account.name, "Operating account");
        assert!(account.is_operator());

        let operator = account.operator_role().unwrap();
        match &operator.keychain {
            Keychain::Turnkey(tk) => {
                assert_eq!(tk.secret, Some("TURNKEY_SECRET".to_string()));
            }
            _ => panic!("Expected Turnkey keychain"),
        }
    }

    #[test]
    fn test_parse_operator_account_base58() {
        let yaml = r#"
name: Local operator
role:
  type: operator
  keychain:
    type: base58
    secret: "5K1gY..."
"#;

        let account: AccountConfig = serde_yml::from_str(yaml).unwrap();
        assert!(account.is_operator());

        let operator = account.operator_role().unwrap();
        match &operator.keychain {
            Keychain::Base58(base58) => {
                assert_eq!(base58.secret, "5K1gY...");
            }
            _ => panic!("Expected Base58 keychain"),
        }
    }

    #[test]
    fn test_parse_fanout_account() {
        let yaml = r#"
name: Revenue split
role:
  type: fanout
  operator: ops
  recipients:
    - account: payout_1
      percentage: 90
    - account: platform_fee
      fixed_amount: 1000000
"#;

        let account: AccountConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(account.name, "Revenue split");
        assert!(account.is_fanout());

        let fanout = account.fanout_role().unwrap();
        assert_eq!(fanout.operator, "ops");
        assert_eq!(fanout.recipients.len(), 2);
        assert_eq!(fanout.recipients[0].account, "payout_1");
        assert_eq!(fanout.recipients[0].percentage, Some(90.0));
        assert_eq!(fanout.recipients[1].fixed_amount, Some(1000000));
    }

    #[test]
    fn test_id_from_filename() {
        let yaml = r#"
name: Test account
role:
  type: payout
  recipient_address: ABC123
"#;

        let account: AccountConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(account.id, ""); // ID is empty initially

        let account = account.with_id_from_filename("payout-main");
        assert_eq!(account.id, "payout_main");
    }

    #[test]
    fn test_explicit_id_not_overwritten() {
        let yaml = r#"
id: my_custom_id
name: Test account
role:
  type: payout
  recipient_address: ABC123
"#;

        let account: AccountConfig = serde_yml::from_str(yaml).unwrap();
        let account = account.with_id_from_filename("payout-main");
        assert_eq!(account.id, "my_custom_id"); // Explicit ID preserved
    }
}
