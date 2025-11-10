use axum::{Json, extract::State, response::IntoResponse};

use crate::catalog::{
    ProviderState,
    stripe::types::{ListParams, ListResponse},
};

/// GET /v1/transactions - List transactions
///
/// This endpoint returns a list of transactions stored from x402 payments.
pub async fn list_transactions(
    State(state): State<ProviderState>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(100) as usize;

    // Get transactions from state
    let all_transactions = match state.transactions.lock() {
        Ok(transactions) => transactions.clone(),
        Err(_) => {
            // If lock fails, return empty list
            vec![]
        }
    };

    // Handle pagination
    let start_idx = if let Some(starting_after) = params.starting_after {
        all_transactions
            .iter()
            .position(|t| t.id == starting_after)
            .map(|idx| idx + 1)
            .unwrap_or(0)
    } else {
        0
    };

    let end_idx = (start_idx + limit).min(all_transactions.len());
    let transactions_slice = &all_transactions[start_idx..end_idx];

    let has_more = end_idx < all_transactions.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: transactions_slice.to_vec(),
        has_more,
        url: "/v1/transactions".to_string(),
    })
}
