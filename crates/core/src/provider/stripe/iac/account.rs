use anyhow::Result;

/// Information about a Stripe account
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub business_name: Option<String>,
    pub display_name: Option<String>,
    pub account_id: String,
    pub is_test: bool,
}

/// Retrieve account information from Stripe
///
/// # Arguments
/// * `api_key` - Your Stripe secret API key
///
/// # Returns
/// `AccountInfo` containing account details
///
/// # Example
/// ```no_run
/// use moneymq_core::provider::stripe::iac::account::get_account_info;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let api_key = std::env::var("STRIPE_SECRET_KEY")?;
///     let account_info = get_account_info(&api_key).await?;
///     println!("Account: {}", account_info.display_name.unwrap_or_default());
///     Ok(())
/// }
/// ```
pub async fn get_account_info(api_key: &str) -> Result<AccountInfo> {
    // Use the account endpoint directly via HTTP
    let http_client = reqwest::Client::new();
    let response = http_client
        .get("https://api.stripe.com/v1/account")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to fetch account info ({}): {}",
            status,
            error_body
        ));
    }

    let account_json: serde_json::Value = response.json().await?;
    let is_test = api_key.starts_with("sk_test_") || api_key.starts_with("rk_test_");

    // Extract account information from JSON
    let business_name = account_json
        .get("business_profile")
        .and_then(|bp| bp.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let display_name = account_json
        .get("settings")
        .and_then(|s| s.get("dashboard"))
        .and_then(|d| d.get("display_name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let account_id = account_json
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(AccountInfo {
        business_name,
        display_name,
        account_id,
        is_test,
    })
}
