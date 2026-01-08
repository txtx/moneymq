use serde::Serialize;

/// Stripe-compatible subscription response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StripeSubscription {
    pub id: String,
    pub object: String,
    pub customer: String,
    pub status: String,
    pub created: i64,
    pub current_period_start: i64,
    pub current_period_end: i64,
    pub items: SubscriptionItems,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_invoice: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionItems {
    pub object: String,
    pub data: Vec<SubscriptionItemData>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionItemData {
    pub id: String,
    pub object: String,
    pub price: SubscriptionPrice,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionPrice {
    pub id: String,
    pub object: String,
}
