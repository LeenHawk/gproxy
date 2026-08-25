use sha2::{Digest, Sha256};

pub(crate) const USER_KEY_DIGEST_VERSION: u32 = 1;
const SUPPORTED_DIGEST_VERSIONS: &[u32] = &[1];

pub(crate) fn user_key_digest(version: u32, api_key: &str) -> Option<Vec<u8>> {
    match version {
        1 => Some(Sha256::digest(api_key.as_bytes()).to_vec()),
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
