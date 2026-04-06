use sha2::{Digest, Sha256};

/// Compute the stable API-key digest used for in-memory lookup and database
/// uniqueness enforcement.
pub fn api_key_digest(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gproxy-api-key-v1:");
    hasher.update(api_key.as_bytes());
    let hash = hasher.finalize();
    hash.iter().map(|b| format!("{b:02x}")).collect()
}
