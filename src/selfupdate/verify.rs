//! Integrity (sha256) + signature (ed25519) verification (§19.2) — NATIVE only.
//!
//! The signature is the hard floor: the manifest is ed25519-signed and the
//! public key is compiled in. A downloaded artifact is installed only if BOTH
//! its sha256 matches the manifest AND the manifest signature verifies against
//! the embedded key. This applies to BOTH channels (staging included).

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use super::UpdateError;
use super::manifest::Manifest;

/// ed25519 public key (32 raw bytes) compiled into the binary (§19.2). Override
/// at build time with `GPROXY_UPDATE_PUBKEY` (base64 of the 32-byte key);
/// otherwise a placeholder all-zero key is embedded, which **rejects every**
/// signature — the secure default until a real key is provisioned at release.
const EMBEDDED_PUBKEY_B64: Option<&str> = option_env!("GPROXY_UPDATE_PUBKEY");

/// Compute the lowercase-hex sha256 of a file and compare to `expected_hex`.
pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<(), UpdateError> {
    let bytes = std::fs::read(path)?;
    let actual = sha256_hex(&bytes);
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(UpdateError::Integrity(format!(
            "sha256 mismatch: expected {expected_hex}, got {actual}"
        )))
    }
}

/// Lowercase-hex sha256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    to_hex(&digest)
}

/// Verify the manifest's ed25519 signature against the embedded public key.
pub fn verify_manifest_signature(manifest: &Manifest) -> Result<(), UpdateError> {
    let key = embedded_verifying_key()?;
    let sig_bytes = B64
        .decode(manifest.signature.trim())
        .map_err(|e| UpdateError::Signature(format!("signature is not valid base64: {e}")))?;
    let sig_arr: [u8; 64] = sig_bytes.as_slice().try_into().map_err(|_| {
        UpdateError::Signature(format!(
            "signature must be 64 bytes, got {}",
            sig_bytes.len()
        ))
    })?;
    let signature = Signature::from_bytes(&sig_arr);

    key.verify_strict(&manifest.signing_payload(), &signature)
        .map_err(|e| UpdateError::Signature(format!("ed25519 verification failed: {e}")))
}

/// Decode and validate the embedded public key. An absent or malformed key is a
/// hard error — we never fall back to "skip verification".
fn embedded_verifying_key() -> Result<VerifyingKey, UpdateError> {
    let b64 = EMBEDDED_PUBKEY_B64.ok_or_else(|| {
        UpdateError::Signature(
            "no update public key compiled in (set GPROXY_UPDATE_PUBKEY at build time)".to_string(),
        )
    })?;
    let raw = B64
        .decode(b64.trim())
        .map_err(|e| UpdateError::Signature(format!("embedded pubkey is not valid base64: {e}")))?;
    let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| {
        UpdateError::Signature(format!(
            "embedded pubkey must be 32 bytes, got {}",
            raw.len()
        ))
    })?;
    VerifyingKey::from_bytes(&arr).map_err(|e| {
        UpdateError::Signature(format!("embedded pubkey is not a valid ed25519 key: {e}"))
    })
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_matches_known_vector() {
        // sha256("") = e3b0c442...855
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // sha256("abc") known vector.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
