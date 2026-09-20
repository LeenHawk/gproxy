//! Gateway API keys, indexed by the digest of the key the client sends.
//!
//! The digest, not the key, is what the instance holds: `api_keys.key_hash` is
//! the **lowercase hex of the SHA-256** of the key text. Nothing in
//! `gproxy-store` fixes that encoding — the column is a plain `String` — so it
//! is fixed here, once, and every writer and reader goes through
//! [`encode_key_hash`] / [`decode_key_hash`]. The index is keyed by the decoded
//! 32 bytes rather than the text so that a lookup is one hash of the presented
//! key and one map probe, with no per-row string formatting and no accidental
//! case mismatch.
//!
//! Which digests to compute from a presented key (the raw text, and again with
//! an `sk-` prefix stripped) belongs to the authentication ladder, not here.
//! This index answers one question: whose key has this digest.

use crate::hex;
use gproxy_store::entity::identity::{
    api_key::{self, ApiKeyKind},
    user,
};
use std::collections::HashMap;

/// The caller identity a valid API key resolves to. Everything an admission
/// decision needs about the key itself, so the hot path never touches a row.
///
/// `organization_id` and `team_id` come from the key's own binding, never from
/// a request header: they decide the budget owner chain, the permission
/// subject and the credential-visibility boundary at once, and a client that
/// could choose them could choose who pays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiKeyIdentity {
    pub api_key_id: String,
    pub user_id: String,
    /// The instance-wide role of the owning user (`users.role`), copied so a
    /// lookup does not need a second map probe. Organization and team roles
    /// are memberships; see [`super::MembershipIndex`].
    pub user_role: String,
    pub organization_id: Option<String>,
    pub team_id: Option<String>,
    pub subscription_id: Option<String>,
    /// `OAuth` keys carry a grant's identity and are not bearer API keys. The
    /// authentication layer refuses to accept one presented as a bearer key.
    pub kind: ApiKeyKind,
    /// Always true for an indexed key: disabled rows are not indexed. Kept so
    /// a caller that holds only the identity can still report the state it was
    /// admitted under.
    pub enabled: bool,
    /// Kept even though expired rows are skipped at build time, because a key
    /// can expire while this snapshot is the live one. The authentication
    /// layer re-checks it against the current clock on every request.
    pub expires_at_ms: Option<i64>,
}

/// Digest → identity, for the keys that were usable when the snapshot was
/// assembled.
#[derive(Clone, Debug, Default)]
pub struct ApiKeyIndex {
    by_digest: HashMap<[u8; 32], ApiKeyIdentity>,
}

impl ApiKeyIndex {
    /// Index the keys that can serve a request at `now_ms`.
    ///
    /// A row is skipped when it is disabled, already expired, owned by a user
    /// who is disabled or absent, or carries a `key_hash` that is not 32 bytes
    /// of hex. Those are not errors — a disabled key is a normal state and an
    /// undecodable digest is an operator's or a migration's mistake — so they
    /// are counted and reported once instead of failing the assembly, which
    /// would take the whole instance down for one bad row.
    pub fn build(
        keys: &[api_key::Model],
        users: &HashMap<String, user::Model>,
        now_ms: i64,
    ) -> Self {
        let mut by_digest = HashMap::with_capacity(keys.len());
        let mut skipped = 0_usize;
        let mut undecodable = 0_usize;
        for key in keys {
            if !key.enabled || key.expires_at_ms.is_some_and(|at| at <= now_ms) {
                skipped += 1;
                continue;
            }
            let Some(user) = users.get(&key.user_id).filter(|user| user.enabled) else {
                skipped += 1;
                continue;
            };
            let Some(digest) = decode_key_hash(&key.key_hash) else {
                undecodable += 1;
                continue;
            };
            let identity = ApiKeyIdentity {
                api_key_id: key.id.clone(),
                user_id: key.user_id.clone(),
                user_role: user.role.clone(),
                organization_id: key.organization_id.clone(),
                team_id: key.team_id.clone(),
                subscription_id: key.subscription_id.clone(),
                kind: key.kind,
                enabled: true,
                expires_at_ms: key.expires_at_ms,
            };
            // Rows arrive ordered by id, so a collision (two rows whose
            // `key_hash` differs only in case) always resolves the same way.
            if by_digest.insert(digest, identity).is_some() {
                undecodable += 1;
            }
        }
        if undecodable > 0 {
            tracing::warn!(
                count = undecodable,
                "api key rows skipped: key_hash is not 32 bytes of hex, or collides with another row"
            );
        }
        tracing::debug!(
            indexed = by_digest.len(),
            skipped,
            "assembled the api key index"
        );
        Self { by_digest }
    }

    /// The identity behind a digest, without checking expiry: the caller has
    /// the clock and decides.
    pub fn lookup(&self, digest: &[u8; 32]) -> Option<&ApiKeyIdentity> {
        self.by_digest.get(digest)
    }

    pub fn len(&self) -> usize {
        self.by_digest.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_digest.is_empty()
    }
}

/// The stored form of a key digest: lowercase hex. Every writer of
/// `api_keys.key_hash` must use this, or its keys will not be found.
pub fn encode_key_hash(digest: &[u8; 32]) -> String {
    hex::encode(digest)
}

