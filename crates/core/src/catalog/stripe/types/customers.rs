use serde::{Deserialize, Serialize};

/// Request body for creating a customer
#[derive(Debug, Deserialize)]
pub struct CreateCustomerRequest {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// Stripe-compatible customer response
#[derive(Debug, Serialize)]
pub struct StripeCustomer {
    pub id: String,
    pub object: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}
