//! Sealing of credential secrets at rest. The Store column is one opaque byte
//! string; the first byte says which envelope follows. Core ships two codecs and
//! the host picks one explicitly: there is no silent plaintext fallback when a
//! key is missing and no silent acceptance of plaintext rows by a keyed codec.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use serde_json::Value;
use zeroize::Zeroize;

const DEK_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const ENVELOPE_PLAINTEXT: u8 = 0x00;
const ENVELOPE_AES_GCM: u8 = 0x01;
const PAYLOAD_AAD: &[u8] = b"gproxy:v4:credential:v1:payload:";
const WRAPPED_KEY_AAD: &[u8] = b"gproxy:v4:credential:v1:wrapped-dek:";

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum SecretError {
    #[error("secure randomness is unavailable")]
    Entropy,
    #[error("failed to seal secret")]
    Seal,
    /// Deliberately uninformative: tampering, a wrong key, a different
    /// credential ID and a foreign envelope format all read the same.
    #[error("sealed secret is invalid for this codec")]
    Open,
}

/// Seal and open one credential's secret. `credential_id` is bound into the
/// authenticated data, so a sealed blob cannot be copied onto another row.
pub trait SecretCodec: Send + Sync {
    fn seal(&self, credential_id: &str, secret: &Value) -> Result<Vec<u8>, SecretError>;
    fn open(&self, credential_id: &str, sealed: &[u8]) -> Result<Value, SecretError>;
}

/// AES-256-GCM envelope: a fresh per-credential data key encrypts the JSON,
/// the master key wraps that data key, and both use distinct random nonces.
/// Layout: `0x01 | payload_nonce[12] | key_nonce[12] | wrapped_len u16 BE |
/// wrapped_key | ciphertext`.
#[derive(Clone)]
pub struct AesGcmCodec {
    master: Aes256Gcm,
}

impl AesGcmCodec {
    pub fn new(master_key: [u8; DEK_BYTES]) -> Self {
        let mut key = master_key;
        let master = Aes256Gcm::new(&Key::<Aes256Gcm>::from(key));
        key.zeroize();
        Self { master }
    }
}

impl std::fmt::Debug for AesGcmCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AesGcmCodec")
    }
}

impl SecretCodec for AesGcmCodec {
    fn seal(&self, credential_id: &str, secret: &Value) -> Result<Vec<u8>, SecretError> {
        let dek = Zeroizing(random_bytes::<DEK_BYTES>()?);
        let payload_nonce = random_bytes::<NONCE_BYTES>()?;
        let key_nonce = distinct_nonce(payload_nonce)?;
        let plaintext = Zeroizing(serde_json::to_vec(secret).map_err(|_| SecretError::Seal)?);
        let ciphertext = Aes256Gcm::new(&Key::<Aes256Gcm>::from(dek.0))
            .encrypt(
                &Nonce::from(payload_nonce),
                Payload {
                    msg: &plaintext.0,
                    aad: &aad(PAYLOAD_AAD, credential_id),
                },
            )
            .map_err(|_| SecretError::Seal)?;
        let wrapped = self
            .master
            .encrypt(
                &Nonce::from(key_nonce),
                Payload {
                    msg: &dek.0,
                    aad: &aad(WRAPPED_KEY_AAD, credential_id),
                },
            )
            .map_err(|_| SecretError::Seal)?;
        let wrapped_len = u16::try_from(wrapped.len()).map_err(|_| SecretError::Seal)?;
        let mut out =
            Vec::with_capacity(1 + NONCE_BYTES * 2 + 2 + wrapped.len() + ciphertext.len());
        out.push(ENVELOPE_AES_GCM);
        out.extend_from_slice(&payload_nonce);
        out.extend_from_slice(&key_nonce);
        out.extend_from_slice(&wrapped_len.to_be_bytes());
        out.extend_from_slice(&wrapped);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn open(&self, credential_id: &str, sealed: &[u8]) -> Result<Value, SecretError> {
        let (&version, rest) = sealed.split_first().ok_or(SecretError::Open)?;
        if version != ENVELOPE_AES_GCM || rest.len() < NONCE_BYTES * 2 + 2 {
            return Err(SecretError::Open);
        }
        let (payload_nonce, rest) = rest.split_at(NONCE_BYTES);
        let (key_nonce, rest) = rest.split_at(NONCE_BYTES);
        if payload_nonce == key_nonce {
            return Err(SecretError::Open);
        }
        let payload_nonce: [u8; NONCE_BYTES] =
            payload_nonce.try_into().map_err(|_| SecretError::Open)?;
        let key_nonce: [u8; NONCE_BYTES] = key_nonce.try_into().map_err(|_| SecretError::Open)?;
        let (len, rest) = rest.split_at(2);
        let wrapped_len = usize::from(u16::from_be_bytes([len[0], len[1]]));
        if rest.len() < wrapped_len {
            return Err(SecretError::Open);
        }
        let (wrapped, ciphertext) = rest.split_at(wrapped_len);
        let dek = Zeroizing(
            self.master
                .decrypt(
                    &Nonce::from(key_nonce),
                    Payload {
                        msg: wrapped,
                        aad: &aad(WRAPPED_KEY_AAD, credential_id),
                    },
                )
                .map_err(|_| SecretError::Open)?,
        );
        let dek: [u8; DEK_BYTES] = dek.0.as_slice().try_into().map_err(|_| SecretError::Open)?;
        let dek = Zeroizing(dek);
        let plaintext = Zeroizing(
            Aes256Gcm::new(&Key::<Aes256Gcm>::from(dek.0))
                .decrypt(
                    &Nonce::from(payload_nonce),
                    Payload {
                        msg: ciphertext,
                        aad: &aad(PAYLOAD_AAD, credential_id),
                    },
                )
                .map_err(|_| SecretError::Open)?,
        );
        serde_json::from_slice(&plaintext.0).map_err(|_| SecretError::Open)
    }
}

/// Stores the JSON as-is behind a `0x00` marker. Only for deployments that
/// explicitly accept unencrypted secrets at rest; it refuses keyed envelopes so
/// a misconfigured host cannot quietly read rows sealed by another key.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlaintextCodec;

impl SecretCodec for PlaintextCodec {
    fn seal(&self, _credential_id: &str, secret: &Value) -> Result<Vec<u8>, SecretError> {
        let mut out = vec![ENVELOPE_PLAINTEXT];
        serde_json::to_writer(&mut out, secret).map_err(|_| SecretError::Seal)?;
        Ok(out)
    }

    fn open(&self, _credential_id: &str, sealed: &[u8]) -> Result<Value, SecretError> {
        match sealed.split_first() {
            Some((&ENVELOPE_PLAINTEXT, json)) => {
                serde_json::from_slice(json).map_err(|_| SecretError::Open)
            }
            _ => Err(SecretError::Open),
        }
    }
}

fn aad(domain: &[u8], credential_id: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(domain.len() + credential_id.len());
    out.extend_from_slice(domain);
    out.extend_from_slice(credential_id.as_bytes());
    out
}

fn random_bytes<const N: usize>() -> Result<[u8; N], SecretError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| SecretError::Entropy)?;
    Ok(bytes)
}

fn distinct_nonce(payload_nonce: [u8; NONCE_BYTES]) -> Result<[u8; NONCE_BYTES], SecretError> {
    loop {
        let candidate = random_bytes()?;
        if candidate != payload_nonce {
            return Ok(candidate);
        }
    }
}

struct Zeroizing<T: Zeroize>(T);

impl<T: Zeroize> Drop for Zeroizing<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
