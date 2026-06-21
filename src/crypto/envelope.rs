//! Envelope format and seal/open primitives.
//!
//! Stored shape: `{"kek_id": "...", "wrapped_dek": "<b64>", "nonce": "<b64>",
//! "ciphertext": "<b64>"}`. The AEAD message is the serialized plaintext JSON
//! bytes; the AAD is the `kek_id` bytes.

use anyhow::{Context, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::kms::Kms;
use crate::util::rand;

/// Serialized envelope. Binary fields are standard base64.
#[derive(Serialize, Deserialize)]
pub struct Envelope {
    pub kek_id: String,
    pub wrapped_dek: String,
    pub nonce: String,
    pub ciphertext: String,
}

/// True iff `v` is an object with *exactly* the four envelope keys, all with
/// string values. Extra keys → not an envelope. The exact-4 check reduces
/// false positives; a user secret that happens to have precisely this shape
/// would still be misdetected — accepted residual risk (documented).
pub fn is_envelope(v: &Value) -> bool {
    let Some(obj) = v.as_object() else {
        return false;
    };
    obj.len() == 4
        && ["kek_id", "wrapped_dek", "nonce", "ciphertext"]
            .iter()
            .all(|k| obj.get(*k).is_some_and(Value::is_string))
}

/// Seal `plain` under a fresh random 32-byte DEK + 24-byte nonce; the DEK is
/// wrapped by `kms`.
pub fn seal_with(kms: &dyn Kms, plain: &Value) -> anyhow::Result<Value> {
    let mut dek = [0u8; 32];
    let mut nonce = [0u8; 24];
    rand::fill(&mut dek);
    rand::fill(&mut nonce);

    let msg = serde_json::to_vec(plain).context("serializing secret for sealing")?;
    let kek_id = kms.kek_id();
    let ct = XChaCha20Poly1305::new(&Key::from(dek))
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &msg,
                aad: kek_id.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("envelope seal failed"))?;

    let env = Envelope {
        kek_id: kek_id.to_string(),
        wrapped_dek: B64.encode(kms.wrap(&dek)),
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ct),
    };
    Ok(serde_json::to_value(env)?)
}

/// Open an envelope value (caller checks [`is_envelope`] first for legacy
/// passthrough). Wrong KEK is reported with both kek_ids.
pub fn open_with(kms: &dyn Kms, stored: &Value) -> anyhow::Result<Value> {
    let env: Envelope = serde_json::from_value(stored.clone()).context("malformed envelope")?;
    if env.kek_id != kms.kek_id() {
        bail!(
            "secret sealed under KEK '{}' but configured KEK is '{}'",
            env.kek_id,
            kms.kek_id()
        );
    }
    let wrapped = B64
        .decode(&env.wrapped_dek)
        .context("envelope wrapped_dek is not valid base64")?;
    let dek = kms.unwrap_dek(&wrapped)?;
    let nonce = B64
        .decode(&env.nonce)
        .context("envelope nonce is not valid base64")?;
    let nonce: [u8; 24] = nonce
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("envelope nonce must be 24 bytes, got {}", nonce.len()))?;
    let ct = B64
        .decode(&env.ciphertext)
        .context("envelope ciphertext is not valid base64")?;
    let plain = XChaCha20Poly1305::new(&Key::from(dek))
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &ct,
                aad: env.kek_id.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("envelope open failed: ciphertext tampered or corrupt"))?;
    serde_json::from_slice(&plain).context("decrypted secret is not valid JSON")
}
