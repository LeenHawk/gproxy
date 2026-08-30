use sha2::{Digest, Sha256};

pub(crate) const USER_KEY_DIGEST_VERSION: u32 = 1;
const SUPPORTED_DIGEST_VERSIONS: &[u32] = &[1];

/// `sk-` and `at-` are presentation prefixes, not part of the secret: a channel
/// that demands a PAT-shaped token takes `at-…` while the same key is presented
/// as `sk-…` elsewhere. Digesting the payload keeps both forms one identity, so
/// quota and usage follow the key rather than the spelling.
pub(crate) fn key_payload(api_key: &str) -> &str {
    api_key
        .strip_prefix("sk-")
        .or_else(|| api_key.strip_prefix("at-"))
        .unwrap_or(api_key)
}

pub(crate) fn user_key_digest(version: u32, api_key: &str) -> Option<Vec<u8>> {
    match version {
        1 => Some(Sha256::digest(key_payload(api_key).as_bytes()).to_vec()),
        _ => None,
    }
}

pub(crate) fn user_key_digests(api_key: &str) -> impl Iterator<Item = (u32, Vec<u8>)> + '_ {
    SUPPORTED_DIGEST_VERSIONS.iter().filter_map(move |version| {
        user_key_digest(*version, api_key).map(|digest| (*version, digest))
    })
}

pub(crate) fn supported_user_key_digest(version: u32) -> bool {
    SUPPORTED_DIGEST_VERSIONS.contains(&version)
}
