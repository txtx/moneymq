//! Sandbox command - shorthand for `moneymq run sandbox`
//!
//! This module provides the `moneymq sandbox` command which is equivalent to
//! running `moneymq run sandbox`. It starts a local development environment
//! with an embedded Solana validator.

use console::style;
use moneymq_types::Product;

use crate::{
    Context,
    catalog::examples::load_weather_example,
    service::{RunCommand, RunCommandError, ServiceCommand},
};

/// Sandbox command - starts MoneyMQ with the "sandbox" environment.
///
/// This is a convenience shorthand for `moneymq run sandbox`.
///
/// # Example
///
/// ```bash
/// # These are equivalent:
/// moneymq sandbox
/// moneymq run sandbox
/// moneymq sandbox --port 9000
/// moneymq run sandbox --port 9000
///
/// # Load the Weather API example catalog:
/// moneymq sandbox --weather-example
/// ```
#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct SandboxCommand {
    /// Port to run the server on (overrides environment config)
    #[arg(long)]
    pub port: Option<u16>,

    /// Log level (error, warn, info, debug, trace). If not set, logging is disabled.
    #[arg(long)]
    pub log_level: Option<String>,

    /// Load the Weather API example catalog (starter, pro, enterprise tiers)
    #[arg(long)]
    pub weather_example: bool,
}

impl SandboxCommand {
    /// Convert to RunCommand with "sandbox" environment
    fn to_run_command(&self) -> RunCommand {
        RunCommand {
            environment: "sandbox".to_string(),
            port: self.port,
            log_level: self.log_level.clone(),
        }
    }

    /// Load the example catalog if --weather-example is set
    pub fn load_example_catalog(&self) -> Option<Vec<Product>> {
        if self.weather_example {
            match load_weather_example() {
                Ok(products) => {
                    println!(
                        "{} {} Weather API example (starter, pro, enterprise)",
                        style("Loading").dim(),
                        style("✓").green()
                    );
                    Some(products)
                }
                Err(e) => {
                    eprintln!("{} Failed to load weather example: {}", style("✗").red(), e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Execute the sandbox command
    pub async fn execute(&self, ctx: &Context) -> Result<(), RunCommandError> {
        let run_cmd = self.to_run_command();
        let example_products = self.load_example_catalog();
        ServiceCommand::execute_with_products(&run_cmd, ctx, example_products).await
    }
}
