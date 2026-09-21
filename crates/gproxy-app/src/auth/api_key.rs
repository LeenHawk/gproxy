//! The gateway API key half of the ladder: which digests a presented token can
//! have been stored under, and how a new key is minted.

use super::{Authenticator, Caller};
use crate::{
    AppError,
    snapshot::{ApiKeyIdentity, encode_key_hash},
};
use base64::Engine;
use gproxy_store::entity::identity::api_key::ApiKeyKind;
use sha2::{Digest, Sha256};

/// The presentation prefix new keys are minted with.
pub const API_KEY_PREFIX: &str = "sk-";

/// The presentation prefixes that are *not* part of the secret.
///
/// `sk-` is the OpenAI shape and `at-` is a personal-access-token shape. Some
/// channels refuse a token that does not look like a PAT, so the same key is
/// handed to one upstream as `at-…` and to another as `sk-…`. v3 solved that
/// by digesting only the payload (`crates/gproxy-app/src/control/user_key.rs`),
/// which keeps both spellings one identity so quota and usage follow the key
/// rather than how it was typed.
const PRESENTATION_PREFIXES: [&str; 2] = ["sk-", "at-"];

/// Every digest a stored key row could hold for this token, in the order they
/// are tried.
///
/// Two, at most: the SHA-256 of the raw text, and the SHA-256 of the text with
/// one leading presentation prefix removed. The raw digest comes first because
/// it is what [`generate_api_key`] writes — a v4 key is stored under the
/// digest of the whole `sk-…` string. The stripped digest is second because it
/// is what v3 wrote, and because it is what makes `sk-X`, `at-X` and bare `X`
/// one key: present any of the three and the payload digest is identical.
///
/// A token with no prefix yields one digest; the two are equal and the second
/// is not emitted, so a lookup never probes the same map twice.
pub fn digests(token: &str) -> Vec<[u8; 32]> {
    let raw: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let mut out = vec![raw];
    if let Some(payload) = strip_presentation_prefix(token) {
        let stripped: [u8; 32] = Sha256::digest(payload.as_bytes()).into();
        if stripped != raw {
            out.push(stripped);
        }
    }
    out
}

/// The secret inside a presented token, or None when it carries no known
/// presentation prefix.
fn strip_presentation_prefix(token: &str) -> Option<&str> {
    PRESENTATION_PREFIXES
        .iter()
        .find_map(|prefix| token.strip_prefix(prefix))
}

/// Mint a key: the text handed to the user once, and the two columns the row
/// keeps.
///
/// The three returned strings are, in order, the **full token** the client
/// will present, the **`prefix` column** for display in a list, and the
/// **`key_hash` column**. 32 random bytes, base64 URL-safe without padding,
/// behind `prefix` (normally [`API_KEY_PREFIX`]). The display prefix is the
/// first eight characters of the body — after the presentation prefix, which
/// is the same on every key and therefore distinguishes nothing.
///
/// Fallible only because entropy is: a key built from a failed fill would be
/// all zeroes and identical on every instance, so that must be an error and
/// never a value. The plaintext is returned here and nowhere else; the row
/// holds only the digest.
pub fn generate_api_key(prefix: &str) -> Result<(String, String, String), AppError> {
    let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
    let display: String = body.chars().take(8).collect();
    let token = format!("{prefix}{body}");
    let key_hash = encode_key_hash(&Sha256::digest(token.as_bytes()).into());
    Ok((token, display, key_hash))
}

/// The two columns a row keeps for a token the caller already has, rather than
/// one this module generated.
///
/// The same digest [`generate_api_key`] writes — the SHA-256 of the whole text —
/// so an adopted key is found on the first rung of [`digests`] exactly like a
/// minted one. That is the whole point of routing this through here: v3 stored a
/// bootstrap key under the digest of its *payload* while its console stored
/// minted keys under a different rule, and the mismatch meant an operator's
/// `sk-`-prefixed bootstrap key authenticated on some surfaces and answered 401
/// on others. There is one answer to "hashed how" in this crate, and it is this
/// file.
///
/// The display prefix is the first eight characters of the body, after any
/// presentation prefix, matching what a minted key shows in a list. A token too
/// short to have eight is shown as far as it goes rather than padded.
pub fn adopt_api_key(token: &str) -> Result<(String, String), AppError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AppError::invalid("api key must not be blank"));
    }
    let body = strip_presentation_prefix(token).unwrap_or(token);
    let display: String = body.chars().take(8).collect();
    let key_hash = encode_key_hash(&Sha256::digest(token.as_bytes()).into());
    Ok((display, key_hash))
}

/// Cryptographically secure bytes, or an error. Never a zeroed buffer.
pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N], AppError> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| AppError::internal("secure randomness is unavailable"))?;
    Ok(bytes)
}

/// The digest a credential is stored under: the SHA-256 of the text exactly as
/// presented.
///
/// One function for all of them, because "hashed how" must have a single
/// answer in this crate. What differs between the columns is only the
/// *encoding* of the result: `api_keys.key_hash` and `user_sessions.token_hash`
/// are text and go through
/// [`encode_key_hash`](crate::snapshot::encode_key_hash), while the OAuth
/// tables (`oauth_tokens.token_hash`, `oauth_codes.code_hash`,
/// `oauth_devices.device_code_hash`) are `Binary(32)` and store these bytes.
///
/// No presentation prefix is stripped here — that ladder belongs to
/// [`digests`], and it exists so one gateway key can be typed three ways. A
/// token the issuer minted has exactly one spelling, and widening what it can
/// match would only ever help an attacker.
pub(crate) fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

