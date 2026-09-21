//! v3's sealed secrets, opened here and re-sealed by the sdk.
//!
//! # Why this file exists at all
//!
//! [`crate::transfer::import`] never opens anything: it hands the sdk a source
//! master key and `manage().transfer().import` opens each blob with
//! [`AesGcmCodec`] and re-seals it under this instance's codec. That works
//! between two v4 instances because both write the same envelope.
//!
//! v3 does not. Its layout is four separate columns — `ciphertext`,
//! `wrapped_key`, `payload_nonce`, `key_nonce` — and its authenticated data is
//! a fixed `gproxy:v3:…` domain string with no credential id in it, where v4's
//! is `gproxy:v4:credential:v1:payload:` **plus the row's id**. A v3 blob
//! cannot be handed to v4's codec under any key; it has to be opened by v3's
//! rules, and those rules only exist here.
//!
//! # What this file deliberately does not do
//!
//! It does not seal anything for the destination. The opened value is re-sealed
//! under a **random key that exists for the length of one import**, and that key
//! is handed to the sdk as the import's `sourceMasterKey`. So the document the
//! sdk receives is a perfectly ordinary v4 export whose secrets it can open,
//! and the only code that ever writes a secret into this database is the sdk's
//! own re-seal — the same path an ordinary `gproxy import` takes, with the same
//! per-credential id binding and the same refusal when it cannot open a row.
//!
//! The ephemeral key costs one extra AES pass per credential and buys the
//! property that matters: there is one sealing path in the product, not two.
//!
//! # The three source states, and what each one means
//!
//! | The v3 export says | What happens |
//! |---|---|
//! | secrets included, blobs sealed, `--source-master-key` given and right | opened, re-sealed, imported |
//! | secrets included, blobs sealed, key missing or wrong | **the whole import fails** |
//! | secrets included, blobs plaintext (the source had no master key) | read as JSON, re-sealed, imported |
//! | secrets omitted (`include_secrets` was false) | every credential is refused, before anything is written |
//!
//! Row two is the one that has to be loud. The sdk's own import skips a
//! credential it cannot open and counts it, which is right between two v4
//! instances: one missing credential there is one login. Here it is not a
//! missing row, it is *the wrong key* — every credential in the document is
//! sealed by the same master key, so if one fails they all fail, and skipping
//! them would report a successful migration of a deployment that cannot reach
//! a single upstream. It is refused as a whole instead.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use gproxy_core::{AesGcmCodec, SecretCodec};
use serde_json::Value;
use zeroize::Zeroize;

use super::document::Envelope;
use crate::{Error, Result};

/// v3's authenticated-data domains, copied from
/// `v3:crates/gproxy-app/src/secrets.rs`. They are fixed strings: v3 bound no
/// row id into the envelope, which is why a v3 blob opens the same wherever it
/// is read from and a v4 one does not.
const CREDENTIAL_PAYLOAD_AAD: &[u8] = b"gproxy:v3:credential-envelope:v1:payload";
const CREDENTIAL_WRAPPED_KEY_AAD: &[u8] = b"gproxy:v3:credential-envelope:v1:wrapped-dek";
const USER_KEY_PAYLOAD_AAD: &[u8] = b"gproxy:v3:user-key-envelope:v1:payload";
const USER_KEY_WRAPPED_KEY_AAD: &[u8] = b"gproxy:v3:user-key-envelope:v1:wrapped-dek";

const NONCE_BYTES: usize = 12;
const KEY_BYTES: usize = 32;

/// Which of v3's two envelope domains a blob belongs to. They are different
/// strings, so a credential blob does not open as a user key's and vice versa —
/// which is a feature: it catches a document whose halves were spliced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Credential,
    UserKey,
}

impl Domain {
    fn aads(self) -> (&'static [u8], &'static [u8]) {
        match self {
            Self::Credential => (CREDENTIAL_PAYLOAD_AAD, CREDENTIAL_WRAPPED_KEY_AAD),
            Self::UserKey => (USER_KEY_PAYLOAD_AAD, USER_KEY_WRAPPED_KEY_AAD),
        }
    }

