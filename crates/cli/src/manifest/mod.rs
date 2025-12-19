use std::{fs, path::Path};

use indexmap::IndexMap;
use moneymq_types::x402::config::constants::DEFAULT_SANDBOX;
use serde::{Deserialize, Serialize};

pub mod environments;
pub mod payments;

pub use environments::{Chain, EnvironmentConfig, SandboxEnvironment};
pub use payments::PaymentsConfig;

/// MoneyMQ manifest file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Multiple catalog configurations
    /// Key is the catalog name (e.g., "v1", "v2")
    #[serde(default)]
    pub catalogs: IndexMap<String, CatalogConfig>,

    /// Payment configuration - what payments to accept
    #[serde(default)]
    pub payments: PaymentsConfig,

    /// Environment configurations
    /// Key is the environment name (e.g., "sandbox", "production")
    #[serde(default = "environments::default_environments")]
    pub environments: IndexMap<String, EnvironmentConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadManifestError {
    #[error("{} not found at {}. Please create a {} file in your project root.",
        moneymq_types::MANIFEST_FILE_NAME,
        .0.display(),
        moneymq_types::MANIFEST_FILE_NAME)]
    FileNotFound(std::path::PathBuf),
    #[error("Failed to read {}: {}", .0.display(), .1)]
    ReadError(std::path::PathBuf, std::io::Error),
    #[error("Failed to parse {}: {}", .0.display(), .1)]
    ParseError(std::path::PathBuf, serde_yml::Error),
}

impl Manifest {
    /// Load manifest from the specified file path
    pub fn load(manifest_file_path: &Path) -> Result<Self, LoadManifestError> {
        if !manifest_file_path.exists() {
            return Err(LoadManifestError::FileNotFound(
                manifest_file_path.to_path_buf(),
            ));
        }

        let content = fs::read_to_string(manifest_file_path)
            .map_err(|e| LoadManifestError::ReadError(manifest_file_path.to_path_buf(), e))?;

        let manifest: Manifest = serde_yml::from_str(&content)
            .map_err(|e| LoadManifestError::ParseError(manifest_file_path.to_path_buf(), e))?;

        Ok(manifest)
    }

    /// Try to load manifest, returning a default instance if the file doesn't exist
    pub fn load_or_default(manifest_file_path: &Path) -> Self {
        Self::load(manifest_file_path).unwrap_or_default()
    }

    /// Get a catalog configuration by name
    pub fn get_catalog(&self, name: &str) -> Option<&CatalogConfig> {
        self.catalogs.get(name)
    }

    /// Get an environment configuration by name
    pub fn get_environment(&self, name: &str) -> Option<&EnvironmentConfig> {
        self.environments.get(name)
    }

    /// Get the default sandbox environment
    pub fn get_sandbox(&self) -> Option<&EnvironmentConfig> {
        self.environments.get("sandbox")
    }

    /// Get the production environment
    pub fn get_production(&self) -> Option<&EnvironmentConfig> {
        self.environments.get("production")
    }

    /// Save manifest to the specified file path with proper formatting
    pub fn save(&self, path: &Path) -> Result<(), String> {
        use crate::yaml_util::to_pretty_yaml_with_header_and_footer;

        let content =
            to_pretty_yaml_with_header_and_footer(self, Some("Manifest"), Some("v1"), None)?;

        std::fs::write(path, content)
            .map_err(|e| format!("Failed to write manifest to {}: {}", path.display(), e))?;

        Ok(())
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            catalogs: IndexMap::new(),
            payments: PaymentsConfig::default(),
            environments: environments::default_environments(),
        }
    }
}

/// Catalog configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogConfig {
    /// Optional description of this catalog (e.g., "Stripe account - Acme Corp")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Catalog path - base path for billing data (e.g., "billing/v1") - defaults to "billing/v1"
    /// Products are in {catalog_path}/products, meters in {catalog_path}/meters, etc.
    #[serde(default = "default_catalog_path")]
    pub catalog_path: String,

    /// The source/provider for this catalog (defaults to Stripe if not specified)
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub source: Option<CatalogSourceType>,
}

impl CatalogConfig {
    /// Get Stripe configuration if this catalog uses Stripe as source
    pub fn stripe_config(&self) -> Option<&StripeConfig> {
        match &self.source {
            Some(CatalogSourceType::Stripe(config)) => Some(config),
            None => None,
        }
    }
}

/// Catalog source type (Stripe, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum CatalogSourceType {
    Stripe(StripeConfig),
}

/// Stripe sandbox/test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSandboxConfig {
    /// Optional description of this sandbox environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Stripe API secret key (optional)
    /// WARNING: It's recommended to use STRIPE_SANDBOX_SECRET_KEY environment variable instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// API version to use (optional, defaults to Stripe's latest)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

    /// Webhook endpoint URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_endpoint: Option<String>,

    /// Webhook secret for signature verification (should be in .env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret_env: Option<String>,
}

impl Default for StripeSandboxConfig {
    fn default() -> Self {
        Self {
            description: None,
            api_key: None,
            api_version: None,
            webhook_endpoint: None,
            webhook_secret_env: None,
        }
    }
}

impl StripeSandboxConfig {
    pub fn api_key(&self) -> Option<&String> {
        self.api_key.as_ref()
    }

    pub fn api_version(&self) -> Option<&String> {
        self.api_version.as_ref()
    }
}

