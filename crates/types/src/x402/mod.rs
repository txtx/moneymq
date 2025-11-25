use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};
use solana_pubkey::Pubkey;
use url::Url;

pub mod config;
mod currency;
mod recipient;
pub mod transactions;

pub use currency::{Currency, SolanaCurrency};
pub use recipient::{
    LocalManagedRecipient, MoneyMqManagedRecipient, Recipient, RemoteManagedRecipient,
};

/// X402 protocol version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X402Version {
    V1,
}

impl Serialize for X402Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            X402Version::V1 => serializer.serialize_u8(1),
        }
    }
}

impl Display for X402Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            X402Version::V1 => write!(f, "1"),
        }
    }
}

#[derive(Debug)]
pub struct X402VersionError(pub u8);

impl Display for X402VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unsupported x402Version: {}", self.0)
    }
}
impl std::error::Error for X402VersionError {}

impl TryFrom<u8> for X402Version {
    type Error = X402VersionError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(X402Version::V1),
            _ => Err(X402VersionError(value)),
        }
    }
}

impl<'de> Deserialize<'de> for X402Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let num = u8::deserialize(deserializer)?;
        X402Version::try_from(num).map_err(serde::de::Error::custom)
    }
}

/// Payment scheme
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Exact,
}

/// MoneyMQ internal networks
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoneyMqNetwork {
    SolanaSurfnet,
    SolanaMainnet,
}

impl From<MoneyMqNetwork> for Network {
    fn from(network: MoneyMqNetwork) -> Self {
        match network {
            MoneyMqNetwork::SolanaSurfnet => Network::Solana,
            MoneyMqNetwork::SolanaMainnet => Network::Solana,
        }
    }
}

/// Network identifier for the x402 protocol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    Solana,
}

impl Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Network::Solana => write!(f, "solana"),
        }
    }
}

/// Token amount (U256 as string)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenAmount(pub String);

/// Mixed address Solana or off-chain formats
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixedAddress {
    Solana(Pubkey),   // Base58-encoded
    Offchain(String), // Custom format
}

impl MixedAddress {
    pub fn pubkey(&self) -> Option<&Pubkey> {
        match self {
            MixedAddress::Solana(pk) => Some(pk),
            _ => None,
        }
    }
}

impl Display for MixedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MixedAddress::Offchain(address) => write!(f, "{address}"),
            MixedAddress::Solana(pubkey) => write!(f, "{pubkey}"),
        }
    }
}

impl<'de> Deserialize<'de> for MixedAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // static OFFCHAIN_ADDRESS_REGEX: Lazy<Regex> = Lazy::new(|| {
        //     Regex::new(r"^[A-Za-z0-9][A-Za-z0-9-]{0,34}[A-Za-z0-9]$")
        //         .expect("Invalid regex for offchain address")
        // });

        let s = String::deserialize(deserializer)?;
        // 2) Solana Pubkey (base58, 32 bytes)
        if let Ok(pk) = Pubkey::from_str(&s) {
            return Ok(MixedAddress::Solana(pk));
        }
        // 3) Off-chain address by regex
        // if OFFCHAIN_ADDRESS_REGEX.is_match(&s) {
        //     return Ok(MixedAddress::Offchain(s));
        // }
        Err(serde::de::Error::custom("Invalid address format"))
    }
}
impl Serialize for MixedAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // MixedAddress::Evm(addr) => serializer.serialize_str(&addr.to_string()),
            MixedAddress::Offchain(s) => serializer.serialize_str(s),
            MixedAddress::Solana(pubkey) => serializer.serialize_str(pubkey.to_string().as_str()),
        }
    }
}

impl From<Pubkey> for MixedAddress {
    fn from(pubkey: Pubkey) -> Self {
        MixedAddress::Solana(pubkey)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionHash {
    /// A 32-byte EVM transaction hash, encoded as 0x-prefixed hex string.
    // Evm([u8; 32]),
    Solana([u8; 64]),
}

impl<'de> Deserialize<'de> for TransactionHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;

        // static EVM_TX_HASH_REGEX: Lazy<Regex> =
        //     Lazy::new(|| Regex::new(r"^0x[0-9a-fA-F]{64}$").expect("invalid regex"));

        // // EVM: 0x-prefixed, 32 bytes hex
        // if EVM_TX_HASH_REGEX.is_match(&s) {
        //     let bytes = hex::decode(s.trim_start_matches("0x"))
        //         .map_err(|_| serde::de::Error::custom("Invalid hex in transaction hash"))?;
        //     let array: [u8; 32] = bytes.try_into().map_err(|_| {
        //         serde::de::Error::custom("Transaction hash must be exactly 32 bytes")
        //     })?;
        //     return Ok(TransactionHash::Evm(array));
        // }

        // Solana: base58 string, decodes to exactly 64 bytes
        if let Ok(bytes) = bs58::decode(&s).into_vec()
            && bytes.len() == 64
        {
            let array: [u8; 64] = bytes.try_into().unwrap(); // safe after length check
            return Ok(TransactionHash::Solana(array));
        }

        Err(serde::de::Error::custom("Invalid transaction hash format"))
    }
}

