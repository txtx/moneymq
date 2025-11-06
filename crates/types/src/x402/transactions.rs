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

    // Debug fields - x402 protocol messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_payment_required_response: Option<String>, // Base64-encoded 402 response when PAYMENT header missing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_verify_request: Option<String>, // Base64-encoded verify request to facilitator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_verify_response: Option<String>, // Base64-encoded verify response from facilitator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_settle_request: Option<String>, // Base64-encoded settle request to facilitator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_settle_response: Option<String>, // Base64-encoded settle response from facilitator
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
            x402_payment_required_response: None,
            x402_verify_request: None,
            x402_verify_response: None,
            x402_settle_request: None,
            x402_settle_response: None,
        }
    }

    /// Set x402 protocol debug data
    pub fn with_x402_data(
        mut self,
        payment_required_response: Option<String>,
        verify_request: Option<String>,
        verify_response: Option<String>,
        settle_request: Option<String>,
        settle_response: Option<String>,
    ) -> Self {
        self.x402_payment_required_response = payment_required_response;
        self.x402_verify_request = verify_request;
        self.x402_verify_response = verify_response;
        self.x402_settle_request = settle_request;
        self.x402_settle_response = settle_response;
        self
    }
}
