use axum::{Extension, Json, response::IntoResponse};

use crate::api::{
    catalog::stripe::types::{ListParams, ListResponse},
    payment::PaymentApiConfig,
};

/// GET /v1/transactions - List transactions
///
/// This endpoint returns a list of transactions stored from x402 payments.
pub async fn list_transactions(
    Extension(state): Extension<PaymentApiConfig>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(100) as usize;

    let starting_after = params.starting_after.and_then(|s| s.parse::<i32>().ok());

    let (transactions, has_more) = state
        .db_manager
        .list_transactions(
            limit,
            starting_after,
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
