//! What every host does around a sign-in, whatever it stores the session in:
//! Proof Key for Code Exchange, and reading the authorization code a person
//! brings back.
//!
//! The verifier never leaves the host: the authorize URL carries only its
//! SHA-256 digest, and the verifier itself waits in the login session until the
//! token exchange. That is the whole point of PKCE — an authorization code
//! intercepted on the way back is useless without the secret that was never
//! sent. Only S256 is produced; `plain` is a downgrade no kind asks for.

use base64::Engine as _;
use sha2::{Digest, Sha256};

/// RFC 7636 allows 43..=128 verifier characters: 32..=96 random bytes once
/// base64url-encoded. The channel picks the size its upstream client uses.
const VERIFIER_BYTES: std::ops::RangeInclusive<usize> = 32..=96;

/// A verifier and the challenge derived from it.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn pkce(verifier_bytes: usize) -> Result<Pkce, getrandom::Error> {
    let mut bytes =
        vec![0_u8; verifier_bytes.clamp(*VERIFIER_BYTES.start(), *VERIFIER_BYTES.end())];
    // Unlike a row id, a guessable verifier is a vulnerability rather than a
    // collision risk, so entropy failure is fatal here instead of tolerated.
    getrandom::fill(&mut bytes)?;
    let verifier = encode(&bytes);
    let challenge = challenge(&verifier);
    Ok(Pkce {
        verifier,
        challenge,
    })
}

/// The S256 challenge of a verifier: base64url of the SHA-256 of its ASCII
/// characters, never of the bytes they were encoded from.
pub fn challenge(verifier: &str) -> String {
    encode(&Sha256::digest(verifier.as_bytes()))
}

fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Why what a person handed back cannot finish a login.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct CallbackError(pub &'static str);

/// The authorization code and, when there is one, the state that must match.
/// Exactly one of `callback_url` and `code` is accepted, because the two could
/// disagree and there is no principled winner.
pub fn authorization_code(
    callback_url: Option<&str>,
    code: Option<&str>,
    state: Option<&str>,
) -> Result<(String, Option<String>), CallbackError> {
    match (non_blank(callback_url), non_blank(code)) {
        (Some(url), None) => callback(url),
        (None, Some(code)) => Ok((code.to_owned(), non_blank(state).map(str::to_owned))),
        (Some(_), Some(_)) => Err(CallbackError(
            "provide either callbackUrl or code, not both",
        )),
        (None, None) => Err(CallbackError(
            "provide the callbackUrl or the authorization code",
        )),
    }
}

/// `code` and `state` out of a callback's query. A bare query string and a
/// site-relative path are both accepted: a browser hands a console
/// `location.search`, not an absolute URL.
fn callback(url: &str) -> Result<(String, Option<String>), CallbackError> {
    if let Some((code, state)) = pasted_code(url) {
        return Ok((code.to_owned(), Some(state.to_owned())));
    }
    let malformed = || CallbackError("the login callback is not a valid URL");
    let base = url::Url::parse("http://callback.invalid/").expect("a constant URL parses");
    let parsed = url::Url::options()
        .base_url(Some(&base))
        .parse(url)
        .map_err(|_| malformed())?;
    let mut code = None;
    let mut state = None;
    for (name, value) in parsed.query_pairs() {
        match name.as_ref() {
            "code" => code = non_blank(Some(value.as_ref())).map(str::to_owned),
            "state" => state = non_blank(Some(value.as_ref())).map(str::to_owned),
            _ => {}
        }
    }
    let code = code.ok_or(CallbackError("the login callback carries no code"))?;
    if state.is_none() {
        // Every session a host starts has a state, so a callback without one
        // cannot be the answer to it.
        return Err(CallbackError(
            "the login callback carries no authorization state",
        ));
    }
    Ok((code, state))
}

/// The `code#state` text a provider's own callback page shows for copying
/// instead of leaving the result in the address bar (Claude does). Anything
/// that looks like a URL or a query is left to [`callback`].
pub fn pasted_code(text: &str) -> Option<(&str, &str)> {
    let (code, state) = text.trim().split_once('#')?;
    let plain = |part: &str| {
        !part.is_empty() && !part.contains(|c: char| c.is_whitespace() || "/?&=#".contains(c))
    };
    (plain(code) && plain(state)).then_some((code, state))
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{authorization_code, callback, challenge, pkce};

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

    #[test]
    fn a_callback_or_a_bare_code_but_not_both() {
        assert_eq!(
            authorization_code(
                Some("http://localhost/callback?code=from-callback&state=s1"),
                None,
                None
            )
            .unwrap(),
            ("from-callback".into(), Some("s1".into()))
        );
        assert_eq!(
            authorization_code(None, Some("  bare  "), Some("s1")).unwrap(),
            ("bare".into(), Some("s1".into()))
        );
        assert!(authorization_code(Some("http://x/?code=a&state=s"), Some("b"), None).is_err());
        assert!(authorization_code(None, None, Some("s1")).is_err());
    }

    #[test]
    fn a_bare_query_string_is_a_callback() {
        assert_eq!(
            callback("?code=a&state=s1").unwrap(),
            ("a".into(), Some("s1".into()))
        );
        assert_eq!(
            callback("/oauth/callback?state=s1&code=a").unwrap(),
            ("a".into(), Some("s1".into()))
        );
    }

    #[test]
    fn a_callback_without_a_code_or_a_state_is_refused() {
        assert!(callback("http://localhost/?state=s1").is_err());
        assert!(callback("http://localhost/?code=a").is_err());
        assert!(callback("http://localhost/?code=&state=s1").is_err());
    }
}
