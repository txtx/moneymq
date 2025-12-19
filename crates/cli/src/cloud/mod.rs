//! Cloud module for MoneyMQ CLI
//!
//! Handles authentication and interaction with MoneyMQ cloud services.

pub mod auth;
pub mod login;

use clap::{Parser, Subcommand};

use crate::Context;

/// Cloud service URLs (same as surfpool - shared txtx cloud infrastructure)
pub const AUTH_SERVICE_URL: &str = "https://cloud.money.mq";
pub const AUTH_CALLBACK_PORT: &str = "17422";
pub const ID_SERVICE_URL: &str = "https://id.txtx.run/v1";

#[derive(Parser, PartialEq, Clone, Debug)]
pub struct CloudCommand {
    #[clap(subcommand)]
    pub command: CloudSubcommand,
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
pub enum CloudSubcommand {
    /// Log in to MoneyMQ Cloud
    Login(LoginCommand),
    /// Log out of MoneyMQ Cloud
    Logout,
    /// Show current login status
    Status,
}

#[derive(Parser, PartialEq, Clone, Debug, Default)]
pub struct LoginCommand {
    /// Email address for authentication
    #[arg(
        long = "email",
        short = 'e',
        requires = "password",
        conflicts_with = "pat"
    )]
    pub email: Option<String>,

    /// Password for authentication
    #[arg(
        long = "password",
        short = 'p',
        requires = "email",
        conflicts_with = "pat"
    )]
    pub password: Option<String>,

    /// Personal Access Token for non-interactive login
    #[arg(long = "pat", conflicts_with_all = &["email", "password"])]
    pub pat: Option<String>,
}

impl CloudCommand {
    pub async fn execute(&self, _ctx: &Context) -> Result<(), String> {
        match &self.command {
            CloudSubcommand::Login(cmd) => {
                login::handle_login_command(
                    cmd,
                    AUTH_SERVICE_URL,
                    AUTH_CALLBACK_PORT,
                    ID_SERVICE_URL,
                )
                .await
            }
            CloudSubcommand::Logout => {
                auth::AuthConfig::delete_from_system_config()?;
                println!("Logged out successfully.");
                Ok(())
            }
            CloudSubcommand::Status => {
                match auth::AuthConfig::read_from_system_config()? {
                    Some(config) => {
                        println!("Logged in as: {}", config.user.display_name);
                        if let Some(email) = &config.user.email {
                            println!("Email: {}", email);
                        }
                        if config.is_access_token_expired() {
                            println!("Status: Session expired (will refresh on next command)");
                        } else {
                            println!("Status: Active");
                        }
                    }
                    None => {
                        println!("Not logged in. Run `moneymq cloud login` to authenticate.");
                    }
                }
                Ok(())
            }
        }
    }
}
