use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionStatus {
    Completed,
    Pending,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionCustomer {
    pub label: Option<String>,
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FacilitatedTransaction {
    pub id: i32,
    pub created_at: i64,                       // Unix timestamp
    pub updated_at: i64,                       // Unix timestamp
    pub product: Option<String>,               // Product name
    pub customer: Option<TransactionCustomer>, // Customer with label and address
    pub amount: String,                        // Amount as string
    pub currency: Option<String>,              // Currency code (e.g., "USDC")
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>, // Solana transaction signature

    // Debug fields - x402 protocol messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_payment_requirement: Option<String>, // Base64-encoded 402 response when PAYMENT header missing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_verify_request: Option<String>, // Base64-encoded verify request to facilitator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_verify_response: Option<String>, // Base64-encoded verify response from facilitator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_settle_request: Option<String>, // Base64-encoded settle request to facilitator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_settle_response: Option<String>, // Base64-encoded settle response from facilitator

    // Payment stack context
    pub payment_stack_id: String, // The payment stack ID (subdomain) that processed this transaction
    pub is_sandbox: bool,         // Whether this transaction was processed in sandbox mode
}
