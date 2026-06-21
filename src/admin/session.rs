//! Admin session: opaque cache-backed token (§14.2). Revocable, sliding TTL.
//!
//! The token is server-side state (a random id keyed into the cache), NOT a
//! JWT: it is revocable and is re-validated against persistence on every
//! request, so disabling or demoting an admin mid-session takes effect at once.
//! cache + persistence only — compiles on native and edge (axum-free).

use std::time::Duration;

use base64::Engine as _;

use crate::store::cache::{CacheBackend, CacheError};
use crate::store::persistence::PersistenceBackend;

/// Sliding session lifetime; refreshed on each successful validate.
pub const SESSION_TTL: Duration = Duration::from_secs(12 * 3600);
const COOKIE_NAME: &str = "gproxy_session";

const B64URL: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Authenticated admin identity attached to a request after the middleware.
#[derive(Debug, Clone)]
pub struct AdminUser {
    pub id: i64,
    pub name: String,
}

/// Portal/session identity — admits ANY enabled user (admin or not). Carries
/// is_admin + org/team so /user/* can scope effective rules without a re-fetch.
#[derive(Debug, Clone)]
pub struct SessionUser {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
    pub org_id: i64,
    pub team_id: Option<i64>,
}

/// Mint a session: random opaque token → cache `sess:{token}` = user_id.
///
/// A failed cache write is an error: returning the token anyway would hand the
/// client a credential that 401s on every subsequent request.
pub async fn create(cache: &dyn CacheBackend, user_id: i64) -> Result<String, CacheError> {
    let token = B64URL.encode(crate::util::rand::bytes::<32>());
    cache
        .set(
            &key(&token),
            user_id.to_le_bytes().to_vec(),
            Some(SESSION_TTL),
        )
        .await?;
    Ok(token)
}

/// Validate a token → the live admin user (re-checked against persistence so a
/// disabled/demoted user is rejected mid-session). Refreshes the sliding TTL.
pub async fn validate(
    cache: &dyn CacheBackend,
    db: &dyn PersistenceBackend,
    token: &str,
) -> Option<AdminUser> {
    let raw = cache.get(&key(token)).await?;
    let bytes: [u8; 8] = raw.try_into().ok()?;
    let user_id = i64::from_le_bytes(bytes);
    let user = db.get_user(user_id).await.ok().flatten()?;
    if !user.enabled || !user.is_admin {
        return None;
    }
    // Slide the TTL on each successful use — best-effort: a failed refresh
    // just means the session expires on the original schedule.
    let _ = cache
        .set(
            &key(token),
            user_id.to_le_bytes().to_vec(),
            Some(SESSION_TTL),
        )
        .await;
    Some(AdminUser {
        id: user.id,
        name: user.name,
    })
}

/// Like `validate` but does NOT require is_admin (only `enabled`). Refreshes the
/// sliding TTL identically. Returns the full session identity.
pub async fn validate_session(
    cache: &dyn CacheBackend,
    db: &dyn PersistenceBackend,
    token: &str,
) -> Option<SessionUser> {
    let raw = cache.get(&key(token)).await?;
    let bytes: [u8; 8] = raw.try_into().ok()?;
    let user_id = i64::from_le_bytes(bytes);
    let user = db.get_user(user_id).await.ok().flatten()?;
    if !user.enabled {
        return None; // NOTE: no is_admin gate
    }
    // Slide the TTL on each successful use — best-effort: a failed refresh
    // just means the session expires on the original schedule.
    let _ = cache
        .set(
            &key(token),
            user_id.to_le_bytes().to_vec(),
            Some(SESSION_TTL),
        )
        .await;
    Some(SessionUser {
        id: user.id,
        name: user.name,
        is_admin: user.is_admin,
        org_id: user.org_id,
        team_id: user.team_id,
    })
}

/// Revoke a session (logout): drop its cache entry.
pub async fn revoke(cache: &dyn CacheBackend, token: &str) {
    cache.delete(&key(token)).await;
}

fn key(token: &str) -> String {
    format!("sess:{token}")
}

/// The session cookie name.
pub fn cookie_name() -> &'static str {
    COOKIE_NAME
}

/// Parse the `gproxy_session` value out of a raw `Cookie` request header.
pub fn parse_cookie(cookie_header: &str) -> Option<&str> {
    cookie_header.split(';').find_map(|kv| {
        let kv = kv.trim();
        kv.strip_prefix(COOKIE_NAME)
            .and_then(|rest| rest.strip_prefix('='))
            .filter(|v| !v.is_empty())
    })
}