/// Stripe catalog source configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StripeConfig {
    /// Stripe API secret key (optional)
    /// WARNING: It's recommended to use STRIPE_SECRET_KEY environment variable instead
    /// to avoid committing secrets to version control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// API version to use (optional, defaults to Stripe's latest)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

    /// Webhook endpoint URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_endpoint: Option<String>,

    /// Webhook secret for signature verification (should be in .env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret_env: Option<String>,

    /// Nested sandbox/test configurations
    /// Key is the sandbox name (e.g., "default", "staging", "test")
    /// When --sandbox flag is used, the "default" sandbox will be used
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sandboxes: IndexMap<String, StripeSandboxConfig>,
}

impl StripeConfig {
    /// Get the default sandbox configuration
    pub fn get_default_sandbox(&self) -> Option<&StripeSandboxConfig> {
        self.sandboxes.get(DEFAULT_SANDBOX)
    }
}

fn default_catalog_path() -> String {
    "billing/v1".to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use moneymq_types::x402::config::constants::{
        DEFAULT_MONEYMQ_PORT, DEFAULT_SOLANA_RPC_PORT, DEFAULT_SOLANA_WS_PORT,
    };
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_parse_new_manifest_structure() {
        let yaml = r#"
catalogs:
  v1:
    description: 'Surfpool'
    catalog_path: billing/v1

payments:
  networks:
    chain: Solana
    stablecoins:
      - USDC

environments:
  sandbox:
    deployment: Sandbox
    binding_address: 0.0.0.0
    port: 8488
    facilitator:
      fee: 0
      key_management: TurnKey
    network:
      chain: Solana
      binding_address: 0.0.0.0
      rpc_port: 8899
      ws_port: 8900

  internal:
    deployment: SelfHosted
    binding_address: 0.0.0.0
    port: 8488
    facilitator:
      fee: 0
      key_management: TurnKey
    network:
      chain: Solana
      rpc_url: http://localhost:8899
      ws_url: ws://localhost:8900

  production:
    deployment: CloudHosted
    project: Surfpool Project
    workspace: surfpool
    facilitator:
      fee: 0
      key_management: TurnKey
"#;

        let manifest: Manifest = serde_yml::from_str(yaml).expect("Failed to parse manifest");

        // Verify catalogs
        assert_eq!(manifest.catalogs.len(), 1);
        let catalog = manifest.get_catalog("v1").expect("Catalog v1 not found");
        assert_eq!(catalog.description, Some("Surfpool".to_string()));

        // Verify payments
        assert_eq!(manifest.payments.networks.chain, Chain::Solana);
        assert_eq!(manifest.payments.networks.stablecoins, vec!["USDC"]);

        // Verify environments
        assert_eq!(manifest.environments.len(), 3);

        // Check sandbox
        match manifest.get_sandbox().expect("Sandbox not found") {
            EnvironmentConfig::Sandbox(env) => {
                assert_eq!(env.port, DEFAULT_MONEYMQ_PORT);
                assert_eq!(env.network.rpc_port, DEFAULT_SOLANA_RPC_PORT);
                assert_eq!(env.network.ws_port, DEFAULT_SOLANA_WS_PORT);
            }
            _ => panic!("Expected Sandbox environment"),
        }

        // Check internal (SelfHosted)
        match manifest
            .get_environment("internal")
            .expect("Internal not found")
        {
            EnvironmentConfig::SelfHosted(env) => {
                assert_eq!(env.network.rpc_url, "http://localhost:8899");
                assert_eq!(env.network.ws_url, Some("ws://localhost:8900".to_string()));
            }
            _ => panic!("Expected SelfHosted environment"),
        }

        // Check production (CloudHosted)
        match manifest.get_production().expect("Production not found") {
            EnvironmentConfig::CloudHosted(env) => {
                assert_eq!(env.project, "Surfpool Project");
                assert_eq!(env.workspace, "surfpool");
            }
            _ => panic!("Expected CloudHosted environment"),
        }
    }

    #[test]
    fn test_manifest_default_has_sandbox() {
        let manifest = Manifest::default();

        assert!(manifest.environments.contains_key("sandbox"));
        match manifest.get_sandbox().unwrap() {
            EnvironmentConfig::Sandbox(env) => {
                assert_eq!(env.port, DEFAULT_MONEYMQ_PORT);
            }
            _ => panic!("Expected Sandbox environment"),
        }
    }

    #[test]
    fn test_manifest_save_and_load() {
        let mut manifest = Manifest::default();

        manifest.catalogs.insert(
            "v1".to_string(),
            CatalogConfig {
                description: Some("Test catalog".to_string()),
                catalog_path: "billing/v1".to_string(),
                source: None,
            },
        );

        // Save to temp file
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let manifest_path = temp_dir.path().join("moneymq.yaml");

        manifest
            .save(&manifest_path)
            .expect("Failed to save manifest");

        // Read back the content
        let content = fs::read_to_string(&manifest_path).expect("Failed to read manifest");

        // Verify it has the header
        assert!(content.starts_with("---\n"));
        assert!(content.contains("# MoneyMQ Manifest - API version v1"));

        // Verify structure
        assert!(content.contains("catalogs:"));
        assert!(content.contains("payments:"));
        assert!(content.contains("environments:"));

        println!("Generated manifest:\n{}", content);

        // Load it back and verify
        let loaded = Manifest::load(&manifest_path).expect("Failed to load manifest");
        assert_eq!(loaded.catalogs.len(), 1);
        assert!(loaded.environments.contains_key("sandbox"));
    }
}
