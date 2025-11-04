use axum::{Json, body::Bytes, extract::State, http::StatusCode, response::IntoResponse};

use crate::provider::{
    ProviderState,
    stripe::{
        types::{StripeSubscription, SubscriptionItemData, SubscriptionItems, SubscriptionPrice},
        utils::generate_stripe_id,
    },
};

/// POST /v1/subscriptions - Create a subscription
pub async fn create_subscription(
    State(_state): State<ProviderState>,
    body: Bytes,
) -> impl IntoResponse {
    // Parse form-encoded body manually to handle nested arrays like items[0][price]
    let body_str = String::from_utf8_lossy(&body);

    // Extract customer field
    let customer = body_str
        .split('&')
        .find(|part| part.starts_with("customer="))
        .and_then(|part| part.strip_prefix("customer="))
        .unwrap_or("unknown_customer")
        .to_string();

    // Extract price IDs from items[N][price] fields
    let price_ids: Vec<String> = body_str
        .split('&')
        .filter(|part| part.contains("[price]="))
        .filter_map(|part| {
            part.split('=')
                .nth(1)
                .map(|id| urlencoding::decode(id).unwrap_or_default().to_string())
        })
        .collect();

    // Generate a mock subscription ID
    let sub_id = generate_stripe_id("sub");
    let created = chrono::Utc::now().timestamp();
    let current_period_end = created + 30 * 24 * 60 * 60; // 30 days from now

    // Mock subscription items
    let items_data: Vec<SubscriptionItemData> = price_ids
        .iter()
        .map(|price_id| {
            let item_id = generate_stripe_id("si");
            SubscriptionItemData {
                id: item_id,
                object: "subscription_item".to_string(),
                price: SubscriptionPrice {
                    id: price_id.clone(),
                    object: "price".to_string(),
                },
            }
        })
        .collect();

    let subscription = StripeSubscription {
        id: sub_id,
        object: "subscription".to_string(),
        customer,
        status: "active".to_string(),
        created,
        current_period_start: created,
        current_period_end,
        items: SubscriptionItems {
            object: "list".to_string(),
            data: items_data,
        },
        latest_invoice: Some(generate_stripe_id("in")),
    };

    (StatusCode::OK, Json(subscription))
}
