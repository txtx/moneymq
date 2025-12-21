use axum::{Extension, Json, body::Bytes, http::StatusCode, response::IntoResponse};

use crate::api::catalog::{
    CatalogState,
    stripe::{
        types::{StripeSubscription, SubscriptionItemData, SubscriptionItems, SubscriptionPrice},
        utils::generate_stripe_id,
    },
};

#[allow(dead_code)]

pub struct SubscriptionRequest {
    pub customer: Option<String>,
    pub price_ids: Vec<String>,
}

impl SubscriptionRequest {
    pub const DEFAULT_CUSTOMER: &str = "unknown_customer";
    pub fn parse(body: &Bytes) -> SubscriptionRequest {
        let body_str = String::from_utf8_lossy(body);

        // Extract customer field
        let customer = body_str
            .split('&')
            .find(|part| part.starts_with("customer="))
            .and_then(|part| part.strip_prefix("customer="))
            .map(|c| c.to_string());

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

        SubscriptionRequest {
            customer,
            price_ids,
        }
    }
}

/// POST /v1/subscriptions - Create a subscription
pub async fn create_subscription(
    Extension(state): Extension<CatalogState>,
    body: Bytes,
) -> impl IntoResponse {
    let SubscriptionRequest {
        customer,
        price_ids,
    } = SubscriptionRequest::parse(&body);

    let customer = customer.unwrap_or_else(|| SubscriptionRequest::DEFAULT_CUSTOMER.to_string());

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

    (StatusCode::OK, Json(subscription)).into_response()
}
