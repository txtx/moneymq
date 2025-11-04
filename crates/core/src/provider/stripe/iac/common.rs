use std::collections::HashMap;

use indexmap::IndexMap;
use sha2::{Digest, Sha256};

/// Convert Stripe metadata (HashMap) to IndexMap with sorted keys for consistent ordering
pub fn metadata_to_sorted_indexmap(metadata: HashMap<String, String>) -> IndexMap<String, String> {
    let mut keys: Vec<_> = metadata.keys().collect();
    keys.sort();
    keys.into_iter()
        .map(|k| (k.clone(), metadata.get(k).unwrap().clone()))
        .collect()
}

/// Generate a base58-encoded ID from a Stripe resource ID
/// Uses SHA256 hash to ensure consistent length and valid base58 characters
pub fn generate_base58_id(stripe_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(stripe_id.as_bytes());
    let hash = hasher.finalize();

    // Take first 16 bytes of hash for a reasonable ID length
    bs58::encode(&hash[..16]).into_string()
}
