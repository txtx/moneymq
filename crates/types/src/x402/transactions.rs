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
    pub id: String,
    pub object: String,
    pub timestamp: i64,                // Unix timestamp
    pub product: String,               // Product name
    pub customer: TransactionCustomer, // Customer with label and address
    pub amount: String,                // Amount as string
    pub currency: String,              // Currency code (e.g., "USDC")
    pub status: TransactionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>, // Solana transaction signature
}

impl FacilitatedTransaction {
    /// Create a transaction with actual data
    pub fn new(
        id: String,
        timestamp: i64,
        product: String,
        customer_label: Option<String>,
        customer_address: String,
        amount: String,
        currency: String,
        status: TransactionStatus,
        signature: Option<String>,
    ) -> Self {
        Self {
            id,
            object: "transaction".to_string(),
            timestamp,
            product,
            customer: TransactionCustomer {
                label: customer_label,
                address: customer_address,
            },
            amount,
            currency,
            status,
            signature,
        }
    }
}
