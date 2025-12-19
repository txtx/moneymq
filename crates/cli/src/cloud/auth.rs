//! Authentication configuration for MoneyMQ Cloud
//!
//! Stores authentication tokens in the system data directory.

use std::io::{Read, Write};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// Authentication configuration stored on disk
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub access_token: String,
    pub exp: u64,
    pub refresh_token: String,
    pub pat: Option<String>,
    pub user: AuthUser,
}

/// User information from authentication
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: String,
    pub email: Option<String>,
    pub display_name: String,
}

/// Response from token refresh endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshSessionResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: AuthUser,
}

impl AuthConfig {
    pub fn new(
        access_token: String,
        exp: u64,
        refresh_token: String,
        pat: Option<String>,
        user: AuthUser,
    ) -> Self {
        Self {
            access_token,
            exp,
            refresh_token,
            pat,
            user,
        }
    }

    /// Write auth config to system data directory.
    pub fn write_to_system_config(&self) -> Result<(), String> {
        let data_dir = dirs::data_dir().ok_or("Failed to get system data directory")?;

        std::fs::create_dir_all(data_dir.join("moneymq"))
            .map_err(|e| format!("Failed to create data directory: {}", e))?;

        let path = data_dir.join("moneymq/auth.toml");

        let mut file = std::fs::File::create(&path)
            .map_err(|e| format!("Failed to create config file: {}", e))?;

        let toml = toml::to_string(&self)
            .map_err(|e| format!("Failed to serialize auth config: {}", e))?;

        file.write_all(toml.as_bytes())
            .map_err(|e| format!("Failed to write auth config: {}", e))?;
        Ok(())
    }

    /// Read auth config from system data directory.
    pub fn read_from_system_config() -> Result<Option<Self>, String> {
        let data_dir = dirs::data_dir().ok_or("Failed to get system data directory")?;
        let path = data_dir.join("moneymq/auth.toml");

        if !path.exists() {
            return Ok(None);
        }

        let mut file =
            std::fs::File::open(&path).map_err(|e| format!("Failed to open config file: {}", e))?;
        let mut buf = String::new();

        file.read_to_string(&mut buf)
            .map_err(|e| format!("Failed to read config file: {}", e))?;

        let config =
            toml::from_str(&buf).map_err(|e| format!("Failed to parse auth config file: {}", e))?;
        Ok(Some(config))
    }

    /// Delete auth config from system data directory.
    pub fn delete_from_system_config() -> Result<(), String> {
        let data_dir = dirs::data_dir().ok_or("Failed to get system data directory")?;
        let path = data_dir.join("moneymq/auth.toml");

        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete config file: {}", e))?;
        }
        Ok(())
    }

    /// Check if the access token is expired.
    pub fn is_access_token_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH")
            .as_secs();

        self.exp < now
    }

    /// Refresh the session if the access token is expired.
    pub async fn refresh_session_if_needed(&mut self, id_service_url: &str) -> Result<(), String> {
        if self.is_access_token_expired() {
            let refreshed_auth_config = self.get_refreshed_session(id_service_url).await.map_err(|e| {
                format!("Failed to refresh session. Run `moneymq cloud login` to log in again. Error: {e}")
            })?;
            self.access_token = refreshed_auth_config.access_token;
            self.exp = refreshed_auth_config.exp;
            self.refresh_token = refreshed_auth_config.refresh_token;
            self.write_to_system_config()
                .map_err(|e| format!("Failed to write refreshed session to config: {}", e))?;
        }
        Ok(())
    }

    /// Get a new access token using the refresh token.
    async fn get_refreshed_session(&self, id_service_url: &str) -> Result<AuthConfig, String> {
        let client = reqwest::Client::new();
        let res = client
            .post(&format!("{id_service_url}/token"))
            .json(&serde_json::json!({
                "refreshToken": &self.refresh_token,
            }))
            .send()
            .await
            .map_err(|e| format!("Failed to send request to refresh session: {}", e))?;

        if res.status().is_success() {
            let res = res
                .json::<RefreshSessionResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;

            let auth_config = AuthConfig::from_refresh_session_response(&res, &self.pat)?;

            auth_config.write_to_system_config()?;
            return Ok(auth_config);
        } else {
            let err = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!(
                "Received error from refresh session request: {}",
                err
            ));
        }
    }

    fn from_refresh_session_response(
        response: &RefreshSessionResponse,
        pat: &Option<String>,
    ) -> Result<Self, String> {
        // Extract exp from JWT payload without validation (trust the auth service)
        let exp = extract_jwt_exp(&response.access_token).unwrap_or(0);

        Ok(Self {
            access_token: response.access_token.clone(),
            exp,
            refresh_token: response.refresh_token.clone(),
            pat: pat.clone(),
            user: response.user.clone(),
        })
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
