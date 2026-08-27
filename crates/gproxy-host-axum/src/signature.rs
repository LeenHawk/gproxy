use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};

pub(crate) fn verify_detached(bytes: &[u8], signature: &str) -> Result<(), SignatureError> {
    verify_detached_with_key(bytes, signature, crate::UPDATE_SIGNING_PUBLIC_KEY)
}

pub(crate) fn verify_detached_with_key(
    bytes: &[u8],
    signature: &str,
    public_key: Option<&str>,
) -> Result<(), SignatureError> {
    let encoded_key = public_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(SignatureError)?;
    let key = decode::<32>(encoded_key)?;
    let signature = decode::<64>(signature.trim())?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| SignatureError)?;
    key.verify_strict(bytes, &Signature::from_bytes(&signature))
        .map_err(|_| SignatureError)
}

fn decode<const N: usize>(value: &str) -> Result<[u8; N], SignatureError> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| SignatureError)?
        .try_into()
        .map_err(|_| SignatureError)
}

#[derive(Debug, thiserror::Error)]
#[error("signature verification failed")]
pub(crate) struct SignatureError;