    fn what(self) -> &'static str {
        match self {
            Self::Credential => "credential",
            Self::UserKey => "user key",
        }
    }
}

/// Opens v3 envelopes with the source's master key, and re-seals what it opened
/// under one throwaway key that only this import knows.
pub struct Bridge {
    /// None when the source instance had no master key: its blobs are JSON.
    source: Option<Aes256Gcm>,
    /// The key the sdk will be given, and the codec that seals for it. Both
    /// halves of one value, kept together so they cannot disagree.
    ephemeral: EphemeralKey,
    codec: AesGcmCodec,
}

impl Bridge {
    /// A bridge over a source that sealed with `master_key`, or over one that
    /// did not seal at all when it is `None`.
    pub fn new(master_key: Option<[u8; KEY_BYTES]>) -> Result<Self> {
        let ephemeral = EphemeralKey::random()?;
        Ok(Self {
            source: master_key.map(|key| Aes256Gcm::new(&Key::<Aes256Gcm>::from(key))),
            codec: AesGcmCodec::new(ephemeral.0),
            ephemeral,
        })
    }

    /// The key to hand the sdk as `sourceMasterKey`, in the standard base64 it
    /// reads.
    pub fn sdk_source_key(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(self.ephemeral.0)
    }

    /// Whether this bridge holds a key at all. A document whose blobs are
    /// sealed and a bridge that is not keyed is a mistake worth naming before
    /// the first decryption fails.
    pub fn is_keyed(&self) -> bool {
        self.source.is_some()
    }

    /// Open one v3 envelope. `what` names the row in the error, because the
    /// error an operator sees for a wrong key has to say which key is wrong.
    pub fn open(&self, domain: Domain, what: &str, envelope: &Envelope) -> Result<Value> {
        let (payload_aad, wrapped_key_aad) = domain.aads();
        if envelope.is_plaintext() {
            // v3 without a master key: `ciphertext` is the JSON itself. A
            // keyed source never produces this shape, so it is not ambiguous.
            return serde_json::from_slice(&envelope.ciphertext).map_err(|error| {
                Error::other(format!(
                    "{} `{what}` carries an unsealed secret that is not JSON: {error}",
                    domain.what()
                ))
            });
        }
        let Some(master) = &self.source else {
            return Err(Error::other(format!(
                "{} `{what}` is sealed, but no v3 master key was supplied; pass \
                 --source-master-key with the value of GPROXY_MASTER_KEY on the v3 instance",
                domain.what()
            )));
        };
        let payload_nonce = nonce(domain, what, &envelope.payload_nonce)?;
        let key_nonce = nonce(domain, what, &envelope.key_nonce)?;
        if payload_nonce == key_nonce {
            return Err(self.refuse(domain, what));
        }
        let mut wrapped = master
            .decrypt(
                &Nonce::from(key_nonce),
                Payload {
                    msg: &envelope.wrapped_key,
                    aad: wrapped_key_aad,
                },
            )
            .map_err(|_| self.refuse(domain, what))?;
        let dek: [u8; KEY_BYTES] = wrapped.as_slice().try_into().map_err(|_| {
            wrapped.zeroize();
            self.refuse(domain, what)
        })?;
        wrapped.zeroize();
        let mut plaintext = Aes256Gcm::new(&Key::<Aes256Gcm>::from(dek))
            .decrypt(
                &Nonce::from(payload_nonce),
                Payload {
                    msg: &envelope.ciphertext,
                    aad: payload_aad,
                },
            )
            .map_err(|_| self.refuse(domain, what))?;
        let value = serde_json::from_slice(&plaintext);
        plaintext.zeroize();
        value.map_err(|_| self.refuse(domain, what))
    }

    /// Seal an opened value for the sdk's import, bound to the v4 id the row
    /// will have — exactly as the sdk will re-bind it when it re-seals.
    pub fn seal_for_sdk(&self, id: &str, value: &Value) -> Result<Vec<u8>> {
        self.codec
            .seal(id, value)
            .map_err(|error| Error::other(format!("re-sealing `{id}` for the import: {error}")))
    }

