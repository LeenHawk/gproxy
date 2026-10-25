//! Proof Key for Code Exchange, and the CSRF state that travels beside it.
//!
//! The verifier never leaves this process: the authorize URL carries only its
//! SHA-256 digest, and the verifier itself waits in the login session until the
//! token exchange. That is the whole point of PKCE — an authorization code
//! intercepted on the way back is useless without the secret that was never
//! sent. Only S256 is produced; `plain` is a downgrade this crate does not
//! offer, and no channel asks for it.

use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::{SdkError, SdkResult};

/// RFC 7636 allows 43..=128 verifier characters: 32..=96 random bytes once
/// base64url-encoded. The channel picks the size its upstream client uses.
const VERIFIER_BYTES: std::ops::RangeInclusive<usize> = 32..=96;

/// A verifier and the challenge derived from it.
pub(crate) struct Pkce {
    pub(crate) verifier: String,
    pub(crate) challenge: String,
}

pub(crate) fn pkce(verifier_bytes: usize) -> SdkResult<Pkce> {
    let mut bytes =
        vec![0_u8; verifier_bytes.clamp(*VERIFIER_BYTES.start(), *VERIFIER_BYTES.end())];
    // Unlike a row id, a guessable verifier is a vulnerability rather than a
    // collision risk, so entropy failure is fatal here instead of tolerated.
    getrandom::fill(&mut bytes)
        .map_err(|_| SdkError::invalid("secure randomness is unavailable"))?;
    let verifier = encode(&bytes);
    let challenge = challenge(&verifier);
    Ok(Pkce {
        verifier,
        challenge,
    })
}

/// The S256 challenge of a verifier: base64url of the SHA-256 of its ASCII
/// characters, never of the bytes they were encoded from.
pub(crate) fn challenge(verifier: &str) -> String {
    encode(&Sha256::digest(verifier.as_bytes()))
}

fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::{challenge, pkce};

    /// The vector from RFC 7636 appendix B, which is what an upstream verifies
    /// the exchange against.
    #[test]
    fn s256_matches_the_rfc_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_verifier_is_fresh_and_url_safe() {
        let first = pkce(32).unwrap();
        let second = pkce(32).unwrap();
        assert_ne!(first.verifier, second.verifier);
        assert_eq!(first.challenge, challenge(&first.verifier));
        assert!(
            first
                .verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        // RFC 7636 requires 43..=128 characters; 32 bytes encode to exactly 43.
        assert_eq!(first.verifier.len(), 43);
        assert_eq!(pkce(48).unwrap().verifier.len(), 64);
    }
}
