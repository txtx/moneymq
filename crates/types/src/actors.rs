//! Actor configuration types for MoneyMQ.
//!
//! This module defines actor types used for payment processing:
//! - **Payout** - Receives settled payments (merchant's settlement account)
//! - **Operator** - Manages transaction signing/sponsoring (facilitator)
//! - **Fanout** - Distributes payments to multiple recipients
//! - **Operated** - Account controlled by an operator
//! - **Hook** - Webhook-based actor that responds to payment lifecycle events
//!
//! # Loading Actors
//!
//! Actors are loaded from YAML files in the `actors/` directory:
//!
//! ```text
//! billing/v1/actors/
//! ├── payout-1.yaml      # id: "payout_1"
//! ├── operator.yaml      # id: "operator"
//! ├── fanout.yaml        # id: "fanout"
//! └── hook.yaml          # id: "hook"
//! ```
//!
//! The `id` field defaults to the snake_cased filename if not specified.
//!
//! # Example YAML
//!
//! ```yaml
//! # billing/v1/actors/payout-1.yaml
//! name: Payout account 1
//! role:
//!   type: payout
//!   recipient_address: DEznE3SWxvzHVvME3hqxdip4qDPn5j2XN7CNYhgMiqr6
//!   network: solana
//! currency_mapping:
//!   usd:
//!     - USDC
//! ```
//!
//! # Hook Actor Example
//!
//! ```yaml
//! # billing/v1/actors/hook.yaml
//! name: Operating account
//! role:
//!   type: hook
//!   ping: https://api.example.com/v1/ping
//!   event:
//!     pre_pricing:
//!     post_verification:
//!       instructions:
//!         - key: fee
//!     post_settlement:
//!       attachments:
//!         - key: surfnet
//!           required: true
//! ```

use std::path::Path;

use indexmap::IndexMap;
#[cfg(feature = "schemars")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Collection of actors indexed by actor ID
pub type ActorsConfig = IndexMap<String, ActorConfig>;

/// Actor configuration loaded from a YAML file
///
/// The `id` field defaults to the snake_cased filename if not specified.
/// e.g., `billing/v1/actors/payout-1.yaml` → id: "payout_1"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct ActorConfig {
    /// Unique identifier for the actor
    /// Defaults to the filename (snake_cased) if not specified
    #[serde(default)]
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Actor role and role-specific configuration
    pub role: ActorRole,

    /// Currency mapping (fiat -> stablecoins)
    /// e.g., { "usd": ["USDC"] }
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub currency_mapping: IndexMap<String, Vec<String>>,
}

impl ActorConfig {
    /// Set the ID from filename if not already set
    pub fn with_id_from_filename(mut self, filename: &str) -> Self {
        if self.id.is_empty() {
            self.id = to_snake_case(filename);
        }
        self
    }

    /// Returns true if this is a payout actor
    pub fn is_payout(&self) -> bool {
        matches!(self.role, ActorRole::Payout(_))
    }

    /// Returns true if this is an operator actor
    pub fn is_operator(&self) -> bool {
        matches!(self.role, ActorRole::Operator(_))
    }

    /// Returns true if this is a fanout actor
    pub fn is_fanout(&self) -> bool {
        matches!(self.role, ActorRole::Fanout(_))
    }

    /// Returns true if this is a hook actor
    pub fn is_hook(&self) -> bool {
        matches!(self.role, ActorRole::Hook(_))
    }

    /// Get payout role if this is a payout actor
    pub fn payout_role(&self) -> Option<&PayoutRole> {
        match &self.role {
            ActorRole::Payout(role) => Some(role),
            _ => None,
        }
    }

    /// Get operator role if this is an operator actor
    pub fn operator_role(&self) -> Option<&OperatorRole> {
        match &self.role {
            ActorRole::Operator(role) => Some(role),
            _ => None,
        }
    }

    /// Get fanout role if this is a fanout actor
    pub fn fanout_role(&self) -> Option<&FanoutRole> {
        match &self.role {
            ActorRole::Fanout(role) => Some(role),
            _ => None,
        }
    }

    /// Get hook role if this is a hook actor
    pub fn hook_role(&self) -> Option<&HookRole> {
        match &self.role {
            ActorRole::Hook(role) => Some(role),
            _ => None,
        }
    }
}

/// Actor role types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActorRole {
    /// Payout actor - receives settled payments
    Payout(PayoutRole),

    /// Operator actor - manages transaction signing/sponsoring
    Operator(OperatorRole),

    /// Fanout actor - distributes to multiple recipients
    Fanout(FanoutRole),

    /// Operated actor - controlled by an operator
    Operated(OperatedRole),

    /// Hook actor - webhook-based actor for payment lifecycle events
    Hook(HookRole),
}

/// Payout role - actor that receives settled payments
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct PayoutRole {
    /// The recipient address for payments
    pub recipient_address: String,

    /// Network (defaults to "solana")
    #[serde(default = "default_network")]
    pub network: String,
}

