use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

pub mod admin;
pub mod health;
pub mod middleware;
pub mod sandbox;
pub mod settle;
pub mod supported;
pub mod verify;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacilitatorExtraContext {
    pub fee_payer: String,
    pub product: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

pub fn serialize_to_base64<T: serde::Serialize>(data: &T) -> String {
    let json = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    BASE64.encode(&json)
}
