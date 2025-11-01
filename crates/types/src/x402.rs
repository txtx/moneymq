use serde::{Deserialize, Serialize};
use url::Url;

/// X402 protocol version
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum X402Version {
    #[serde(rename = "1")]
    V1,
}

/// Payment scheme
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Exact,
}

/// Network identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    SolanaMainnet,
    SolanaSurfnet,
}

/// Token amount (U256 as string)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenAmount(pub String);

/// Mixed address Solana or off-chain formats
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum MixedAddress {
    Solana(String),   // Base58-encoded
    OffChain(String), // Custom format
}

/// Transaction hash
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TransactionHash {
    Solana(String), // Base58-encoded 64-byte
}

/// Exact Solana payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactSolanaPayload {
    pub transaction: String, // Base58-encoded transaction
}

/// Exact payment payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExactPaymentPayload {
    Solana(ExactSolanaPayload),
}

/// Payment payload - signed request to transfer funds on-chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentPayload {
    pub x402_version: X402Version,
    pub scheme: Scheme,
    pub network: Network,
    pub payload: ExactPaymentPayload,
}

/// Payment requirements - constraints for acceptable payments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequirements {
    pub scheme: Scheme,
    pub network: Network,
    pub max_amount_required: TokenAmount,
    pub resource: Url,
    pub description: String,
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    pub pay_to: MixedAddress,
    pub max_timeout_seconds: u64,
    pub asset: MixedAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// Verify request - payment payload and requirements sent to facilitator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    pub x402_version: X402Version,
    pub payment_payload: PaymentPayload,
    pub payment_requirements: PaymentRequirements,
}

/// Settle request - identical to verify request
pub type SettleRequest = VerifyRequest;

/// Facilitator error reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FacilitatorErrorReason {
    InsufficientFunds,
    InvalidSignature,
    InvalidNetwork,
    InvalidAsset,
    Timeout,
    UnknownError,
}

/// Verify response - validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum VerifyResponse {
    Valid {
        payer: MixedAddress,
    },
    Invalid {
        reason: FacilitatorErrorReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        payer: Option<MixedAddress>,
    },
}

/// Settle response - on-chain settlement outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettleResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_reason: Option<FacilitatorErrorReason>,
    pub payer: MixedAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<TransactionHash>,
    pub network: Network,
}

/// Supported payment kind
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedPaymentKind {
    pub x402_version: u8,
    pub scheme: String,
    pub network: Network,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// Supported payment kinds response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedResponse {
    pub kinds: Vec<SupportedPaymentKind>,
}
