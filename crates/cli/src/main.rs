use std::{
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, Subcommand};
use console::style;
use moneymq_mcp::{McpOptions, run_server};

mod catalog;
mod init;
mod manifest;
mod service;
mod yaml_util;

use manifest::Manifest;

use crate::{
    manifest::{CatalogConfig, x402::PaymentConfig},
    service::ServiceCommand,
};

#[derive(Clone, Debug)]
pub struct Context {
    pub manifest_path: PathBuf,
    pub manifest: Manifest,
    pub catalog_name: String,
    pub network_name: String,
    pub use_sandbox: bool,
    pub is_default_manifest: bool,
}

impl Context {
    pub fn new(
        manifest_path: PathBuf,
        manifest: Manifest,
        catalog_name: String,
        network_name: String,
        use_sandbox: bool,
        is_default_manifest: bool,
    ) -> Self {
        Context {
            manifest_path,
            manifest,
            catalog_name,
            network_name,
            use_sandbox,
            is_default_manifest,
        }
    }

    pub fn get_catalog(&self) -> Option<&CatalogConfig> {
        self.manifest.get_catalog(&self.catalog_name)
    }

    pub fn get_payment(&self) -> Option<&PaymentConfig> {
        self.manifest.get_payment(&self.network_name)
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about = "MoneyMQ - Payment gateway management CLI", long_about = None)]
struct Opts {
    /// Path to the manifest file (default: ./moneymq.yaml)
    #[arg(
        long = "manifest-path",
        short = 'm',
        global = true,
        default_value = "./moneymq.yaml"
    )]
    manifest_path: PathBuf,

    /// Catalog configuration to use (e.g., "v1", etc)
    /// If not specified, uses the first catalog found in manifest
    #[arg(long = "catalog", short = 'c', global = true)]
    catalog: Option<String>,

    /// Network configuration to use (e.g., "x402", etc)
    /// If not specified, uses the first network found in manifest
    #[arg(long = "network", short = 'n', global = true)]
    network: Option<String>,

    /// Use the sandbox provider configuration referenced in the main provider
    #[arg(long = "sandbox", short = 's', global = true, default_value = "false")]
    sandbox: bool,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
enum Command {
    /// Initialize MoneyMQ with your payment provider
    Init(init::InitCommand),
    /// Catalog management commands
    Catalog {
        #[clap(subcommand)]
        command: CatalogCommand,
    },
    /// Start the local provider server
    Run(service::RunCommand),
    Sandbox(service::SandboxCommand),
    Mcp,
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
pub enum CatalogCommand {
    /// Fetch catalog from Stripe
    Fetch(catalog::FetchCommand),
    /// Sync production catalog to disk as YAML files
    Sync(catalog::SyncCommand),
}

#[tokio::main]
async fn main() {
    let opts: Opts = match Opts::try_parse() {
        Ok(opts) => opts,
        Err(e) => {
            let _ = e.print();
            process::exit(e.exit_code());
        }
    };

    // Get the directory containing the manifest file
    let manifest_dir = opts
        .manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // Skip environment and manifest loading for init command
    let is_init_command = matches!(opts.command, Command::Init(_));

    if !is_init_command {
        // Load environment variables from .env file in manifest directory
        load_env_file(&manifest_dir);
    }

    // Load manifest from file (skip for init command)
    let (manifest, is_default_manifest) = if is_init_command {
        (Manifest::default(), true)
    } else {
        match Manifest::load(&opts.manifest_path) {
            Ok(manifest) => (manifest, false),
            Err(e) => {
                // If there's no manifest file and the user is running the sandbox command, suppress the warning
                // to let the user have a nice "out of the box" experience
                if !(matches!(e, manifest::LoadManifestError::FileNotFound(_))
                    && matches!(opts.command, Command::Sandbox(_)))
                {
                    println!(
                        "{}: using default configuration ({})",
                        style("warning:").yellow(),
                        e
                    );
                    println!();
                }
                (Manifest::default(), true)
            }
        }
    };

    // Determine catalog: use specified catalog or auto-detect first catalog from manifest
    let catalog_name = if let Some(ref c) = opts.catalog {
        c.clone()
    } else {
        // Auto-detect first catalog from manifest
        manifest
            .catalogs
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "v1".to_string())
    };

    // Determine network: use specified catalog or auto-detect first catalog from manifest
    let network_name = if let Some(ref c) = opts.catalog {
        c.clone()
    } else {
        // Auto-detect first catalog from manifest
        manifest
            .payments
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "v1".to_string())
    };

    let sandbox = if matches!(opts.command, Command::Sandbox(_)) {
        true
    } else {
        opts.sandbox
    };

    // Create context with manifest directory, loaded manifest, selected provider, and sandbox flag
    let ctx = Context::new(
        manifest_dir,
        manifest,
        catalog_name,
        network_name,
        sandbox,
        is_default_manifest,
    );

    if let Err(e) = handle_command(opts, &ctx).await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

/// Load environment variables from .env file in the manifest directory
fn load_env_file(manifest_path: &Path) {
    // Construct path to .env file in the manifest directory
    let env_file_path = manifest_path.join(".env");

    match dotenvy::from_path(&env_file_path) {
        Ok(_) => {}
        Err(e) if e.not_found() => {
            // .env file not found is fine, just continue silently
        }
        Err(e) => {
            eprintln!(
                "Warning: Failed to load .env file at {}: {}",
                env_file_path.display(),
                e
            );
        }
    }
}

async fn handle_command(opts: Opts, ctx: &Context) -> Result<(), String> {
    match opts.command {
        Command::Init(cmd) => cmd.execute(ctx).await,
        Command::Catalog { command } => handle_catalog_commands(command, ctx).await,
        Command::Run(cmd) => cmd.execute(ctx).await.map_err(|e| e.to_string()),
        Command::Sandbox(cmd) => cmd.execute(ctx).await.map_err(|e| e.to_string()),
        Command::Mcp => {
            let mcp_opts = McpOptions::default();
            run_server(&mcp_opts).await
        }
    }
}

async fn handle_catalog_commands(cmd: CatalogCommand, ctx: &Context) -> Result<(), String> {
    match cmd {
        CatalogCommand::Fetch(cmd) => cmd.execute(ctx).await,
        CatalogCommand::Sync(cmd) => cmd.execute(ctx).await,
    }
}
