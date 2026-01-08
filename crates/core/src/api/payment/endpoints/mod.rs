use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

pub mod admin;
pub mod channels;
pub mod health;
pub mod jwt;
pub mod settle;
pub mod supported;
pub mod verify;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilitatorExtraContext {
    pub fee_payer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Unique transaction ID for channel-based event routing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// Product features (capabilities and limits from the purchased product)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<serde_json::Value>,
}

pub fn serialize_to_base64<T: serde::Serialize>(data: &T) -> String {
    let json = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    BASE64.encode(&json)
}