    /// One message for every way a blob can fail to open, because they are one
    /// thing from where the operator stands: the key is wrong.
    fn refuse(&self, domain: Domain, what: &str) -> Error {
        Error::other(format!(
            "{} `{what}` did not open with the supplied v3 master key. Every secret in a v3 \
             export is sealed by the same key, so this is refused as a whole rather than \
             imported without it: check --source-master-key against GPROXY_MASTER_KEY on the \
             v3 instance.",
            domain.what()
        ))
    }
}

fn nonce(domain: Domain, what: &str, value: &[u8]) -> Result<[u8; NONCE_BYTES]> {
    value.try_into().map_err(|_| {
        Error::other(format!(
            "{} `{what}` has a {}-byte nonce where {NONCE_BYTES} were expected; the export is \
             damaged",
            domain.what(),
            value.len()
        ))
    })
}

/// 32 random bytes that exist for one import and are wiped when it ends.
struct EphemeralKey([u8; KEY_BYTES]);

impl EphemeralKey {
    fn random() -> Result<Self> {
        let mut bytes = [0_u8; KEY_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| {
            Error::other("secure randomness is unavailable; cannot re-seal the v3 secrets")
        })?;
        Ok(Self(bytes))
    }
}

impl Drop for EphemeralKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// A master key as an operator holds it — 64 hex characters or base64 — as the
/// 32 bytes v3 sealed with.
pub fn master_key(value: &str) -> Result<[u8; KEY_BYTES]> {
    use gproxy_app::config::MasterKey;

    let trimmed = value.trim();
    let candidate = if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        MasterKey::Hex(trimmed.to_owned())
    } else {
        MasterKey::Base64(trimmed.to_owned())
    };
    candidate
        .resolve()
        .map_err(|error| {
            Error::config(
                "--source-master-key / GPROXY_IMPORT_SOURCE_MASTER_KEY",
                error,
            )
        })?
        .ok_or_else(|| {
            Error::config(
                "--source-master-key / GPROXY_IMPORT_SOURCE_MASTER_KEY",
                "a source master key must not be empty",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// v3's sealing, reimplemented from its own source, so the tests below
    /// assert against bytes v3 would actually have written rather than against
    /// this module's inverse of itself.
    fn seal_like_v3(master: Option<[u8; 32]>, domain: Domain, value: &Value) -> Envelope {
        let (payload_aad, wrapped_key_aad) = domain.aads();
        let Some(master) = master else {
            return Envelope {
                ciphertext: serde_json::to_vec(value).unwrap(),
                ..Envelope::default()
            };
        };
        let master = Aes256Gcm::new(&Key::<Aes256Gcm>::from(master));
        let dek = [9_u8; 32];
        let payload_nonce = [1_u8; NONCE_BYTES];
        let key_nonce = [2_u8; NONCE_BYTES];
        let ciphertext = Aes256Gcm::new(&Key::<Aes256Gcm>::from(dek))
            .encrypt(
                &Nonce::from(payload_nonce),
                Payload {
                    msg: &serde_json::to_vec(value).unwrap(),
                    aad: payload_aad,
                },
            )
            .unwrap();
        let wrapped_key = master
            .encrypt(
                &Nonce::from(key_nonce),
                Payload {
                    msg: &dek,
                    aad: wrapped_key_aad,
                },
            )
            .unwrap();
        Envelope {
            ciphertext,
            wrapped_key,
            payload_nonce: payload_nonce.to_vec(),
            key_nonce: key_nonce.to_vec(),
        }
    }

    #[test]
    fn a_sealed_v3_credential_opens_with_the_source_key() {
        let secret = json!({"api_key": "the-upstream-token"});
        let envelope = seal_like_v3(Some([7; 32]), Domain::Credential, &secret);
        let bridge = Bridge::new(Some([7; 32])).unwrap();
        assert_eq!(
            bridge.open(Domain::Credential, "c1", &envelope).unwrap(),
            secret
        );
    }

    #[test]
    fn a_user_keys_envelope_uses_its_own_domain() {
        let secret = json!("sk-the-key-text");
        let envelope = seal_like_v3(Some([7; 32]), Domain::UserKey, &secret);
        let bridge = Bridge::new(Some([7; 32])).unwrap();
        assert_eq!(
            bridge.open(Domain::UserKey, "k1", &envelope).unwrap(),
            secret
        );
        // And it is not a credential's: the domains are distinct on purpose.
        assert!(bridge.open(Domain::Credential, "k1", &envelope).is_err());
    }

    #[test]
    fn a_plaintext_v3_export_needs_no_key_at_all() {
        let secret = json!({"api_key": "unsealed"});
        let envelope = seal_like_v3(None, Domain::Credential, &secret);
        let bridge = Bridge::new(None).unwrap();
        assert!(!bridge.is_keyed());
        assert_eq!(
            bridge.open(Domain::Credential, "c1", &envelope).unwrap(),
            secret
        );
    }

    #[test]
    fn the_wrong_key_is_an_error_that_names_the_flag() {
        let envelope = seal_like_v3(Some([7; 32]), Domain::Credential, &json!({"a": 1}));
        let error = Bridge::new(Some([8; 32]))
            .unwrap()
            .open(Domain::Credential, "c1", &envelope)
            .unwrap_err()
            .to_string();
        assert!(error.contains("--source-master-key"), "{error}");
        assert!(error.contains("refused as a whole"), "{error}");
    }

    #[test]
    fn a_sealed_blob_without_a_key_says_which_flag_is_missing() {
        let envelope = seal_like_v3(Some([7; 32]), Domain::Credential, &json!({"a": 1}));
        let error = Bridge::new(None)
            .unwrap()
            .open(Domain::Credential, "c1", &envelope)
            .unwrap_err()
            .to_string();
        assert!(error.contains("GPROXY_MASTER_KEY"), "{error}");
    }

    /// The handshake with the sdk: what `seal_for_sdk` writes is what an
    /// `AesGcmCodec` built from `sdk_source_key` opens, under the same id.
    #[test]
    fn what_the_bridge_seals_is_what_the_sdk_will_open() {
        use base64::Engine;
        let bridge = Bridge::new(None).unwrap();
        let secret = json!({"api_key": "x"});
        let sealed = bridge.seal_for_sdk("v3-credentials-1", &secret).unwrap();

        let key: [u8; 32] = base64::engine::general_purpose::STANDARD
            .decode(bridge.sdk_source_key())
            .unwrap()
            .try_into()
            .unwrap();
        let codec = AesGcmCodec::new(key);
        assert_eq!(codec.open("v3-credentials-1", &sealed).unwrap(), secret);
        // Bound to the row: the same blob under another id does not open,
        // which is the property the sdk's re-seal relies on.
        assert!(codec.open("v3-credentials-2", &sealed).is_err());
    }

    #[test]
    fn two_bridges_do_not_share_a_key() {
        assert_ne!(
            Bridge::new(None).unwrap().sdk_source_key(),
            Bridge::new(None).unwrap().sdk_source_key()
        );
    }

    #[test]
    fn a_master_key_is_read_as_hex_or_base64() {
        use base64::Engine;
        let raw = [0x5a_u8; 32];
        let hex: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(master_key(&hex).unwrap(), raw);
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        assert_eq!(master_key(&b64).unwrap(), raw);
        assert!(master_key("   ").is_err());
        assert!(master_key("too-short").is_err());
    }

    #[test]
    fn a_damaged_nonce_says_the_export_is_damaged() {
        let mut envelope = seal_like_v3(Some([7; 32]), Domain::Credential, &json!({"a": 1}));
        envelope.payload_nonce.truncate(4);
        let error = Bridge::new(Some([7; 32]))
            .unwrap()
            .open(Domain::Credential, "c1", &envelope)
            .unwrap_err()
            .to_string();
        assert!(error.contains("damaged"), "{error}");
    }
}
