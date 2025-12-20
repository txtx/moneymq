use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Stripe-compatible checkout session response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeCheckoutSession {
    pub id: String,
    pub object: String,
    pub status: CheckoutSessionStatus,
    pub payment_status: PaymentStatus,
    pub currency: String,
    pub amount_total: i64,
    pub amount_subtotal: i64,
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_intent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    pub line_items: CheckoutLineItemList,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_url: Option<String>,
}

/// Checkout session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutSessionStatus {
    Open,
    Complete,
    Expired,
}

/// Payment status within a checkout session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Unpaid,
    Paid,
    NoPaymentRequired,
}

/// Line items list wrapper (Stripe returns this as a list object)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutLineItemList {
    pub object: String,
    pub data: Vec<CheckoutLineItem>,
    pub has_more: bool,
    pub url: String,
}

impl Default for CheckoutLineItemList {
    fn default() -> Self {
        Self {
            object: "list".to_string(),
            data: Vec::new(),
            has_more: false,
            url: "/v1/checkout/sessions/{CHECKOUT_SESSION_ID}/line_items".to_string(),
        }
    }
}

/// Individual line item in a checkout session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutLineItem {
    pub id: String,
    pub object: String,
    pub amount_subtotal: i64,
    pub amount_total: i64,
    pub currency: String,
    pub quantity: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub price: CheckoutLineItemPrice,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_discount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_tax: Option<i64>,
}

/// Price information within a line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutLineItemPrice {
    pub id: String,
    pub object: String,
    pub currency: String,
    pub unit_amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(rename = "type")]
    pub price_type: String,
}

/// Create checkout session request
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCheckoutSessionRequest {
    pub line_items: Vec<CreateLineItem>,
    #[serde(default)]
    pub customer: Option<String>,
    #[serde(default)]
    pub customer_email: Option<String>,
    #[serde(default)]
    pub success_url: Option<String>,
    #[serde(default)]
    pub cancel_url: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "payment".to_string()
}

/// Line item in create request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateLineItem {
    /// Price ID (if using existing price)
    #[serde(default)]
    pub price: Option<String>,
    /// Quantity of the item
    #[serde(default = "default_quantity")]
    pub quantity: i64,
    /// Inline price data (if not using existing price)
    #[serde(default)]
    pub price_data: Option<CreateLineItemPriceData>,
}

fn default_quantity() -> i64 {
    1
}

/// Inline price data for line item
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateLineItemPriceData {
    pub currency: String,
    pub unit_amount: i64,
    pub product_data: CreateLineItemProductData,
}

/// Inline product data for line item
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateLineItemProductData {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub images: Option<Vec<String>>,
    #[serde(default)]
    pub metadata: Option<HashMap<String, String>>,
}

/// Expire checkout session request (optional)
#[derive(Debug, Clone, Deserialize)]
pub struct ExpireCheckoutSessionRequest {
    // Empty for now, but could include options
}
