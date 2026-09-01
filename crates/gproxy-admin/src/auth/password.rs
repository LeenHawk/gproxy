use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

use crate::AdminError;

pub(crate) fn validate(password: &str) -> Result<(), AdminError> {
    if password.trim().is_empty() {
        Err(AdminError::BadRequest("password must not be blank".into()))
    } else {
        Ok(())
    }
}

pub(crate) fn hash(password: &str) -> Result<String, AdminError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash: PasswordHash| hash.to_string())
        .map_err(|error| AdminError::Internal(error.to_string()))
}

pub(crate) fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}
