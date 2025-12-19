//! Sandbox command - shorthand for `moneymq run sandbox`
//!
//! This module provides the `moneymq sandbox` command which is equivalent to
//! running `moneymq run sandbox`. It starts a local development environment
//! with an embedded Solana validator.

use crate::{
    Context,
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
/// ```
#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct SandboxCommand {
    /// Port to run the server on (overrides environment config)
    #[arg(long)]
    pub port: Option<u16>,
}

impl SandboxCommand {
    /// Convert to RunCommand with "sandbox" environment
    fn to_run_command(&self) -> RunCommand {
        RunCommand {
            environment: "sandbox".to_string(),
            port: self.port,
        }
    }

    /// Execute the sandbox command
    pub async fn execute(&self, ctx: &Context) -> Result<(), RunCommandError> {
        let run_cmd = self.to_run_command();
        ServiceCommand::execute(&run_cmd, ctx).await
    }
}
