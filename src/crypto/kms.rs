//! KMS abstraction + `LocalKms` (KEK = decoded `GPROXY_MASTER_KEY`).

use anyhow::{Context, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use crate::util::rand;

/// Wraps/unwraps per-secret data-encryption keys (DEKs) under a
/// key-encryption key (KEK). Object-safe: ciphers hold `Box<dyn Kms>`.
pub trait Kms: Send + Sync {
    /// Stable KEK identifier, stored in envelopes for wrong-key detection
    /// and as a future rotation marker.
    fn kek_id(&self) -> &str;
    /// Wrap a 32-byte DEK into an opaque blob.
    fn wrap(&self, dek: &[u8; 32]) -> Vec<u8>;
    /// Unwrap a blob produced by [`Kms::wrap`].
    fn unwrap_dek(&self, wrapped: &[u8]) -> anyhow::Result<[u8; 32]>;
}

/// Local KMS: the KEK is `GPROXY_MASTER_KEY` decoded from base64 (must be
/// exactly 32 bytes). Wrapped-DEK blob layout: `nonce(24) || ciphertext`
/// (fresh random nonce per wrap, prepended).
pub struct LocalKms {
    kek: XChaCha20Poly1305,
    kek_id: String,
}

impl LocalKms {
    /// Build from the base64 master key. Malformed base64 or a length other
    /// than 32 bytes is a hard error (boot should fail, not warn).
    pub fn from_master_key_b64(master_b64: &str) -> anyhow::Result<Self> {
        let raw = B64
            .decode(master_b64.trim())
            .context("GPROXY_MASTER_KEY is not valid base64")?;
        let kek: [u8; 32] = raw.as_slice().try_into().map_err(|_| {
            anyhow::anyhow!(
                "GPROXY_MASTER_KEY must decode to exactly 32 bytes, got {}",
                raw.len()
            )
        })?;
        let kek_id = format!("local-{}", &blake3::hash(&kek).to_hex().as_str()[..8]);
        Ok(Self {
            kek: XChaCha20Poly1305::new(&Key::from(kek)),
            kek_id,
        })
    }
}

impl Kms for LocalKms {
    fn kek_id(&self) -> &str {
        &self.kek_id
    }

    fn wrap(&self, dek: &[u8; 32]) -> Vec<u8> {
        let mut nonce = [0u8; 24];
        rand::fill(&mut nonce);
        let ct = self
            .kek
            .encrypt(&XNonce::from(nonce), dek.as_slice())
            .expect("XChaCha20-Poly1305 encryption of an in-memory buffer cannot fail");
        let mut out = Vec::with_capacity(nonce.len() + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        out
    }

    fn unwrap_dek(&self, wrapped: &[u8]) -> anyhow::Result<[u8; 32]> {
        if wrapped.len() < 24 {
            bail!("wrapped DEK blob too short ({} bytes)", wrapped.len());
        }
        let (nonce, ct) = wrapped.split_at(24);
        let nonce: [u8; 24] = nonce.try_into().expect("split_at(24) yields 24 bytes");
        let plain = self
            .kek
            .decrypt(&XNonce::from(nonce), ct)
            .map_err(|_| anyhow::anyhow!("DEK unwrap failed under KEK '{}'", self.kek_id))?;
        plain
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("unwrapped DEK has wrong length"))
    }
}
