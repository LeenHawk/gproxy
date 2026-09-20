//! Console and portal passwords: argon2id, one PHC string per user.
//!
//! `users.password_hash` holds a full PHC string (`$argon2id$v=19$m=…`), not a
//! bare digest, so the parameters a hash was produced with travel with it and
//! raising them later does not invalidate every existing password.

use crate::AppError;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

/// Minimum length. Short enough not to lock an operator out of their own
/// instance, long enough that a leaked hash is not trivially reversed; argon2
/// does the rest of the work.
const MINIMUM_LENGTH: usize = 8;

/// Longer than this is refused rather than hashed. Argon2 has no practical
/// input limit, so this is not a cryptographic bound — it is a bound on how
/// much work an unauthenticated login attempt can ask the instance to do.
const MAXIMUM_LENGTH: usize = 1024;

/// The policy a new or changed password must satisfy.
///
/// v3 (`crates/gproxy-admin/src/auth/password.rs`) refused only a blank
/// password. That rule is kept — it is the one that matters, because a blank
/// password is an account with no password at all — and a minimum length and a
/// maximum length are added around it. Deliberately no composition rule beyond
/// that: mandatory character classes push users towards `Password1!` and
/// forbid long passphrases, which is the opposite of the goal.
///
/// The password is not trimmed. Leading and trailing whitespace is part of a
/// password a user chose; only an entirely blank one is refused, because that
/// is a user who typed nothing.
pub fn validate(password: &str) -> Result<(), AppError> {
    if password.trim().is_empty() {
        return Err(AppError::invalid("password must not be blank"));
    }
    if password.chars().count() < MINIMUM_LENGTH {
        return Err(AppError::invalid(format!(
            "password must be at least {MINIMUM_LENGTH} characters"
        )));
    }
    if password.len() > MAXIMUM_LENGTH {
        return Err(AppError::invalid(format!(
            "password must be at most {MAXIMUM_LENGTH} bytes"
        )));
    }
    Ok(())
}

/// Hash a password for storage. A fresh 16-byte salt per call, so two users
/// with the same password have different hashes and one cracked hash is one
/// cracked account.
///
/// Does not call [`validate`]: a caller that is re-hashing an existing
/// password under new parameters must not be made to re-validate it against a
/// policy that has since tightened.
pub fn hash(password: &str) -> Result<String, AppError> {
    let salt = super::api_key::random_bytes::<16>()?;
    Argon2::default()
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map(|hash: PasswordHash| hash.to_string())
        .map_err(|error| AppError::internal(format!("password hashing failed: {error}")))
}

/// Whether `password` produced `phc`.
///
/// Constant-time in the part that matters: the comparison of the computed
/// output against the stored one is `Output`'s own equality, which does not
/// short-circuit. It is not constant-time in the parameters — a hash with a
/// larger memory cost takes longer — and cannot be, since those are stored in
/// the clear in the PHC string.
///
/// A malformed, empty or foreign-algorithm hash is `false`. A user row whose
/// `password_hash` is unparseable is a user who cannot log in, never a panic
/// and never an accidental success.
pub fn verify(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_verifies_against_its_own_hash() {
        let phc = hash("correct horse battery staple").unwrap();
        assert!(phc.starts_with("$argon2id$"));
        assert!(verify("correct horse battery staple", &phc));
    }

    #[test]
    fn a_wrong_password_does_not() {
        let phc = hash("correct horse battery staple").unwrap();
        assert!(!verify("correct horse battery stapler", &phc));
        assert!(!verify("", &phc));
        assert!(!verify("Correct horse battery staple", &phc));
    }

    #[test]
    fn the_same_password_hashes_differently_every_time() {
        let first = hash("correct horse battery staple").unwrap();
        let second = hash("correct horse battery staple").unwrap();
        assert_ne!(first, second, "the salt is not fresh");
        assert!(verify("correct horse battery staple", &first));
        assert!(verify("correct horse battery staple", &second));
    }

    #[test]
    fn a_malformed_hash_is_a_failed_verification_not_a_panic() {
        for stored in [
            "",
            "   ",
            "not a phc string",
            "$argon2id$",
            "$argon2id$v=19$m=19456,t=2,p=1",
            // Well-formed PHC, unknown algorithm.
            "$pbkdf2$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2g",
        ] {
            assert!(!verify("correct horse battery staple", stored), "{stored}");
        }
    }

    #[test]
    fn a_truncated_hash_does_not_verify() {
        let phc = hash("correct horse battery staple").unwrap();
        assert!(!verify(
            "correct horse battery staple",
            &phc[..phc.len() - 4]
        ));
    }

    #[test]
    fn the_policy_refuses_what_is_not_a_password() {
        for bad in ["", "   ", "\n\t", "short", "1234567"] {
            let error = validate(bad).unwrap_err();
            assert_eq!(error.status_code(), 400, "{bad}");
        }
        let error = validate(&"a".repeat(MAXIMUM_LENGTH + 1)).unwrap_err();
        assert_eq!(error.status_code(), 400);
    }

    #[test]
    fn the_policy_accepts_a_passphrase_without_demanding_punctuation() {
        for good in [
            "12345678",
            "correct horse battery staple",
            "  padded  ",
            "两个汉字都不够但是这一串够了",
            &"a".repeat(MAXIMUM_LENGTH),
        ] {
            assert!(validate(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn length_is_counted_in_characters_and_capped_in_bytes() {
        // Eight characters, more than eight bytes: accepted.
        assert!(validate("héllo wö").is_ok());
        assert!(validate("汉字汉字汉字汉字").is_ok());
        // Seven characters: refused, however many bytes they take.
        assert!(validate("汉字汉字汉字汉").is_err());
        // The cap is on bytes, because that is what the hasher is asked to
        // work over: a thousand multi-byte characters is over it.
        assert!(validate(&"汉".repeat(MAXIMUM_LENGTH)).is_err());
    }
}
