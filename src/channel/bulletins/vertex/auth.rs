//! Vertex service-account auth: parse the SA `secret_json` and sign an RS256
//! JWT for the OAuth2 JWT-bearer grant.
//!
//! Signing uses `jsonwebtoken` v10 with the pure-Rust `rust_crypto` backend
//! (no `ring`), so the parse-and-sign path compiles and runs on **all targets
//! including wasm/edge**. A token is minted ~once per hour, so the pure-Rust
//! RSA cost is irrelevant.

/// Default region when neither provider settings nor the secret specify one.
/// Used by `prepare` on both targets.
pub(super) const DEFAULT_LOCATION: &str = "us-central1";

mod sign {
    use serde_json::Value;

    use crate::channel::ChannelError;

    const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
    const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

    /// The service-account fields needed to *sign* the assertion JWT.
    /// `project_id` is read directly off the secret in `prepare` (it addresses
    /// the URL, not the JWT), so it is intentionally absent. Unknown fields are
    /// ignored.
    pub(in crate::channel::bulletins::vertex) struct ServiceAccount {
        client_email: String,
        private_key: String,
        pub token_uri: String,
    }

    impl ServiceAccount {
        /// Parse from the decrypted secret. `private_key` is normalized: literal
        /// `\n` sequences (as stored in single-line JSON) become real newlines so
        /// the PEM parser accepts it.
        pub(in crate::channel::bulletins::vertex) fn parse(
            secret: &Value,
        ) -> Result<Self, ChannelError> {
            let client_email = string_field(secret, "client_email")?;
            let private_key = secret
                .get("private_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.replace("\\n", "\n"))
                .ok_or_else(|| ChannelError::InvalidCredential("missing private_key".into()))?;
            let token_uri = secret
                .get("token_uri")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(DEFAULT_TOKEN_URI)
                .to_string();
            Ok(Self {
                client_email,
                private_key,
                token_uri,
            })
        }
    }

    fn string_field(secret: &Value, key: &'static str) -> Result<String, ChannelError> {
        secret
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ChannelError::InvalidCredential(format!("missing {key}")))
    }

    /// Sign the SA assertion JWT (RS256).
    pub(in crate::channel::bulletins::vertex) fn sign_jwt(
        sa: &ServiceAccount,
    ) -> Result<String, ChannelError> {
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
        use serde::Serialize;

        #[derive(Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            scope: &'a str,
            aud: &'a str,
            iat: u64,
            exp: u64,
        }

        let now = crate::util::time::unix_now().max(0) as u64;
        let claims = Claims {
            iss: &sa.client_email,
            scope: SCOPE,
            aud: &sa.token_uri,
            iat: now,
            exp: now.saturating_add(3600),
        };
        let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
            .map_err(|e| ChannelError::InvalidCredential(format!("invalid private_key: {e}")))?;
        let mut header = Header::new(Algorithm::RS256);
        header.typ = Some("JWT".to_string());
        encode(&header, &claims, &key).map_err(|e| ChannelError::Build(format!("jwt sign: {e}")))
    }
}

#[allow(unused_imports)]
pub(super) use sign::{ServiceAccount, sign_jwt};
