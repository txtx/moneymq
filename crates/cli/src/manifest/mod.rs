use std::{fs, path::Path};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::manifest::x402::PaymentConfig;
pub mod x402;
// TODO: Re-enable x402_rs imports when refactoring X402 facilitator
// use x402_rs::{
//     chain::{NetworkProvider, solana::SolanaProvider},
//     provider_cache::ProviderCache,
// };

/// MoneyMQ manifest file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Multiple catalog configurations
    /// Key is the catalog name (e.g., "stripe", "stripe_sandbox")
    #[serde(default)]
    pub catalogs: IndexMap<String, CatalogConfig>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub payments: IndexMap<String, PaymentConfig>,
}

impl Manifest {
    /// Load manifest from the specified file path
    pub fn load(manifest_file_path: &Path) -> Result<Self, String> {
        if !manifest_file_path.exists() {
            return Err(format!(
                "{} not found at {}. Please create a {} file in your project root.",
                moneymq_types::MANIFEST_FILE_NAME,
                manifest_file_path.display(),
                moneymq_types::MANIFEST_FILE_NAME
            ));
        }

        let content = fs::read_to_string(manifest_file_path)
            .map_err(|e| format!("Failed to read {}: {}", manifest_file_path.display(), e))?;

        let manifest: Manifest = serde_yml::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", manifest_file_path.display(), e))?;

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

    /// Get a payment configuration by name
    pub fn get_payment(&self, name: &str) -> Option<&PaymentConfig> {
        self.payments.get(name)
    }

    /// Save manifest to the specified file path with proper formatting
    pub fn save(&self, path: &Path) -> Result<(), String> {
        use crate::yaml_util::{get_default_payments_footer, to_pretty_yaml_with_header_and_footer};

        // Only add the payments footer if payments section is empty
        let footer_string;
        let footer = if self.payments.is_empty() {
            footer_string = get_default_payments_footer();
            Some(footer_string.as_str())
        } else {
            None
        };

        let content = to_pretty_yaml_with_header_and_footer(
            self,
            Some("Manifest"),
            Some("v1"),
            footer,
        )?;

        std::fs::write(path, content)
            .map_err(|e| format!("Failed to write manifest to {}: {}", path.display(), e))?;

        Ok(())
    }
}

impl Default for Manifest {
    fn default() -> Self {
        let mut payments = IndexMap::new();
        payments.insert("stablecoins".into(), PaymentConfig::default());
        Manifest {
            payments,
            catalogs: IndexMap::new(),
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

fn default_catalog_path() -> String {
    "billing/v1".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::x402::{
        AcceptedNetworkConfig, FacilitatorConfig, NetworkIdentifier, PaymentConfig,
        SandboxFacilitatorConfig, SupportedNetworkConfig, ValidatorConfig, X402PaymentConfig,
        X402SandboxConfig,
    };
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_write_complete_manifest_to_disk() {
        // Create a complete manifest with catalogs and payments
        let mut manifest = Manifest {
            catalogs: IndexMap::new(),
            payments: IndexMap::new(),
        };

        // Add catalog configuration
        manifest.catalogs.insert(
            "v1".to_string(),
            CatalogConfig {
                description: Some("Production catalog".to_string()),
                catalog_path: "billing/v1".to_string(),
                source: Some(CatalogSourceType::Stripe(StripeConfig {
                    api_key: None,
                    api_version: Some("2023-10-16".to_string()),
                    webhook_endpoint: None,
                    webhook_secret_env: None,
                    sandboxes: IndexMap::new(),
                })),
            },
        );

        // Add payment configuration
        let mut x402_config = X402PaymentConfig {
            description: Some("Solana stablecoin payments".to_string()),
            facilitator: FacilitatorConfig::ServiceUrl {
                service_url: "https://facilitator.moneymq.co".to_string(),
            },
            accepted: IndexMap::new(),
            sandboxes: IndexMap::new(),
        };

        // Add accepted network
        x402_config.accepted.insert(
            NetworkIdentifier::Solana,
            AcceptedNetworkConfig {
                recipient: Some("recipient123456789".to_string()),
                currencies: vec!["USDC".to_string(), "USDT".to_string()],
            },
        );

        // Add sandbox configuration
        let mut sandbox_facilitator_supported = IndexMap::new();
        sandbox_facilitator_supported.insert(
            NetworkIdentifier::Solana,
            SupportedNetworkConfig {
                recipient: None,
                currencies: vec!["USDC".to_string()],
                fee: 0,
                payer_keypair_path: None,
                rpc_url: None,
                user_accounts: vec![
                    "user1_account".to_string(),
                    "user2_account".to_string(),
                ],
            },
        );

        x402_config.sandboxes.insert(
            "default".to_string(),
            X402SandboxConfig {
                description: Some("Local development sandbox".to_string()),
                facilitator: FacilitatorConfig::Embedded(SandboxFacilitatorConfig {
                    binding_address: "0.0.0.0".to_string(),
                    binding_port: 8080,
                    supported: sandbox_facilitator_supported,
                }),
                validator: ValidatorConfig {
                    binding_address: "0.0.0.0".to_string(),
                    rpc_binding_port: 8899,
                    ws_binding_port: 8900,
                },
            },
        );

        manifest
            .payments
            .insert("stablecoins".to_string(), PaymentConfig::X402(x402_config));

        // Create a temporary directory
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let manifest_path = temp_dir.path().join("moneymq.yaml");

        // Serialize to YAML with header
        let yaml_content = format!(
            "---\n# MoneyMQ Manifest - API version v1\n{}",
            serde_yml::to_string(&manifest).expect("Failed to serialize manifest")
        );

        // Write to disk
        fs::write(&manifest_path, &yaml_content).expect("Failed to write manifest file");

        // Verify file was created
        assert!(manifest_path.exists(), "Manifest file was not created");

        // Read back and verify it parses correctly
        let read_content = fs::read_to_string(&manifest_path).expect("Failed to read manifest file");
        let parsed_manifest: Manifest =
            serde_yml::from_str(&read_content).expect("Failed to parse manifest YAML");

        // Verify catalog
        assert_eq!(parsed_manifest.catalogs.len(), 1);
        let catalog = parsed_manifest.catalogs.get("v1").expect("Catalog v1 not found");
        assert_eq!(
            catalog.description,
            Some("Production catalog".to_string())
        );
        assert_eq!(catalog.catalog_path, "billing/v1");

        // Verify payment
        assert_eq!(parsed_manifest.payments.len(), 1);
        let payment = parsed_manifest
            .payments
            .get("stablecoins")
            .expect("Payment config not found");

        // Extract X402 config (only variant available currently)
        let PaymentConfig::X402(x402) = payment;

        assert_eq!(
            x402.description,
            Some("Solana stablecoin payments".to_string())
        );

        // Verify facilitator
        match &x402.facilitator {
            FacilitatorConfig::ServiceUrl { service_url } => {
                assert_eq!(service_url, "https://facilitator.moneymq.co");
            }
            _ => panic!("Expected ServiceUrl facilitator"),
        }

        // Verify accepted networks
        assert_eq!(x402.accepted.len(), 1);
        let accepted = x402
            .accepted
            .get(&NetworkIdentifier::Solana)
            .expect("Solana network not found");
        assert_eq!(accepted.currencies, vec!["USDC", "USDT"]);

        // Verify sandbox
        assert_eq!(x402.sandboxes.len(), 1);
        let sandbox = x402.sandboxes.get("default").expect("Default sandbox not found");
        assert_eq!(
            sandbox.description,
            Some("Local development sandbox".to_string())
        );

        // Verify sandbox facilitator is embedded
        match &sandbox.facilitator {
            FacilitatorConfig::Embedded(config) => {
                assert_eq!(config.binding_port, 8080);
                assert_eq!(config.supported.len(), 1);
            }
            _ => panic!("Expected Embedded facilitator in sandbox"),
        }

        // Print the generated YAML for manual inspection
        println!("Generated YAML:\n{}", yaml_content);
    }

    #[test]
    fn test_manifest_save_with_payments_footer() {
        // Create a manifest with only catalogs (no payments)
        let mut manifest = Manifest {
            catalogs: IndexMap::new(),
            payments: IndexMap::new(),
        };

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

        manifest.save(&manifest_path).expect("Failed to save manifest");

        // Read back the content
        let content = fs::read_to_string(&manifest_path).expect("Failed to read manifest");

        // Verify it has the header
        assert!(content.starts_with("---\n"));
        assert!(content.contains("# MoneyMQ Manifest - API version v1"));

        // Verify it has the catalog
        assert!(content.contains("catalogs:"));
        assert!(content.contains("v1:"));
        assert!(content.contains("description: Test catalog"));

        // Verify it has the payments footer (commented out)
        assert!(content.contains("# Payment configuration for accepting crypto payments"));
        assert!(content.contains("# payments:"));
        assert!(content.contains("#   stablecoins:"));
        assert!(content.contains("#     protocol: x402"));
        assert!(content.contains("# Learn more: https://docs.moneymq.co/payments"));

        println!("Generated manifest:\n{}", content);
    }

    #[test]
    fn test_manifest_save_without_footer_when_payments_exist() {
        // Create a manifest with payments configured
        let mut manifest = Manifest {
            catalogs: IndexMap::new(),
            payments: IndexMap::new(),
        };

        manifest.catalogs.insert(
            "v1".to_string(),
            CatalogConfig {
                description: Some("Test catalog".to_string()),
                catalog_path: "billing/v1".to_string(),
                source: None,
            },
        );

        manifest.payments.insert("stablecoins".to_string(), PaymentConfig::default());

        // Save to temp file
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let manifest_path = temp_dir.path().join("moneymq.yaml");

        manifest.save(&manifest_path).expect("Failed to save manifest");

        // Read back the content
        let content = fs::read_to_string(&manifest_path).expect("Failed to read manifest");

        // Verify it has the actual payments section (not commented)
        assert!(content.contains("payments:"));
        assert!(content.contains("stablecoins:"));

        // Verify it does NOT have the commented footer
        assert!(!content.contains("# Payment configuration for accepting crypto payments"));

        println!("Generated manifest with payments:\n{}", content);
    }
}
