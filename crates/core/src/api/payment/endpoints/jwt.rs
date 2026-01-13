//! JWT (JSON Web Token) utilities for payment attestation
//!
//! This module provides JWT signing and verification for payment receipts.
//! JWTs are created after successful payment settlement to provide cryptographic
//! proof of payment that can be verified by third-party services.
//!
//! Uses ES256 (ECDSA with P-256) for asymmetric signing, allowing public key
//! verification via JWKS endpoint.

use std::sync::Arc;

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
// Re-export types from moneymq-types
pub use moneymq_types::{BasketItem, defaults};
use p256::ecdsa::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Payment details within the receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDetails {
    /// Payer's public key/address
    pub payer: String,
    /// Transaction ID (deterministic hash of the payment)
    pub transaction_id: String,
    /// Payment amount (as string to preserve precision)
    pub amount: String,
    /// Currency code (e.g., "USDC")
    pub currency: String,
    /// Network (e.g., "solana")
    pub network: String,
    /// On-chain transaction signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Attachments containing processor-provided data
/// Nested map: actor_id -> (key -> data)
/// e.g., { "my-processor": { "surfnet": {...} } }
pub type Attachments = serde_json::Map<String, serde_json::Value>;

/// JWT claims for a payment receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentReceiptClaims {
    /// Basket items (products purchased)
    pub basket: Vec<BasketItem>,
    /// Payment details
    pub payment: PaymentDetails,
    /// Attachments from processors and other services
    #[serde(default, skip_serializing_if = "is_attachments_empty")]
    pub attachments: Attachments,
    /// JWT issued at timestamp (Unix seconds)
    pub iat: i64,
    /// JWT expiration timestamp (Unix seconds)
    pub exp: i64,
    /// Issuer (MoneyMQ payment stack ID)
    pub iss: String,
    /// Subject (transaction_id)
    pub sub: String,
}

fn is_attachments_empty(attachments: &Attachments) -> bool {
    attachments.is_empty()
}

impl PaymentReceiptClaims {
    /// Create new payment receipt claims
    pub fn new(
        transaction_id: String,
        payer: String,
        amount: String,
        currency: String,
        network: String,
        product_id: Option<String>,
        features: Option<serde_json::Value>,
        transaction_signature: Option<String>,
        issuer: String,
        expiration_hours: u64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        let exp = now + (expiration_hours as i64 * 3600);

        // Build basket from product_id and features
        let basket = if let Some(pid) = product_id {
            vec![BasketItem {
                product_id: pid,
                experiment_id: None,
                features: features.unwrap_or_default(),
                quantity: 1,
            }]
        } else {
            vec![]
        };

        let payment = PaymentDetails {
            payer,
            transaction_id: transaction_id.clone(),
            amount,
            currency,
            network: network.to_lowercase(),
            signature: transaction_signature,
        };

        Self {
            basket,
            payment,
            attachments: Attachments::new(),
            iat: now,
            exp,
            iss: issuer,
            sub: transaction_id,
        }
    }

    /// Create new payment receipt claims with a pre-built basket
    pub fn new_with_basket(
        transaction_id: String,
        payer: String,
        amount: String,
        currency: String,
        network: String,
        basket: Vec<BasketItem>,
        transaction_signature: Option<String>,
        issuer: String,
        expiration_hours: u64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        let exp = now + (expiration_hours as i64 * 3600);

        let payment = PaymentDetails {
            payer,
            transaction_id: transaction_id.clone(),
            amount,
            currency,
            network: network.to_lowercase(),
            signature: transaction_signature,
        };

        Self {
            basket,
            payment,
            attachments: Attachments::new(),
            iat: now,
            exp,
            iss: issuer,
            sub: transaction_id,
        }
    }

    /// Add attachments data (map of key -> data from AttachDataRequest)
    pub fn with_attachments(mut self, data: Attachments) -> Self {
        self.attachments = data;
        self
    }

    /// Add features to all basket items
    pub fn with_features(mut self, features: serde_json::Value) -> Self {
        for item in &mut self.basket {
            item.features = features.clone();
        }
        self
    }
}

/// JWKS (JSON Web Key Set) response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwksResponse {
    pub keys: Vec<Jwk>,
}

/// JSON Web Key for ES256
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwk {
    pub kty: String,
    pub crv: String,
    pub x: String,
    pub y: String,
    #[serde(rename = "use")]
    pub key_use: String,
    pub kid: String,
    pub alg: String,
}

/// JWT key pair for signing and verification
#[derive(Clone)]
pub struct JwtKeyPair {
    signing_key: Arc<SigningKey>,
    verifying_key: VerifyingKey,
    key_id: String,
}