/// Read back a stored digest. Accepts either case, because hex is not
/// case-sensitive and refusing an uppercase row would silently disable a key
/// an operator wrote by hand; anything that is not exactly 32 bytes of hex is
/// refused rather than padded or truncated.
pub fn decode_key_hash(stored: &str) -> Option<[u8; 32]> {
    hex::decode(stored.trim())?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn digest_of(key: &str) -> [u8; 32] {
        Sha256::digest(key.as_bytes()).into()
    }

    fn user_row(id: &str, enabled: bool) -> user::Model {
        user::Model {
            id: id.into(),
            name: id.into(),
            password_hash: None,
            role: "user".into(),
            enabled,
            oauth_client_allowlist: None,
            created_at_ms: 0,
        }
    }

    fn users(rows: Vec<user::Model>) -> HashMap<String, user::Model> {
        rows.into_iter().map(|row| (row.id.clone(), row)).collect()
    }

    fn key_row(id: &str, user_id: &str, secret: &str) -> api_key::Model {
        api_key::Model {
            id: id.into(),
            user_id: user_id.into(),
            subscription_id: None,
            organization_id: None,
            team_id: None,
            name: id.into(),
            kind: ApiKeyKind::User,
            key_hash: encode_key_hash(&digest_of(secret)),
            prefix: "sk-".into(),
            secret: None,
            expires_at_ms: None,
            enabled: true,
        }
    }

    #[test]
    fn a_usable_key_is_found_by_the_digest_of_its_text() {
        let index = ApiKeyIndex::build(
            &[key_row("k1", "u1", "sk-live")],
            &users(vec![user_row("u1", true)]),
            1_000,
        );
        let found = index.lookup(&digest_of("sk-live")).unwrap();
        assert_eq!(found.api_key_id, "k1");
        assert_eq!(found.user_id, "u1");
        assert_eq!(found.user_role, "user");
        assert_eq!(found.kind, ApiKeyKind::User);
        assert!(found.enabled);
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn an_unknown_digest_is_a_miss() {
        let index = ApiKeyIndex::build(
            &[key_row("k1", "u1", "sk-live")],
            &users(vec![user_row("u1", true)]),
            1_000,
        );
        assert!(index.lookup(&digest_of("sk-other")).is_none());
        assert!(index.lookup(&[0_u8; 32]).is_none());
    }

    #[test]
    fn the_binding_and_kind_of_a_key_are_carried_as_stored() {
        let mut row = key_row("k1", "u1", "sk-live");
        row.organization_id = Some("org".into());
        row.team_id = Some("team".into());
        row.subscription_id = Some("sub".into());
        row.kind = ApiKeyKind::OAuth;
        let index = ApiKeyIndex::build(&[row], &users(vec![user_row("u1", true)]), 0);
        let found = index.lookup(&digest_of("sk-live")).unwrap();
        assert_eq!(found.organization_id.as_deref(), Some("org"));
        assert_eq!(found.team_id.as_deref(), Some("team"));
        assert_eq!(found.subscription_id.as_deref(), Some("sub"));
        assert_eq!(found.kind, ApiKeyKind::OAuth);
    }

    #[test]
    fn a_disabled_key_is_not_indexed() {
        let mut row = key_row("k1", "u1", "sk-live");
        row.enabled = false;
        let index = ApiKeyIndex::build(&[row], &users(vec![user_row("u1", true)]), 0);
        assert!(index.is_empty());
    }

    #[test]
    fn expiry_is_checked_against_the_assembly_clock() {
        let mut row = key_row("k1", "u1", "sk-live");
        row.expires_at_ms = Some(1_000);
        let live = ApiKeyIndex::build(
            std::slice::from_ref(&row),
            &users(vec![user_row("u1", true)]),
            999,
        );
        assert_eq!(
            live.lookup(&digest_of("sk-live")).unwrap().expires_at_ms,
            Some(1_000)
        );
        // The boundary is exclusive: at the stated instant the key is over.
        let expired = ApiKeyIndex::build(&[row], &users(vec![user_row("u1", true)]), 1_000);
        assert!(expired.is_empty());
    }

    #[test]
    fn a_disabled_or_missing_user_takes_its_keys_with_it() {
        let row = key_row("k1", "u1", "sk-live");
        let disabled = ApiKeyIndex::build(
            std::slice::from_ref(&row),
            &users(vec![user_row("u1", false)]),
            0,
        );
        assert!(disabled.is_empty());
        let absent = ApiKeyIndex::build(&[row], &users(vec![user_row("other", true)]), 0);
        assert!(absent.is_empty());
    }

    #[test]
    fn a_key_hash_that_is_not_a_digest_is_skipped_rather_than_fatal() {
        let mut bad = key_row("k1", "u1", "sk-live");
        bad.key_hash = "hash".into();
        let good = key_row("k2", "u1", "sk-good");
        let index = ApiKeyIndex::build(&[bad, good], &users(vec![user_row("u1", true)]), 0);
        assert_eq!(index.len(), 1);
        assert_eq!(
            index.lookup(&digest_of("sk-good")).unwrap().api_key_id,
            "k2"
        );
    }

    #[test]
    fn a_stored_digest_round_trips_in_either_case() {
        let digest = digest_of("sk-live");
        let stored = encode_key_hash(&digest);
        assert_eq!(stored.len(), 64);
        assert_eq!(stored, stored.to_lowercase());
        assert_eq!(decode_key_hash(&stored).unwrap(), digest);
        assert_eq!(decode_key_hash(&stored.to_uppercase()).unwrap(), digest);
        assert_eq!(decode_key_hash("  "), None);
        assert_eq!(decode_key_hash(&stored[..62]), None);
    }
}