/// Operator role - actor that manages/sponsors transactions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct OperatorRole {
    /// Key management configuration
    #[serde(default)]
    pub keychain: Keychain,
}

/// Key management configuration for operator actors
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
    /// Operator actor ID that manages this fanout
    pub operator: String,

    /// Distribution recipients
    #[serde(default)]
    pub recipients: Vec<FanoutRecipient>,
}

/// Fanout recipient configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct FanoutRecipient {
    /// Reference to another actor by ID
    pub account: String,

    /// Fixed amount in smallest unit (mutually exclusive with percentage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_amount: Option<u64>,

    /// Percentage 0-100 (mutually exclusive with fixed_amount)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f64>,
}

/// Operated role - actor controlled by an operator
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct OperatedRole {
    /// Operator actor ID that controls this actor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
}

/// Hook role - webhook-based actor for payment lifecycle events
///
/// Hook actors can respond to payment events and provide:
/// - Instructions to modify transaction behavior
/// - Attachments to include in payment receipts
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct HookRole {
    /// Health check endpoint URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping: Option<String>,

    /// Event handlers configuration
    #[serde(default)]
    pub event: HookEventConfig,
}

/// Hook event handlers configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct HookEventConfig {
    /// Called before pricing is calculated
    /// Can modify pricing parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_pricing: Option<HookEventHandler>,

    /// Called after payment is verified but before settlement
    /// Can add instructions to the settlement transaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_verification: Option<HookEventHandler>,

    /// Called after payment is settled
    /// Can provide attachments for the receipt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_settlement: Option<HookEventHandler>,
}

/// Configuration for a hook event handler
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct HookEventHandler {
    /// Instructions that this hook can provide
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<HookInstruction>,

    /// Attachments that this hook can provide
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<HookAttachment>,
}

/// Instruction specification for hook responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct HookInstruction {
    /// Key identifier for this instruction
    pub key: String,

    /// Whether this instruction is required
    #[serde(default)]
    pub required: bool,
}

/// Attachment specification for hook responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct HookAttachment {
    /// Key identifier for this attachment
    pub key: String,

    /// Whether this attachment is required
    #[serde(default)]
    pub required: bool,
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

/// Load actors from a directory of YAML files
///
/// Each `.yaml` or `.yml` file in the directory is loaded as an actor.
/// The actor ID defaults to the snake_cased filename if not specified in the YAML.
///
/// # Arguments
/// * `actors_dir` - Path to the actors directory (e.g., `billing/v1/actors`)
///
/// # Returns
/// * `Ok(ActorsConfig)` - IndexMap of actor ID -> ActorConfig
/// * `Err(String)` - Error message if loading fails
pub fn load_actors_from_dir(actors_dir: &Path) -> Result<ActorsConfig, String> {
    let mut actors = IndexMap::new();

    if !actors_dir.exists() {
        return Ok(actors);
    }

    for entry in std::fs::read_dir(actors_dir)
        .map_err(|e| format!("Failed to read actors directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path
            .extension()
            .map_or(false, |ext| ext == "yaml" || ext == "yml")
        {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            let actor: ActorConfig = serde_yml::from_str(&content)
                .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

            // Get filename without extension for ID
            let filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("Invalid filename: {}", path.display()))?;

            // Apply ID from filename if not set
            let actor = actor.with_id_from_filename(filename);

            // Use the actor ID as the key
            actors.insert(actor.id.clone(), actor);
        }
    }

    Ok(actors)
}

/// Extension trait for ActorsConfig
pub trait ActorsConfigExt {
    /// Get the primary payout actor (first payout-type actor)
    fn primary_payout(&self) -> Option<&ActorConfig>;

    /// Get all payout actors
    fn payouts(&self) -> Vec<&ActorConfig>;

    /// Get all operator actors
    fn operators(&self) -> Vec<&ActorConfig>;

    /// Get all hook actors
    fn hooks(&self) -> Vec<&ActorConfig>;

    /// Get actor by ID
    fn get_by_id(&self, id: &str) -> Option<&ActorConfig>;
}

impl ActorsConfigExt for ActorsConfig {
    fn primary_payout(&self) -> Option<&ActorConfig> {
        self.values().find(|acc| acc.is_payout())
    }

    fn payouts(&self) -> Vec<&ActorConfig> {
        self.values().filter(|acc| acc.is_payout()).collect()
    }

    fn operators(&self) -> Vec<&ActorConfig> {
        self.values().filter(|acc| acc.is_operator()).collect()
    }

    fn hooks(&self) -> Vec<&ActorConfig> {
        self.values().filter(|acc| acc.is_hook()).collect()
    }

    fn get_by_id(&self, id: &str) -> Option<&ActorConfig> {
        self.get(id)
    }
}

