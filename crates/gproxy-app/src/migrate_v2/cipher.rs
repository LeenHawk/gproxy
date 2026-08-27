use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde_json::Value;
use zeroize::Zeroize;

pub(super) struct V2Cipher {
    key: Option<SecretBytes<[u8; 32]>>,
}

impl V2Cipher {
    pub(super) fn new(encoded: Option<&str>) -> Result<Self, crate::AppError> {
        let key = encoded
            .map(|encoded| {
                let mut decoded = base64::engine::general_purpose::STANDARD
                    .decode(encoded.trim())
                    .map_err(|_| invalid_key())?;
                let key = decoded.as_slice().try_into().map_err(|_| invalid_key())?;
                decoded.zeroize();
                Ok::<SecretBytes<[u8; 32]>, crate::AppError>(SecretBytes(key))
            })
            .transpose()?;
        Ok(Self { key })
    }

    pub(super) fn open(&self, stored: &Value) -> Result<Value, ()> {
        if !is_envelope(stored) {
            return Ok(stored.clone());
        }
        let key = self.key.as_ref().ok_or(())?;
        let object = stored.as_object().ok_or(())?;
        let kek_id = text(object, "kek_id")?;
        let wrapped = decode(text(object, "wrapped_dek")?)?;
        if wrapped.len() < 24 {
            return Err(());
        }
        let (key_nonce, wrapped_key) = wrapped.split_at(24);
        let key_nonce: [u8; 24] = key_nonce.try_into().map_err(|_| ())?;
        let cipher = XChaCha20Poly1305::new(&Key::from(key.0));
        let mut dek = cipher
            .decrypt(&XNonce::from(key_nonce), wrapped_key)
            .map_err(|_| ())?;
        let result = open_payload(object, kek_id, &dek);
        dek.zeroize();
        result
    }

    pub(super) fn user_key(&self, stored: &str) -> Result<String, ()> {
        match serde_json::from_str::<Value>(stored) {
            Ok(value) if is_envelope(&value) => {
                self.open(&value)?.as_str().map(str::to_owned).ok_or(())
            }
            _ => Ok(stored.to_owned()),
        }
    }
}

fn open_payload(
    object: &serde_json::Map<String, Value>,
    kek_id: &str,
    dek: &[u8],
) -> Result<Value, ()> {
    let dek: [u8; 32] = dek.try_into().map_err(|_| ())?;
    let nonce = decode(text(object, "nonce")?)?;
    let nonce: [u8; 24] = nonce.as_slice().try_into().map_err(|_| ())?;
    let ciphertext = decode(text(object, "ciphertext")?)?;
    let mut plaintext = XChaCha20Poly1305::new(&Key::from(dek))
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &ciphertext,
                aad: kek_id.as_bytes(),
            },
        )
        .map_err(|_| ())?;
    let value = serde_json::from_slice(&plaintext).map_err(|_| ());
    plaintext.zeroize();
    value
}

fn is_envelope(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 4
            && ["kek_id", "wrapped_dek", "nonce", "ciphertext"]
                .iter()
                .all(|key| object.get(*key).is_some_and(Value::is_string))
    })
}

fn text<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Result<&'a str, ()> {
    object.get(key).and_then(Value::as_str).ok_or(())
}

fn decode(value: &str) -> Result<Vec<u8>, ()> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| ())
}

struct SecretBytes<T: Zeroize>(T);

impl<T: Zeroize> Drop for SecretBytes<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

fn invalid_key() -> crate::AppError {
    crate::AppError::Migration(
        "the v2 master key must be standard base64 for exactly 32 bytes".into(),
    )
}
