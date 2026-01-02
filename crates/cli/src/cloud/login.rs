//! Login flow for MoneyMQ Cloud
//!
//! Supports three authentication methods:
//! 1. Browser-based OAuth flow (default)
//! 2. Email/password authentication
//! 3. Personal Access Token (PAT)

use std::{collections::HashMap, net::TcpListener};

use axum::{
    Router,
    extract::{Query, State},
    response::{Html, IntoResponse},
    routing::get,
};
use console::style;
use dialoguer::{Confirm, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    LoginCommand,
    auth::{AuthConfig, AuthUser},
};

/// Login callback result from OAuth flow
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCallbackResult {
    pub access_token: String,
    pub exp: u64,
    pub refresh_token: String,
    pub pat: String,
    pub user: AuthUser,
}

/// Login response from email/password or PAT login
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub session: Session,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    pub user: AuthUser,
}

/// Main login handler
pub async fn handle_login_command(
    cmd: &LoginCommand,
    auth_service_url: &str,
    auth_callback_port: &str,
    id_service_url: &str,
) -> Result<(), String> {
    let auth_config = AuthConfig::read_from_system_config()?;

    // Check if already logged in
    if let Some(mut auth_config) = auth_config {
        if auth_config.is_access_token_expired() {
            match auth_config.refresh_session_if_needed(id_service_url).await {
                Ok(()) => {
                    println!(
                        "{} Logged in as {}.",
                        style("✓").green(),
                        auth_config.user.display_name
                    );
                    return Ok(());
                }
                Err(_e) => {
                    // Try PAT login if available
                    if let Some(pat) = &auth_config.pat
                        && let Ok(auth_config) = pat_login(id_service_url, pat).await
                    {
                        auth_config.write_to_system_config()?;
                        println!(
                            "{} Logged in as {}.",
                            style("✓").green(),
                            auth_config.user.display_name
                        );
                        return Ok(());
                    }
                    println!(
                        "{} Session expired, attempting login...",
                        style("-").yellow()
                    );
                }
            }
        } else {
            println!(
                "{} Logged in as {}.",
                style("✓").green(),
                auth_config.user.display_name
            );
            return Ok(());
        }
    }

    // Perform login based on provided credentials
    let auth_config = if let Some(email) = &cmd.email {
        let password = cmd
            .password
            .as_ref()
            .ok_or("Password is required when email is provided")?;
        user_pass_login(id_service_url, email, password).await?
    } else if let Some(pat) = &cmd.pat {
        pat_login(id_service_url, pat).await?
    } else {
        // Browser-based OAuth flow
        let Some(res) = auth_service_login(auth_service_url, auth_callback_port).await? else {
            return Ok(());
        };
        AuthConfig::new(
            res.access_token,
            res.exp,
            res.refresh_token,
            Some(res.pat),
            res.user,
        )
    };

    auth_config.write_to_system_config()?;
    println!(
        "{} Logged in as {}.",
        style("✓").green(),
        auth_config.user.display_name
    );
    Ok(())
}

/// Browser-based OAuth login flow
async fn auth_service_login(
    auth_service_url: &str,
    auth_callback_port: &str,
) -> Result<Option<LoginCallbackResult>, String> {
    let redirect_url = format!("http://localhost:{}/api/v1/auth", auth_callback_port);
    let encoded_redirect_url: String =
        url::form_urlencoded::byte_serialize(redirect_url.as_bytes()).collect();

    let auth_url = format!("{}/?redirectUrl={}", auth_service_url, encoded_redirect_url);

    // Create channel for receiving callback
    let (tx, mut rx) = mpsc::channel::<LoginCallbackResult>(1);

    // Find available port
    let bind_addr = format!("localhost:{}", auth_callback_port);
    let listener = TcpListener::bind(&bind_addr)
        .map_err(|e| format!("Failed to bind to port {}: {}", auth_callback_port, e))?;

    // Build the router
    let app = Router::new()
        .route("/api/v1/auth", get(auth_callback))
        .with_state(tx);

    // Ask user to open browser
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Open {} in your browser to log in?", auth_url))
        .default(true)
        .interact();

    let Ok(true) = confirm else {
        println!("\nLogin cancelled");
        return Ok(None);
    };

    // Start server
    let server = axum::serve(
        tokio::net::TcpListener::from_std(listener)
            .map_err(|e| format!("Failed to create listener: {}", e))?,
        app,
    );

    // Open browser
    if open::that(&auth_url).is_err() {
        println!(
            "Failed to automatically open your browser. Please open the following URL: {}",
            auth_url
        );
    }

    // Wait for callback with timeout
    let result = tokio::select! {
        res = rx.recv() => {
            res.ok_or_else(|| "Failed to receive auth callback".to_string())
        }
        _ = server => {
            Err("Server stopped unexpectedly".to_string())
        }
    }?;

    Ok(Some(result))
}

