//! Detached ed25519 verification, ported from v3's
//! `gproxy-host-axum/src/signature.rs`.
//!
//! `verify_strict` rather than `verify`: it rejects the small-order public keys
//! and the non-canonical `R` values that make a signature verify under more
//! than one key. Nothing here has a use for a signature that two keys accept.

use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};

use super::config::{SIGNING_PUBLIC_KEY, UpdateError};

/// Verify `signature` over `bytes` under the key compiled into this binary.
pub(super) fn verify_detached(bytes: &[u8], signature: &str) -> Result<(), UpdateError> {
    verify_detached_with_key(bytes, signature, SIGNING_PUBLIC_KEY)
}

/// The same, against a key given here.
///
/// Exists so the tests can sign a manifest with a key pair they generated,
/// which is the only way to exercise this path at all: a release build takes
/// the key from the build environment, and a checkout has none.
pub(super) fn verify_detached_with_key(
    bytes: &[u8],
    signature: &str,
    public_key: Option<&str>,
) -> Result<(), UpdateError> {
    let encoded = public_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(UpdateError::NoSigningKey)?;
    let key = decode::<32>(encoded)?;
    let signature = decode::<64>(signature.trim())?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| UpdateError::Signature)?;
    key.verify_strict(bytes, &Signature::from_bytes(&signature))
        .map_err(|_| UpdateError::Signature)
}

fn decode<const N: usize>(value: &str) -> Result<[u8; N], UpdateError> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| UpdateError::Signature)?
        .try_into()
        .map_err(|_| UpdateError::Signature)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer as _, SigningKey};

    use super::*;

    fn encode(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn a_signature_verifies_under_its_own_key_and_nothing_else() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let other = SigningKey::from_bytes(&[8; 32]);
        let public = encode(key.verifying_key().as_bytes());
        let signature = encode(&key.sign(b"payload").to_bytes());

        verify_detached_with_key(b"payload", &signature, Some(&public)).unwrap();

        // A different payload, a different key, and a truncated signature.
        for (bytes, signature, public_key) in [
            (&b"payload!"[..], signature.as_str(), public.as_str()),
            (&b"payload"[..], signature.as_str(), {
                let other = encode(other.verifying_key().as_bytes());
                Box::leak(other.into_boxed_str())
            }),
            (&b"payload"[..], "not base64 at all", public.as_str()),
        ] {
            assert!(
                verify_detached_with_key(bytes, signature, Some(public_key)).is_err(),
                "accepted a signature it should not have"
            );
        }
    }

    #[test]
    fn a_build_with_no_key_refuses_rather_than_accepting_anything() {
        let error = verify_detached_with_key(b"payload", "AA==", None).unwrap_err();
        assert!(matches!(error, UpdateError::NoSigningKey), "{error}");
        // And an empty one counts as none, because that is what an unset CI
        // secret expands to.
        let error = verify_detached_with_key(b"payload", "AA==", Some("  ")).unwrap_err();
        assert!(matches!(error, UpdateError::NoSigningKey), "{error}");
    }
}
