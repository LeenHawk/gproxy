//! Admin password hashing (§14.2): argon2id PHC strings. `verify` is used by
//! the M10 login path; both compile on all targets (argon2 is pure Rust).
//!
//! The salt is drawn from [`crate::util::rand`] (cross-target getrandom) and
//! base64-encoded into a [`SaltString`], rather than `SaltString::generate`,
//! so we don't depend on `password-hash`'s `OsRng`/`getrandom` feature path
//! (argon2's default features give `rand_core` but not `rand_core/getrandom`).

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

/// Minimum length (after trim) for a NEW admin password.
pub const MIN_PASSWORD_CHARS: usize = 12;

/// Policy gate for NEW plaintext passwords (admin user upsert + the
/// `--admin-password` bootstrap override): reject blank/whitespace-only and
/// anything under [`MIN_PASSWORD_CHARS`]. `None` (no password) is handled by
/// the callers — it simply means password login is disabled for that user.
/// First-boot generated passwords (32 chars, CSPRNG) pass trivially.
pub fn validate_new(password: &str) -> Result<(), String> {
    let trimmed = password.trim();
    if trimmed.is_empty() {
        return Err("password must not be blank".into());
    }
    if trimmed.chars().count() < MIN_PASSWORD_CHARS {
        return Err(format!(
            "password must be at least {MIN_PASSWORD_CHARS} characters"
        ));
    }
    Ok(())
}

/// Hash a password into an argon2id PHC string (suitable for storage).
pub fn hash(password: &str) -> anyhow::Result<String> {
    let salt_bytes = crate::util::rand::bytes::<16>();
    let salt =
        SaltString::encode_b64(&salt_bytes).map_err(|e| anyhow::anyhow!("argon2 salt: {e}"))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("argon2 hash: {e}"))
}

/// Verify a password against a stored PHC string. A malformed PHC is a
/// non-match (never a panic).
pub fn verify(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_roundtrip() {
        let phc = hash("correct horse battery staple").unwrap();
        assert!(verify("correct horse battery staple", &phc));
        assert!(!verify("wrong password", &phc));
    }

    #[test]
    fn verify_rejects_garbage_phc() {
        assert!(!verify("anything", "not-a-phc-string"));
    }
}
