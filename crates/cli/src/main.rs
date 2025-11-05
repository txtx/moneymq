use std::{
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, Subcommand};

mod catalog;
mod init;
mod manifest;
mod run;
mod yaml_util;

use manifest::Manifest;

#[derive(Clone, Debug)]
pub struct Context {
    pub manifest_path: PathBuf,
    pub manifest: Manifest,
    pub provider: String,
    pub use_sandbox: bool,
}

impl Context {
    pub fn new(
        manifest_path: PathBuf,
        manifest: Manifest,
        provider: String,
        use_sandbox: bool,
    ) -> Self {
        Context {
            manifest_path,
            manifest,
            provider,
            use_sandbox,
        }
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about = "MoneyMQ - Payment gateway management CLI", long_about = None)]
struct Opts {
    /// Path to the billing.yaml manifest file (default: ./billing.yaml)
    #[arg(
        long = "manifest-path",
        short = 'm',
        global = true,
        default_value = "./billing.yaml"
    )]
    manifest_path: PathBuf,

    /// Provider configuration to use (e.g., "stripe", "stripe_sandbox")
    /// If not specified, uses the first provider found in billing.yaml
    #[arg(long = "provider", short = 'p', global = true)]
    provider: Option<String>,

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
    Run(run::RunCommand),
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

    // Load manifest from billing.yaml file (skip for init command)
    let manifest = if is_init_command {
        Manifest::default()
    } else {
        match Manifest::load(&opts.manifest_path) {
            Ok(manifest) => {
                eprintln!("✓ Loaded manifest from {}", opts.manifest_path.display());
                manifest
            }
            Err(e) => {
                eprintln!("Warning: {}", e);
                eprintln!("Using default configuration...");
                Manifest::default()
            }
        }
    };

    // Determine provider name: use specified provider or auto-detect first provider from manifest
    let provider_name = if let Some(ref p) = opts.provider {
        p.clone()
    } else {
        // Auto-detect first provider from manifest
        manifest
            .providers
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "stripe".to_string())
    };

    // Create context with manifest directory, loaded manifest, selected provider, and sandbox flag
    let ctx = Context::new(manifest_dir, manifest, provider_name, opts.sandbox);

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
        Ok(_) => {
            eprintln!("✓ Loaded environment from {}", env_file_path.display());
        }
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
    }
}

async fn handle_catalog_commands(cmd: CatalogCommand, ctx: &Context) -> Result<(), String> {
    match cmd {
        CatalogCommand::Fetch(cmd) => cmd.execute(ctx).await,
        CatalogCommand::Sync(cmd) => cmd.execute(ctx).await,
    }
}