/// `Set-Cookie` value for a fresh session. `secure` gates the `Secure` attr.
/// `cross_origin`: when `true`, uses `SameSite=None; Secure` so the cookie
/// flows on cross-site XHR from the admin console. SameSite=None REQUIRES
/// Secure; therefore `cross_origin=true` forces `secure=true` regardless of
/// the env flag. Cross-origin admin always requires HTTPS.
pub fn set_cookie(token: &str, secure: bool, cross_origin: bool) -> String {
    let same_site = if cross_origin { "None" } else { "Lax" };
    // SameSite=None requires Secure; force it in cross-origin mode.
    let secure = secure || cross_origin;
    let mut c = format!(
        "{COOKIE_NAME}={token}; HttpOnly; SameSite={same_site}; Path=/; Max-Age={}",
        SESSION_TTL.as_secs()
    );
    if secure {
        c.push_str("; Secure");
    }
    c
}

/// `Set-Cookie` value clearing the session. `cross_origin` mirrors `set_cookie`.
pub fn clear_cookie(secure: bool, cross_origin: bool) -> String {
    let same_site = if cross_origin { "None" } else { "Lax" };
    let secure = secure || cross_origin;
    let mut c = format!("{COOKIE_NAME}=; HttpOnly; SameSite={same_site}; Path=/; Max-Age=0");
    if secure {
        c.push_str("; Secure");
    }
    c
}

/// Whether to set the `Secure` cookie attr. Secure by default; disabled only
/// when `GPROXY_INSECURE_COOKIES=1` (local plaintext-HTTP development).
pub fn cookies_secure() -> bool {
    std::env::var("GPROXY_INSECURE_COOKIES")
        .map(|v| v != "1")
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::cache::MemoryCache;
    use crate::store::persistence::FilePersistence;
    use crate::store::persistence::records::{OrgInput, UserInput};

    async fn store() -> (tempfile::TempDir, FilePersistence) {
        let dir = tempfile::tempdir().expect("tempdir");
        let fp = FilePersistence::open(dir.path().to_path_buf())
            .await
            .expect("open");
        (dir, fp)
    }

    async fn seed_user(db: &FilePersistence, enabled: bool, is_admin: bool) -> i64 {
        let org = db
            .upsert_org(OrgInput {
                id: None,
                name: "default".to_string(),
                enabled: true,
                description: None,
            })
            .await
            .unwrap();
        db.upsert_user(UserInput {
            id: None,
            name: "admin".to_string(),
            org_id: org.id,
            team_id: None,
            password: Some(crate::crypto::password::hash("pw").unwrap()),
            enabled,
            is_admin,
        })
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn session_create_validate_roundtrip() {
        let (_dir, db) = store().await;
        let cache = MemoryCache::new();
        let uid = seed_user(&db, true, true).await;

        let token = create(&cache, uid).await.expect("create");
        let admin = validate(&cache, &db, &token).await.expect("valid session");
        assert_eq!(admin.id, uid);
        assert_eq!(admin.name, "admin");

        revoke(&cache, &token).await;
        assert!(validate(&cache, &db, &token).await.is_none());
    }

    #[tokio::test]
    async fn validate_rejects_non_admin_or_disabled() {
        let (_dir, db) = store().await;
        let cache = MemoryCache::new();

        // Non-admin (enabled) user: rejected.
        let uid = seed_user(&db, true, false).await;
        let token = create(&cache, uid).await.expect("create");
        assert!(validate(&cache, &db, &token).await.is_none());

        // Unknown / tampered tokens: rejected.
        assert!(validate(&cache, &db, "no-such-token").await.is_none());
        let tampered = format!("{token}x");
        assert!(validate(&cache, &db, &tampered).await.is_none());
    }

    #[test]
    fn set_cookie_same_origin_has_lax_no_secure_when_insecure() {
        let c = set_cookie("tok", false, false);
        assert!(c.contains("SameSite=Lax"), "{c}");
        assert!(!c.contains("Secure"), "{c}");
    }

    #[test]
    fn set_cookie_cross_origin_has_none_and_secure() {
        let c = set_cookie("tok", false, true);
        assert!(c.contains("SameSite=None"), "{c}");
        assert!(c.contains("Secure"), "{c}");
    }

    #[test]
    fn clear_cookie_cross_origin_has_none_and_secure() {
        let c = clear_cookie(false, true);
        assert!(c.contains("SameSite=None"), "{c}");
        assert!(c.contains("Secure"), "{c}");
    }

    #[test]
    fn clear_cookie_same_origin_has_lax() {
        let c = clear_cookie(false, false);
        assert!(c.contains("SameSite=Lax"), "{c}");
    }
}