impl Serialize for TransactionHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // TransactionHash::Evm(bytes) => {
            //     let hex_string = format!("0x{}", hex::encode(bytes));
            //     serializer.serialize_str(&hex_string)
            // }
            TransactionHash::Solana(bytes) => {
                let b58_string = bs58::encode(bytes).into_string();
                serializer.serialize_str(&b58_string)
            }
        }
    }
}

impl Display for TransactionHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            // TransactionHash::Evm(bytes) => {
            //     write!(f, "0x{}", hex::encode(bytes))
            // }
            TransactionHash::Solana(bytes) => {
                write!(f, "{}", bs58::encode(bytes).into_string())
            }
        }
    }
}

/// Exact Solana payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct PaymentPayload {
    pub x402_version: X402Version,
    pub scheme: Scheme,
    pub network: Network,
    pub payload: ExactPaymentPayload,
}

/// Payment requirements - constraints for acceptable payments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// Settle request - includes transaction ID for linking with verify operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettleRequest {
    pub x402_version: X402Version,
    pub payment_payload: PaymentPayload,
    pub payment_requirements: PaymentRequirements,
    /// Optional transaction ID from verify response for explicit linking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
}

/// Facilitator error reasons
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[serde(untagged, rename_all = "camelCase")]
pub enum FacilitatorErrorReason {
    /// Payer doesn't have sufficient funds.
    #[error("insufficient_funds")]
    #[serde(rename = "insufficient_funds")]
    InsufficientFunds,
    /// The scheme in PaymentPayload didn't match expected (e.g., not 'exact'), or settlement failed.
    #[error("invalid_scheme")]
    #[serde(rename = "invalid_scheme")]
    InvalidScheme,
    /// Network in PaymentPayload didn't match a facilitator's expected network.
    #[error("invalid_network")]
    #[serde(rename = "invalid_network")]
    InvalidNetwork,
    /// Unexpected settle error
    #[error("unexpected_settle_error")]
    #[serde(rename = "unexpected_settle_error")]
    UnexpectedSettleError,
    #[error("{0}")]
    FreeForm(String),
}

/// Verify response - validation result
#[derive(Debug, Clone)]
pub enum VerifyResponse {
    Valid {
        payer: MixedAddress,
        transaction_id: String,
    },
    Invalid {
        reason: FacilitatorErrorReason,
        payer: Option<MixedAddress>,
    },
}

impl Serialize for VerifyResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = match self {
            VerifyResponse::Valid { .. } => serializer.serialize_struct("VerifyResponse", 3)?,
            VerifyResponse::Invalid { .. } => serializer.serialize_struct("VerifyResponse", 3)?,
        };

        match self {
            VerifyResponse::Valid { payer, transaction_id } => {
                s.serialize_field("isValid", &true)?;
                s.serialize_field("payer", payer)?;
                s.serialize_field("transactionId", transaction_id)?;
            }
            VerifyResponse::Invalid { reason, payer } => {
                s.serialize_field("isValid", &false)?;
                s.serialize_field("invalidReason", reason)?;
                if let Some(payer) = payer {
                    s.serialize_field("payer", payer)?
                }
            }
        }

        s.end()
    }
}

impl<'de> Deserialize<'de> for VerifyResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            is_valid: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            payer: Option<MixedAddress>,
            #[serde(default)]
            invalid_reason: Option<FacilitatorErrorReason>,
            #[serde(default)]
            transaction_id: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;

        match (raw.is_valid, raw.invalid_reason) {
            (true, None) => match (raw.payer, raw.transaction_id) {
                (None, _) => Err(serde::de::Error::custom(
                    "`payer` must be present when `isValid` is true",
                )),
                (Some(_), None) => Err(serde::de::Error::custom(
                    "`transactionId` must be present when `isValid` is true",
                )),
                (Some(payer), Some(transaction_id)) => Ok(VerifyResponse::Valid { payer, transaction_id }),
            },
            (false, Some(reason)) => Ok(VerifyResponse::Invalid {
                payer: raw.payer,
                reason,
            }),
            (true, Some(_)) => Err(serde::de::Error::custom(
                "`invalidReason` must be absent when `isValid` is true",
            )),
            (false, None) => Err(serde::de::Error::custom(
                "`invalidReason` must be present when `isValid` is false",
            )),
        }
    }
}

/// Settle response - on-chain settlement outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    pub extra: Option<SupportedPaymentKindExtra>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedPaymentKindExtra {
    pub fee_payer: MixedAddress,
}