/// Handler for OAuth callback
async fn auth_callback(
    State(tx): State<mpsc::Sender<LoginCallbackResult>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // The callback comes as base64-encoded query string
    // Try to decode and parse
    let result = (|| -> Result<LoginCallbackResult, String> {
        // Check if we have the data directly in params
        if let (Some(access_token), Some(exp), Some(refresh_token), Some(pat), Some(user_json)) = (
            params.get("accessToken"),
            params.get("exp"),
            params.get("refreshToken"),
            params.get("pat"),
            params.get("user"),
        ) {
            let user: AuthUser = serde_json::from_str(user_json)
                .map_err(|e| format!("Failed to parse user: {}", e))?;
            return Ok(LoginCallbackResult {
                access_token: access_token.clone(),
                exp: exp.parse().unwrap_or(0),
                refresh_token: refresh_token.clone(),
                pat: pat.clone(),
                user,
            });
        }

        // Try base64 decoding the entire query string
        if let Some(encoded) = params.keys().next()
            && let Ok(decoded) =
                base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE, encoded)
            && let Ok(decoded_str) = String::from_utf8(decoded)
        {
            let mut inner_params: HashMap<String, String> =
                serde_urlencoded::from_str(&decoded_str)
                    .map_err(|e| format!("Failed to parse params: {}", e))?;

            if let Some(user_json) = inner_params.remove("user") {
                let user: AuthUser = serde_json::from_str(&user_json)
                    .map_err(|e| format!("Failed to parse user: {}", e))?;
                return Ok(LoginCallbackResult {
                    access_token: inner_params.remove("accessToken").unwrap_or_default(),
                    exp: inner_params
                        .remove("exp")
                        .unwrap_or_default()
                        .parse()
                        .unwrap_or(0),
                    refresh_token: inner_params.remove("refreshToken").unwrap_or_default(),
                    pat: inner_params.remove("pat").unwrap_or_default(),
                    user,
                });
            }
        }

        Err("Invalid callback data".to_string())
    })();

    match result {
        Ok(login_result) => {
            let _ = tx.send(login_result).await;
            Html(include_str!("callback.html").to_string())
        }
        Err(e) => Html(format!(
            "<html><body><h1>Authentication failed</h1><p>{}</p></body></html>",
            e
        )),
    }
}

/// Email/password login
async fn user_pass_login(
    id_service_url: &str,
    email: &str,
    password: &str,
) -> Result<AuthConfig, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/signin/email-password", id_service_url))
        .json(&serde_json::json!({
            "email": email,
            "password": password,
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send login request: {}", e))?;

    if res.status().is_success() {
        let res = res
            .json::<LoginResponse>()
            .await
            .map_err(|e| format!("Failed to parse login response: {}", e))?;

        // Extract exp from JWT payload without validation (trust the auth service)
        let exp = extract_jwt_exp(&res.session.access_token).unwrap_or(0);

        Ok(AuthConfig::new(
            res.session.access_token,
            exp,
            res.session.refresh_token,
            None,
            res.session.user,
        ))
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Login failed: {}", err))
    }
}

/// Personal Access Token login
pub async fn pat_login(id_service_url: &str, pat: &str) -> Result<AuthConfig, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/signin/pat", id_service_url))
        .json(&serde_json::json!({
            "personalAccessToken": pat,
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send PAT login request: {}", e))?;

    if res.status().is_success() {
        let res = res
            .json::<LoginResponse>()
            .await
            .map_err(|e| format!("Failed to parse PAT login response: {}", e))?;

        // Extract exp from JWT payload without validation (trust the auth service)
        let exp = extract_jwt_exp(&res.session.access_token).unwrap_or(0);

        Ok(AuthConfig::new(
            res.session.access_token,
            exp,
            res.session.refresh_token,
            Some(pat.to_string()),
            res.session.user,
        ))
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("PAT login failed: {}", err))
    }
}

/// Extract exp claim from JWT without validation
fn extract_jwt_exp(token: &str) -> Option<u64> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    let payload =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1]).ok()?;

    let payload_str = String::from_utf8(payload).ok()?;
    let payload_json: serde_json::Value = serde_json::from_str(&payload_str).ok()?;
    payload_json.get("exp")?.as_u64()
}
