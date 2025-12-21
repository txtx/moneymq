use axum::{Extension, Json, response::IntoResponse};

use crate::api::{
    catalog::stripe::types::{ListParams, ListResponse},
    payment::PaymentApiConfig,
};

/// GET /v1/transactions - List transactions
///
/// This endpoint returns a list of transactions stored from x402 payments.
pub async fn list_transactions(
    Extension(state): Extension<Option<PaymentApiConfig>>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let Some(state) = state else {
        return Json(ListResponse {
            object: "list".to_string(),
            data: vec![],
            has_more: false,
            url: "/v1/transactions".to_string(),
        });
    };

    let limit = params.limit.unwrap_or(10).min(100) as usize;

    let start_idx = if let Some(starting_after) = params.starting_after {
        starting_after.parse().unwrap_or(0)
    } else {
        0
    };

    let (transactions, has_more) = state
        .db_manager
        .list_transactions(
            limit,
            Some(start_idx),
            &state.payment_stack_id,
            state.is_sandbox,
        )
        .unwrap_or((vec![], false));

    Json(ListResponse {
        object: "list".to_string(),
        data: transactions.to_vec(),
        has_more,
        url: "/v1/transactions".to_string(),
    })
}
