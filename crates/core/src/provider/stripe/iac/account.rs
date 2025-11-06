use anyhow::Result;

/// Information about a Stripe account
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub business_name: Option<String>,
    pub display_name: Option<String>,
    pub account_id: String,
    pub is_test: bool,
    pub image_url: Option<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
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

    let branding = account_json
        .get("settings")
        .and_then(|s| s.get("branding"));

    // Get icon URL from file ID
    let image_url = if let Some(icon_id) = branding
        .and_then(|b| b.get("icon"))
        .and_then(|i| i.as_str())
    {
        get_file_url(&http_client, api_key, icon_id).await
    } else {
        None
    };

    // Get logo URL from file ID
    let logo_url = if let Some(logo_id) = branding
        .and_then(|b| b.get("logo"))
        .and_then(|l| l.as_str())
    {
        get_file_url(&http_client, api_key, logo_id).await
    } else {
        None
    };

    let primary_color = branding
        .and_then(|b| b.get("primary_color"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    let secondary_color = branding
        .and_then(|b| b.get("secondary_color"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    Ok(AccountInfo {
        business_name,
        display_name,
        account_id,
        is_test,
        image_url,
        logo_url,
        primary_color,
        secondary_color,
    })
}

/// Helper function to retrieve file URL from Stripe file ID
/// Returns a publicly accessible URL that doesn't require authentication
async fn get_file_url(
    http_client: &reqwest::Client,
    api_key: &str,
    file_id: &str,
) -> Option<String> {
    let response = http_client
        .get(format!("https://api.stripe.com/v1/files/{}", file_id))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let file_json: serde_json::Value = response.json().await.ok()?;

    // Check if there's already a public file link
    if let Some(link_url) = file_json
        .get("links")
        .and_then(|l| l.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|link| link.get("url"))
        .and_then(|u| u.as_str())
    {
        return Some(link_url.to_string());
    }

    // If no public link exists, create one
    let create_response = http_client
        .post("https://api.stripe.com/v1/file_links")
        .header("Authorization", format!("Bearer {}", api_key))
        .form(&[("file", file_id)])
        .send()
        .await
        .ok()?;

    if !create_response.status().is_success() {
        return None;
    }

    let link_json: serde_json::Value = create_response.json().await.ok()?;

    link_json
        .get("url")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string())
}
