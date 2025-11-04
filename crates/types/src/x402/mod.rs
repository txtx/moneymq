use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};
use solana_keypair::Pubkey;
use url::Url;

pub mod config;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixedAddress {
    Solana(Pubkey),   // Base58-encoded
    Offchain(String), // Custom format
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

/// Settle request - identical to verify request
pub type SettleRequest = VerifyRequest;

/// Facilitator error reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum FacilitatorErrorReason {
    /// Payer doesn't have sufficient funds.
    #[serde(rename = "insufficient_funds")]
    InsufficientFunds,
    /// The scheme in PaymentPayload didn't match expected (e.g., not 'exact'), or settlement failed.
    #[serde(rename = "invalid_scheme")]
    InvalidScheme,
    /// Network in PaymentPayload didn't match a facilitator's expected network.
    #[serde(rename = "invalid_network")]
    InvalidNetwork,
    /// Unexpected settle error
    #[serde(rename = "unexpected_settle_error")]
    UnexpectedSettleError,
    FreeForm(String),
}

/// Verify response - validation result
#[derive(Debug, Clone)]
pub enum VerifyResponse {
    Valid {
        payer: MixedAddress,
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
            VerifyResponse::Valid { .. } => serializer.serialize_struct("VerifyResponse", 2)?,
            VerifyResponse::Invalid { .. } => serializer.serialize_struct("VerifyResponse", 3)?,
        };

        match self {
            VerifyResponse::Valid { payer } => {
                s.serialize_field("isValid", &true)?;
                s.serialize_field("payer", payer)?;
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
        }

        let raw = Raw::deserialize(deserializer)?;

        match (raw.is_valid, raw.invalid_reason) {
            (true, None) => match raw.payer {
                None => Err(serde::de::Error::custom(
                    "`payer` must be present when `isValid` is true",
                )),
                Some(payer) => Ok(VerifyResponse::Valid { payer }),
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