impl<C> Authenticator<'_, C> {
    /// Step (a) and (b) of the ladder: compute the candidate digests, probe
    /// the snapshot's key index, and turn a live hit into a caller.
    ///
    /// Synchronous and allocation-light: this is the hot path, and it is two
    /// hashes and at most two map probes with no database round trip.
    ///
    /// Three outcomes:
    /// - `Ok(Some(caller))` — a live gateway key;
    /// - `Ok(None)` — no key has either digest, so the OAuth path gets a turn;
    /// - `Err(Unauthorized)` — a key *does* have one of these digests but
    ///   cannot serve the request. That is deliberately not a miss. A
    ///   `kind = OAuth` key is a grant's internal key and must never work as a
    ///   bearer key, and an expired key must not fall through to be re-tried
    ///   as something else.
    pub fn authenticate_api_key(
        &self,
        token: &str,
        now_ms: i64,
    ) -> Result<Option<Caller>, AppError> {
        let Some(identity) = digests(token)
            .iter()
            .find_map(|digest| self.snapshot().keys.lookup(digest))
        else {
            return Ok(None);
        };
        self.caller_for_key(identity, now_ms).map(Some)
    }

    /// The checks that outlive the snapshot they were indexed under: a key can
    /// expire, or be disabled, while this snapshot is still the live one.
    pub(super) fn caller_for_key(
        &self,
        identity: &ApiKeyIdentity,
        now_ms: i64,
    ) -> Result<Caller, AppError> {
        if identity.kind == ApiKeyKind::OAuth {
            return Err(AppError::Unauthorized(
                "oauth keys cannot authenticate directly",
            ));
        }
        if !identity.is_live_at(now_ms) {
            return Err(AppError::Unauthorized("api key is expired or disabled"));
        }
        Ok(Caller::from_key(identity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest_of(text: &str) -> [u8; 32] {
        Sha256::digest(text.as_bytes()).into()
    }

    #[test]
    fn a_prefixed_token_is_tried_raw_and_then_stripped() {
        assert_eq!(
            digests("sk-abc"),
            vec![digest_of("sk-abc"), digest_of("abc")]
        );
        assert_eq!(
            digests("at-abc"),
            vec![digest_of("at-abc"), digest_of("abc")]
        );
    }

    #[test]
    fn the_three_spellings_of_one_key_share_their_payload_digest() {
        let payload = digest_of("abc");
        for spelling in ["sk-abc", "at-abc", "abc"] {
            assert!(
                digests(spelling).contains(&payload),
                "{spelling} lost the payload digest"
            );
        }
    }

    #[test]
    fn an_unprefixed_token_yields_exactly_one_digest() {
        assert_eq!(digests("abc"), vec![digest_of("abc")]);
        // Only the presentation prefixes are stripped, and only one of them.
        assert_eq!(digests("pk-abc"), vec![digest_of("pk-abc")]);
        assert_eq!(
            digests("sk-at-abc"),
            vec![digest_of("sk-at-abc"), digest_of("at-abc")]
        );
    }

    #[test]
    fn a_bare_prefix_is_a_token_whose_payload_is_empty() {
        // Not special-cased: the empty payload has a digest like any other,
        // and no row can hold it unless someone stored an empty key.
        assert_eq!(digests("sk-"), vec![digest_of("sk-"), digest_of("")]);
    }

    #[test]
    fn a_minted_key_is_stored_under_the_digest_of_its_whole_text() {
        let (token, display, key_hash) = generate_api_key(API_KEY_PREFIX).unwrap();
        assert!(token.starts_with("sk-"));
        assert_eq!(
            key_hash,
            crate::snapshot::encode_key_hash(&digest_of(&token))
        );
        assert_eq!(display.len(), 8);
        assert_eq!(token["sk-".len()..][..8], display);
        // 32 bytes, base64 without padding.
        assert_eq!(token.len(), "sk-".len() + 43);
        assert!(!token.contains('='));
        // And the ladder finds it under exactly that digest.
        assert_eq!(digests(&token)[0], digest_of(&token));
    }

    #[test]
    fn two_minted_keys_are_not_the_same_key() {
        let (first, _, _) = generate_api_key(API_KEY_PREFIX).unwrap();
        let (second, _, _) = generate_api_key(API_KEY_PREFIX).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn an_adopted_key_is_stored_exactly_where_a_minted_one_would_be() {
        let (token, display, key_hash) = generate_api_key(API_KEY_PREFIX).unwrap();
        let (adopted_display, adopted_hash) = adopt_api_key(&token).unwrap();
        assert_eq!(adopted_display, display);
        assert_eq!(adopted_hash, key_hash);
    }

    #[test]
    fn an_adopted_key_is_found_on_the_first_rung_of_the_ladder() {
        // The v3 bug this exists to prevent: a bootstrap key spelled `sk-…`
        // stored under the payload digest while the lookup tried the whole text
        // first. Both sides now agree on the whole text.
        for token in [
            "sk-operator-supplied",
            "at-operator-supplied",
            "plain-token",
        ] {
            let (_, key_hash) = adopt_api_key(token).unwrap();
            assert_eq!(
                key_hash,
                crate::snapshot::encode_key_hash(&digests(token)[0])
            );
        }
    }

    #[test]
    fn an_adopted_key_shows_the_body_not_the_presentation_prefix() {
        let (display, _) = adopt_api_key("sk-abcdefghijkl").unwrap();
        assert_eq!(display, "abcdefgh");
        // Shorter than the display width is shown as far as it goes.
        let (short, _) = adopt_api_key("sk-abc").unwrap();
        assert_eq!(short, "abc");
    }

    #[test]
    fn a_blank_adopted_key_is_refused() {
        assert!(adopt_api_key("   ").is_err());
        assert!(adopt_api_key("").is_err());
    }
}
