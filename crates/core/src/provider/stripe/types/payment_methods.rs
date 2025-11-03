use serde::{Deserialize, Serialize};

/// Request body for creating a payment method
#[derive(Debug, Deserialize)]
pub struct CreatePaymentMethodRequest {
    #[serde(rename = "type")]
    pub payment_type: String,
    #[serde(default)]
    pub card: Option<CardDetails>,
}

#[derive(Debug, Deserialize)]
pub struct CardDetails {
    #[serde(default)]
    pub token: Option<String>,
}

/// Stripe-compatible payment method response
#[derive(Debug, Serialize)]
pub struct StripePaymentMethod {
    pub id: String,
    pub object: String,
    #[serde(rename = "type")]
    pub payment_type: String,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<StripeCard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StripeCard {
    pub brand: String,
    pub last4: String,
    pub exp_month: i32,
    pub exp_year: i32,
}

/// Request body for attaching a payment method
#[derive(Debug, Deserialize)]
pub struct AttachPaymentMethodRequest {
    pub customer: String,
}
