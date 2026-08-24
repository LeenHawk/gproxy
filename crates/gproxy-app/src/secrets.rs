use aes_gcm::aead::{Aead, Key, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use gproxy_store::records::CredentialEnvelope;
use serde_json::Value;
use zeroize::Zeroize;

use crate::AppError;

const DEK_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const PAYLOAD_AAD: &[u8] = b"gproxy:v3:credential-envelope:v1:payload";
const WRAPPED_KEY_AAD: &[u8] = b"gproxy:v3:credential-envelope:v1:wrapped-dek";

#[derive(Clone)]
pub(crate) struct EnvelopeCipher {
    master: Aes256Gcm,
}

impl EnvelopeCipher {
    pub(crate) fn new(master_key: [u8; DEK_BYTES]) -> Self {
        Self {
            master: Aes256Gcm::new(&Key::<Aes256Gcm>::from(master_key)),
        }
    }

    pub(crate) fn seal(&self, value: &Value) -> Result<CredentialEnvelope, AppError> {
        let dek = SecretBytes(random_bytes::<DEK_BYTES>()?);
        let payload_nonce = random_bytes::<NONCE_BYTES>()?;
        let key_nonce = distinct_nonce(payload_nonce)?;
        let plaintext = SecretBytes(serde_json::to_vec(value).map_err(|_| seal_error())?);
        let payload_cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(dek.0));
        let ciphertext = payload_cipher
            .encrypt(
                &Nonce::from(payload_nonce),
                Payload {
                    msg: &plaintext.0,
                    aad: PAYLOAD_AAD,
                },
            )
            .map_err(|_| seal_error())?;
        let wrapped_key = self
            .master
            .encrypt(
                &Nonce::from(key_nonce),
                Payload {
                    msg: &dek.0,
                    aad: WRAPPED_KEY_AAD,
                },
            )
            .map_err(|_| seal_error())?;
        Ok(CredentialEnvelope {
            ciphertext,
            wrapped_key,
            payload_nonce: payload_nonce.to_vec(),
            key_nonce: key_nonce.to_vec(),
        })
    }

    pub(crate) fn open(&self, envelope: &CredentialEnvelope) -> Result<Value, AppError> {
        let payload_nonce = nonce(&envelope.payload_nonce)?;
        let key_nonce = nonce(&envelope.key_nonce)?;
        if payload_nonce == key_nonce {
            return Err(open_error());
        }
        let wrapped = SecretBytes(
            self.master
                .decrypt(
                    &Nonce::from(key_nonce),
                    Payload {
                        msg: &envelope.wrapped_key,
                        aad: WRAPPED_KEY_AAD,
                    },
                )
                .map_err(|_| open_error())?,
        );
        let dek: [u8; DEK_BYTES] = wrapped.0.as_slice().try_into().map_err(|_| open_error())?;
        let dek = SecretBytes(dek);
        let payload_cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(dek.0));
        let plaintext = SecretBytes(
            payload_cipher
                .decrypt(
                    &Nonce::from(payload_nonce),
                    Payload {
                        msg: &envelope.ciphertext,
                        aad: PAYLOAD_AAD,
                    },
                )
                .map_err(|_| open_error())?,
        );
        serde_json::from_slice(&plaintext.0).map_err(|_| open_error())
    }
}

struct SecretBytes<T: Zeroize>(T);

impl<T: Zeroize> Drop for SecretBytes<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

fn random_bytes<const N: usize>() -> Result<[u8; N], AppError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| AppError::Encryption("secure randomness unavailable".into()))?;
    Ok(bytes)
}

fn distinct_nonce(payload_nonce: [u8; NONCE_BYTES]) -> Result<[u8; NONCE_BYTES], AppError> {
    loop {
        let candidate = random_bytes()?;
        if candidate != payload_nonce {
            return Ok(candidate);
        }
    }
}

fn nonce(value: &[u8]) -> Result<[u8; NONCE_BYTES], AppError> {
    value.try_into().map_err(|_| open_error())
}

fn seal_error() -> AppError {
    AppError::Encryption("failed to seal credential".into())
}

fn open_error() -> AppError {
    AppError::Encryption("credential envelope is invalid".into())
}