impl JwtKeyPair {
    /// Create a new key pair from a secret (deterministic derivation)
    pub fn from_secret(secret: &str) -> Self {
        // Derive a deterministic private key from the secret using SHA256
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hasher.update(b"moneymq-jwt-key-derivation-v1");
        let hash = hasher.finalize();

        // Use the hash as the private key scalar
        let signing_key =
            SigningKey::from_slice(&hash).expect("Hash should be valid for P-256 scalar");
        let verifying_key = *signing_key.verifying_key();

        // Generate key ID from public key hash
        let mut kid_hasher = Sha256::new();
        kid_hasher.update(verifying_key.to_encoded_point(false).as_bytes());
        let kid_hash = kid_hasher.finalize();
        let key_id = format!("moneymq-{}", hex::encode(&kid_hash[..8]));

        Self {
            signing_key: Arc::new(signing_key),
            verifying_key,
            key_id,
        }
    }

    /// Get the key ID
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Get the JWKS response containing the public key
    pub fn jwks(&self) -> JwksResponse {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

        let point = self.verifying_key.to_encoded_point(false);
        let x_bytes = point.x().expect("Point should have x coordinate");
        let y_bytes = point.y().expect("Point should have y coordinate");

        let jwk = Jwk {
            kty: "EC".to_string(),
            crv: "P-256".to_string(),
            x: URL_SAFE_NO_PAD.encode(x_bytes),
            y: URL_SAFE_NO_PAD.encode(y_bytes),
            key_use: "sig".to_string(),
            kid: self.key_id.clone(),
            alg: "ES256".to_string(),
        };

        JwksResponse { keys: vec![jwk] }
    }

    /// Sign payment receipt claims and return a JWT string
    pub fn sign(&self, claims: &PaymentReceiptClaims) -> Result<String, String> {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        use p256::ecdsa::signature::Signer;

        // Create JWT header
        let header = serde_json::json!({
            "alg": "ES256",
            "typ": "JWT",
            "kid": self.key_id
        });
        let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());

        // Create JWT payload
        let payload_b64 = URL_SAFE_NO_PAD.encode(
            serde_json::to_string(claims)
                .map_err(|e| format!("Failed to serialize claims: {}", e))?
                .as_bytes(),
        );

        // Create signing input
        let signing_input = format!("{}.{}", header_b64, payload_b64);

        // Sign with ECDSA
        let signature: p256::ecdsa::Signature = self.signing_key.sign(signing_input.as_bytes());

        // Convert signature to JWS format (r || s, each 32 bytes)
        let sig_bytes = signature.to_bytes();
        let signature_b64 = URL_SAFE_NO_PAD.encode(sig_bytes);

        Ok(format!("{}.{}", signing_input, signature_b64))
    }
}

/// JWT signer for payment receipts (legacy HS256 compatibility)
pub struct PaymentJwtSigner {
    encoding_key: EncodingKey,
    header: Header,
}

impl PaymentJwtSigner {
    /// Create a new JWT signer with the given secret (HS256)
    pub fn new(secret: &str) -> Self {
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        let header = Header::new(Algorithm::HS256);

        Self {
            encoding_key,
            header,
        }
    }

    /// Sign payment receipt claims and return a JWT string
    pub fn sign(
        &self,
        claims: &PaymentReceiptClaims,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        encode(&self.header, claims, &self.encoding_key)
    }
}

/// Create a payment receipt JWT using ES256 (asymmetric, JWKS-compatible)
pub fn create_payment_receipt_jwt_es256(
    key_pair: &JwtKeyPair,
    transaction_id: String,
    payer: String,
    amount: String,
    currency: String,
    network: String,
    product_id: Option<String>,
    features: Option<serde_json::Value>,
    transaction_signature: Option<String>,
    issuer: String,
) -> Result<String, String> {
    let claims = PaymentReceiptClaims::new(
        transaction_id,
        payer,
        amount,
        currency,
        network,
        product_id,
        features,
        transaction_signature,
        issuer,
        defaults::JWT_EXPIRATION_HOURS,
    );

    key_pair.sign(&claims)
}