/// Supported payment kinds response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedResponse {
    pub kinds: Vec<SupportedPaymentKind>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_serialization() {
        // Test SolanaMainnet serializes to "solana"
        let mainnet = Network::Solana;
        let json = serde_json::to_string(&mainnet).unwrap();
        assert_eq!(json, r#""solana""#);

        // Test deserialization
        let parsed: Network = serde_json::from_str(r#""solana""#).unwrap();
        assert_eq!(parsed, Network::Solana);
    }

    #[test]
    fn test_verify_response_valid_with_transaction_id() {
        use std::str::FromStr;
        
        let pubkey = Pubkey::from_str("11111111111111111111111111111112").unwrap();
        let transaction_id = "550e8400-e29b-41d4-a716-446655440000".to_string();
        
        let response = VerifyResponse::Valid {
            payer: MixedAddress::Solana(pubkey),
            transaction_id: transaction_id.clone(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"isValid\":true"));
        assert!(json.contains("\"transactionId\""));
        assert!(json.contains(&transaction_id));
        let parsed: VerifyResponse = serde_json::from_str(&json).unwrap();
        match parsed {
            VerifyResponse::Valid { payer: _, transaction_id: tid } => {
                assert_eq!(tid, transaction_id);
            }
            _ => panic!("Expected Valid response"),
        }
    }

    #[test]
    fn test_verify_response_invalid_without_transaction_id() {
        let json = r#"{
            "isValid": true,
            "payer": "11111111111111111111111111111112"
        }"#;

            let result: Result<VerifyResponse, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_settle_request_with_transaction_id() {
        use std::str::FromStr;
        
        let pubkey = Pubkey::from_str("11111111111111111111111111111112").unwrap();
        let transaction_id = "550e8400-e29b-41d4-a716-446655440000".to_string();

        let settle_request = SettleRequest {
            x402_version: X402Version::V1,
            payment_payload: PaymentPayload {
                x402_version: X402Version::V1,
                scheme: Scheme::Exact,
                network: Network::Solana,
                payload: ExactPaymentPayload::Solana(ExactSolanaPayload {
                    transaction: "test".to_string(),
                }),
            },
            payment_requirements: PaymentRequirements {
                scheme: Scheme::Exact,
                network: Network::Solana,
                max_amount_required: TokenAmount("1000000".to_string()),
                resource: url::Url::parse("https://example.com").unwrap(),
                description: "Test".to_string(),
                mime_type: "application/json".to_string(),
                output_schema: None,
                pay_to: MixedAddress::Solana(pubkey),
                max_timeout_seconds: 300,
                asset: MixedAddress::Solana(pubkey),
                extra: None,
            },
            transaction_id: Some(transaction_id.clone()),
        };

            let json = serde_json::to_string(&settle_request).unwrap();
        assert!(json.contains("\"transactionId\""));
        assert!(json.contains(&transaction_id));
        let parsed: SettleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.transaction_id, Some(transaction_id));
    }

    #[test]
    fn test_settle_request_without_transaction_id() {
        use std::str::FromStr;
        
        let pubkey = Pubkey::from_str("11111111111111111111111111111112").unwrap();

        let settle_request = SettleRequest {
            x402_version: X402Version::V1,
            payment_payload: PaymentPayload {
                x402_version: X402Version::V1,
                scheme: Scheme::Exact,
                network: Network::Solana,
                payload: ExactPaymentPayload::Solana(ExactSolanaPayload {
                    transaction: "test".to_string(),
                }),
            },
            payment_requirements: PaymentRequirements {
                scheme: Scheme::Exact,
                network: Network::Solana,
                max_amount_required: TokenAmount("1000000".to_string()),
                resource: url::Url::parse("https://example.com").unwrap(),
                description: "Test".to_string(),
                mime_type: "application/json".to_string(),
                output_schema: None,
                pay_to: MixedAddress::Solana(pubkey),
                max_timeout_seconds: 300,
                asset: MixedAddress::Solana(pubkey),
                extra: None,
            },
            transaction_id: None,
        };

        let json = serde_json::to_string(&settle_request).unwrap();
        assert!(!json.contains("transactionId"));

        let parsed: SettleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.transaction_id, None);
    }
}

#[test]
fn test_supported_payment_kind_extra_serialization() {
    use std::str::FromStr;

    use solana_pubkey::Pubkey;

    let pubkey = Pubkey::from_str("11111111111111111111111111111112").unwrap();
    let extra = SupportedPaymentKindExtra {
        fee_payer: MixedAddress::Solana(pubkey),
    };

    let json = serde_json::to_string(&extra).unwrap();
    println!("Serialized: {}", json);

    // Should be camelCase
    assert!(json.contains("feePayer"));
    assert!(!json.contains("fee_payer"));
}