// Backwards compatibility type aliases
#[deprecated(since = "0.2.0", note = "Use ActorsConfig instead")]
pub type AccountsConfig = ActorsConfig;

#[deprecated(since = "0.2.0", note = "Use ActorConfig instead")]
pub type AccountConfig = ActorConfig;

#[deprecated(since = "0.2.0", note = "Use ActorRole instead")]
pub type AccountRole = ActorRole;

#[deprecated(since = "0.2.0", note = "Use load_actors_from_dir instead")]
pub fn load_accounts_from_dir(accounts_dir: &Path) -> Result<ActorsConfig, String> {
    load_actors_from_dir(accounts_dir)
}

#[deprecated(since = "0.2.0", note = "Use ActorsConfigExt instead")]
pub trait AccountsConfigExt: ActorsConfigExt {}

impl<T: ActorsConfigExt> AccountsConfigExt for T {}

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
    fn test_parse_payout_actor() {
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

        let actor: ActorConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(actor.name, "Payout account 1");
        assert!(actor.is_payout());

        let payout = actor.payout_role().unwrap();
        assert_eq!(
            payout.recipient_address,
            "DEznE3SWxvzHVvME3hqxdip4qDPn5j2XN7CNYhgMiqr6"
        );
        assert_eq!(payout.network, "solana");
    }

    #[test]
    fn test_parse_operator_actor_turnkey() {
        let yaml = r#"
name: Operating account
role:
  type: operator
  keychain:
    type: turnkey
    secret: TURNKEY_SECRET
"#;

        let actor: ActorConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(actor.name, "Operating account");
        assert!(actor.is_operator());

        let operator = actor.operator_role().unwrap();
        match &operator.keychain {
            Keychain::Turnkey(tk) => {
                assert_eq!(tk.secret, Some("TURNKEY_SECRET".to_string()));
            }
            _ => panic!("Expected Turnkey keychain"),
        }
    }

    #[test]
    fn test_parse_operator_actor_base58() {
        let yaml = r#"
name: Local operator
role:
  type: operator
  keychain:
    type: base58
    secret: "5K1gY..."
"#;

        let actor: ActorConfig = serde_yml::from_str(yaml).unwrap();
        assert!(actor.is_operator());

        let operator = actor.operator_role().unwrap();
        match &operator.keychain {
            Keychain::Base58(base58) => {
                assert_eq!(base58.secret, "5K1gY...");
            }
            _ => panic!("Expected Base58 keychain"),
        }
    }

    #[test]
    fn test_parse_fanout_actor() {
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

        let actor: ActorConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(actor.name, "Revenue split");
        assert!(actor.is_fanout());

        let fanout = actor.fanout_role().unwrap();
        assert_eq!(fanout.operator, "ops");
        assert_eq!(fanout.recipients.len(), 2);
        assert_eq!(fanout.recipients[0].account, "payout_1");
        assert_eq!(fanout.recipients[0].percentage, Some(90.0));
        assert_eq!(fanout.recipients[1].fixed_amount, Some(1000000));
    }

    #[test]
    fn test_parse_hook_actor() {
        let yaml = r#"
name: Operating account
role:
  type: hook
  ping: https://api.example.com/v1/ping
  event:
    pre_pricing:
    post_verification:
      instructions:
        - key: fee
    post_settlement:
      attachments:
        - key: surfnet
          required: true
"#;

        let actor: ActorConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(actor.name, "Operating account");
        assert!(actor.is_hook());

        let hook = actor.hook_role().unwrap();
        assert_eq!(
            hook.ping,
            Some("https://api.example.com/v1/ping".to_string())
        );

        // Check post_verification has instructions
        let post_verify = hook.event.post_verification.as_ref().unwrap();
        assert_eq!(post_verify.instructions.len(), 1);
        assert_eq!(post_verify.instructions[0].key, "fee");

        // Check post_settlement has attachments
        let post_settle = hook.event.post_settlement.as_ref().unwrap();
        assert_eq!(post_settle.attachments.len(), 1);
        assert_eq!(post_settle.attachments[0].key, "surfnet");
        assert!(post_settle.attachments[0].required);
    }

    #[test]
    fn test_id_from_filename() {
        let yaml = r#"
name: Test actor
role:
  type: payout
  recipient_address: ABC123
"#;

        let actor: ActorConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(actor.id, ""); // ID is empty initially

        let actor = actor.with_id_from_filename("payout-main");
        assert_eq!(actor.id, "payout_main");
    }

    #[test]
    fn test_explicit_id_not_overwritten() {
        let yaml = r#"
id: my_custom_id
name: Test actor
role:
  type: payout
  recipient_address: ABC123
"#;

        let actor: ActorConfig = serde_yml::from_str(yaml).unwrap();
        let actor = actor.with_id_from_filename("payout-main");
        assert_eq!(actor.id, "my_custom_id"); // Explicit ID preserved
    }
}
