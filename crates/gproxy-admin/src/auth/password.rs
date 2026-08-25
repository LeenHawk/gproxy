use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::AdminError;

pub(super) fn validate(password: &str) -> Result<(), AdminError> {
    if password.trim().is_empty() {
        Err(AdminError::BadRequest("password must not be blank".into()))
    } else {
        Ok(())
    }
}

pub(super) fn hash(password: &str) -> Result<String, AdminError> {
    let mut salt = [0_u8; 16];
    getrandom::fill(&mut salt)
        .map_err(|_| AdminError::Internal("secure randomness unavailable".into()))?;
    let salt =
        SaltString::encode_b64(&salt).map_err(|error| AdminError::Internal(error.to_string()))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AdminError::Internal(error.to_string()))
}

pub(super) fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}