/// Create a payment receipt JWT using HS256 (symmetric, legacy)
pub fn create_payment_receipt_jwt(
    secret: &str,
    transaction_id: String,
    payer: String,
    amount: String,
    currency: String,
    network: String,
    product_id: Option<String>,
    features: Option<serde_json::Value>,
    transaction_signature: Option<String>,
    issuer: String,
) -> Result<String, String> {
    let signer = PaymentJwtSigner::new(secret);

    let claims = PaymentReceiptClaims::new(
        transaction_id,
        payer,
        amount,
        currency,
        network,
        product_id,
        features,
        transaction_signature,
        issuer,
        defaults::JWT_EXPIRATION_HOURS,
    );

    signer
        .sign(&claims)
        .map_err(|e| format!("Failed to sign JWT: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_key_pair_from_secret() {
        let key_pair = JwtKeyPair::from_secret("test-secret");
        assert!(!key_pair.key_id().is_empty());
        assert!(key_pair.key_id().starts_with("moneymq-"));
    }

    #[test]
    fn test_jwks_generation() {
        let key_pair = JwtKeyPair::from_secret("test-secret");
        let jwks = key_pair.jwks();

        assert_eq!(jwks.keys.len(), 1);
        let jwk = &jwks.keys[0];
        assert_eq!(jwk.kty, "EC");
        assert_eq!(jwk.crv, "P-256");
        assert_eq!(jwk.alg, "ES256");
        assert_eq!(jwk.key_use, "sig");
        assert!(!jwk.x.is_empty());
        assert!(!jwk.y.is_empty());
    }

    #[test]
    fn test_create_payment_receipt_jwt_es256() {
        let key_pair = JwtKeyPair::from_secret("test-secret");
        let features = serde_json::json!({
            "max_transactions": 500,
            "storage_gb": 10
        });

        let jwt = create_payment_receipt_jwt_es256(
            &key_pair,
            "tx_123abc".to_string(),
            "payer_pubkey".to_string(),
            "1000000".to_string(),
            defaults::CURRENCY.to_string(),
            defaults::NETWORK.to_string(),
            Some("prod_xyz".to_string()),
            Some(features),
            Some("5sigABC...".to_string()),
            "moneymq-local".to_string(),
        );

        assert!(jwt.is_ok());
        let token = jwt.unwrap();
        assert!(token.starts_with("eyJ")); // JWT header prefix
        assert_eq!(token.matches('.').count(), 2); // JWT has 3 parts
    }

    #[test]
    fn test_deterministic_key_derivation() {
        let key_pair1 = JwtKeyPair::from_secret("same-secret");
        let key_pair2 = JwtKeyPair::from_secret("same-secret");

        // Same secret should produce same key ID
        assert_eq!(key_pair1.key_id(), key_pair2.key_id());

        // Same JWKS
        let jwks1 = key_pair1.jwks();
        let jwks2 = key_pair2.jwks();
        assert_eq!(jwks1.keys[0].x, jwks2.keys[0].x);
        assert_eq!(jwks1.keys[0].y, jwks2.keys[0].y);
    }

    #[test]
    fn test_create_payment_receipt_jwt_hs256() {
        let features = serde_json::json!({
            "max_transactions": 500,
            "storage_gb": 10
        });
        let jwt = create_payment_receipt_jwt(
            "test-secret-key",
            "tx_123abc".to_string(),
            "payer_pubkey".to_string(),
            "1000000".to_string(),
            defaults::CURRENCY.to_string(),
            defaults::NETWORK.to_string(),
            Some("prod_xyz".to_string()),
            Some(features),
            Some("5sigABC...".to_string()),
            "moneymq-local".to_string(),
        );

        assert!(jwt.is_ok());
        let token = jwt.unwrap();
        assert!(token.starts_with("eyJ")); // JWT header prefix
        assert!(token.contains('.')); // JWT has 3 parts separated by dots
    }

    #[test]
    fn test_payment_receipt_claims() {
        let claims = PaymentReceiptClaims::new(
            "tx_123".to_string(),
            "payer".to_string(),
            "1000".to_string(),
            defaults::CURRENCY.to_string(),
            defaults::NETWORK.to_string(),
            Some("prod_abc".to_string()),
            None,
            None,
            "issuer".to_string(),
            defaults::JWT_EXPIRATION_HOURS,
        );

        assert_eq!(claims.payment.transaction_id, "tx_123");
        assert_eq!(claims.payment.payer, "payer");
        assert_eq!(claims.payment.network, defaults::NETWORK);
        assert_eq!(claims.sub, "tx_123");
        assert!(claims.exp > claims.iat);
        assert_eq!(claims.basket.len(), 1);
        assert_eq!(claims.basket[0].product_id, "prod_abc");
    }

    #[test]
    fn test_payment_receipt_with_attachments() {
        // Create nested attachments: { "my-processor": { "surfnet": {...} } }
        let mut inner_map = serde_json::Map::new();
        inner_map.insert(
            "surfnet".to_string(),
            serde_json::json!({
                "bucket": "uploads",
                "key_prefix": "user/123"
            }),
        );
        let mut attachments = Attachments::new();
        attachments.insert(
            "my-processor".to_string(),
            serde_json::Value::Object(inner_map),
        );

        let claims = PaymentReceiptClaims::new(
            "tx_456".to_string(),
            "payer".to_string(),
            "2000".to_string(),
            defaults::CURRENCY.to_string(),
            defaults::NETWORK.to_string(),
            Some("prod_xyz".to_string()),
            None,
            None,
            "issuer".to_string(),
            defaults::JWT_EXPIRATION_HOURS,
        )
        .with_attachments(attachments);

        assert!(claims.attachments.contains_key("my-processor"));
        let processor = claims.attachments.get("my-processor").unwrap();
        assert!(processor.get("surfnet").is_some());
        assert_eq!(processor["surfnet"]["bucket"], "uploads");
    }
}
