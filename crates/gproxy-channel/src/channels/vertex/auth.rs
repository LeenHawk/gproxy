//! Service-account authentication.
//!
//! A Vertex credential is a Google service-account key: a long-lived RSA
//! private key that is *not* itself a bearer token. What goes on the wire is a
//! short-lived OAuth access token minted from it, so the mint is modelled as
//! [`CredentialRefresh`]: the host runs it under a lease, persists the result
//! with a version CAS, and re-runs it before `expires_at_ms`. `prepare` only
//! ever reads the token the last refresh stored — minting inside `prepare`
//! would mean a signature and a round trip on every single request, and
//! `prepare` is synchronous and may not do I/O at all.
//!
//! The exchange is the standard JWT bearer grant (RFC 7523) as Google
//! documents it for server-to-server use: an RS256 assertion signed with the
//! key, posted as `urn:ietf:params:oauth:grant-type:jwt-bearer` to the key's
//! own `token_uri`.

use super::Vertex;
use crate::channel::{ChannelError, CredentialRefresh, CredentialUpdate, RefreshContext};
use base64::Engine as _;
use gproxy_protocol::capability::CapabilityFuture;
use gproxy_protocol::{HttpBody, WireResponse, connection::Bytes};
use http::{HeaderValue, Method, StatusCode, header};
use serde::Serialize;
use serde_json::Value;

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
/// Google issues one-hour tokens and accepts assertions valid for at most an
/// hour; asking for exactly that is what `gcloud` does.
const ASSERTION_TTL_SECONDS: u64 = 3_600;
/// A token response without `expires_in` is treated as the documented hour.
const DEFAULT_EXPIRES_IN_SECONDS: i64 = 3_600;
const MAX_TOKEN_BODY: usize = 64 * 1024;

/// The access token `prepare` puts in the `Authorization` header. Absent means
/// no refresh has run yet, which is a credential the host must refresh before
/// it can serve a request, not a request the channel can rescue.
pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "access_token").ok_or(ChannelError::InvalidCredential)
}

impl CredentialRefresh for Vertex {
    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>> {
        Box::pin(async move {
            let account = ServiceAccount::parse(context.credential.secret)?;
            let assertion = account.assertion()?;
            let body = form_encode(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
            ]);
            let mut request = http::Request::builder()
                .method(Method::POST)
                .uri(&account.token_uri)
                .body(HttpBody::Bytes(Bytes::from(body.into_bytes())))
                .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
            request.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
            request
                .headers_mut()
                .insert(header::ACCEPT, HeaderValue::from_static("application/json"));
            let WireResponse {
                status,
                body: response,
                ..
            } = context.client.send(request).await?;
            let bytes = read_body(response).await?;
            if !status.is_success() {
                return Err(reject(status, &bytes));
            }
            let token: Value = serde_json::from_slice(&bytes)
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            rotate(context.credential.secret, &token)
        })
    }
}

/// A refusal the upstream will repeat forever versus one worth retrying.
///
/// Google answers a revoked, deleted or disabled service-account key with
/// `400 invalid_grant`, and a key whose project lost the API with `401`/`403`.
/// Those are definitive: the host marks the credential dead and a person has
/// to paste a new key. A `429` or a `5xx` from the token endpoint is the token
/// endpoint having a bad minute and must stay retryable.
fn reject(status: StatusCode, body: &[u8]) -> ChannelError {
    let code = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| match value.get("error") {
            Some(Value::String(code)) => Some(code.clone()),
            Some(Value::Object(error)) => error
                .get("status")
                .or_else(|| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            _ => None,
        });
    let definitive = matches!(
        status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::BAD_REQUEST
    ) && code.as_deref() != Some("internal_failure");
    if definitive {
        ChannelError::RefreshRejected(code.unwrap_or_else(|| format!("http {}", status.as_u16())))
    } else {
        ChannelError::UpstreamResponse {
            status,
            body: Bytes::copy_from_slice(body),
        }
    }
}

/// A full replacement secret: the key material is carried over untouched and
/// only the minted token and its expiry change.
fn rotate(secret: &Value, token: &Value) -> Result<CredentialUpdate, ChannelError> {
    let access = field(token, "access_token").ok_or_else(|| {
        ChannelError::InvalidResponse("token response has no access_token".into())
    })?;
    let expires_in = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_EXPIRES_IN_SECONDS)
        .max(0);
    let expires_at_ms = unix_seconds()
        .saturating_add(expires_in)
        .saturating_mul(1_000);
    let mut secret = secret.clone();
    let object = secret
        .as_object_mut()
        .ok_or_else(|| ChannelError::InvalidCredential)?;
    object.insert("access_token".into(), Value::String(access.to_owned()));
    object.insert("expires_at_ms".into(), Value::from(expires_at_ms));
    Ok(CredentialUpdate {
        secret,
        expires_at_ms: Some(expires_at_ms),
    })
}

struct ServiceAccount {
    client_email: String,
    private_key: String,
    token_uri: String,
}

impl ServiceAccount {
    fn parse(secret: &Value) -> Result<Self, ChannelError> {
        let required = |name: &str| {
            field(secret, name).ok_or_else(|| {
                // No amount of retrying invents a field the key never had, so
                // this is as definitive as an upstream refusal.
                ChannelError::RefreshRejected(format!("service-account key has no `{name}`"))
            })
        };
        Ok(Self {
            client_email: required("client_email")?.to_owned(),
            // JSON keys pasted through a shell often arrive with literal `\n`.
            private_key: required("private_key")?.replace("\\n", "\n"),
            token_uri: field(secret, "token_uri")
                .unwrap_or(DEFAULT_TOKEN_URI)
                .to_owned(),
        })
    }

    fn assertion(&self) -> Result<String, ChannelError> {
        let issued = unix_seconds().max(0) as u64;
        let claims = Claims {
            iss: &self.client_email,
            scope: SCOPE,
            aud: &self.token_uri,
            iat: issued,
            exp: issued.saturating_add(ASSERTION_TTL_SECONDS),
        };
        let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header = engine.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = engine.encode(
            serde_json::to_vec(&claims)
                .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?,
        );
        let signing_input = format!("{header}.{claims}");
        let signature = sign(&self.private_key, signing_input.as_bytes())?;
        Ok(format!("{signing_input}.{}", engine.encode(signature)))
    }
}

/// RS256 over the JWT signing input. A key that will not parse can never be
/// made to work, so it is reported the same way a revoked key is.
fn sign(pem: &str, message: &[u8]) -> Result<Vec<u8>, ChannelError> {
    use rsa::pkcs1::DecodeRsaPrivateKey as _;
    use rsa::pkcs8::DecodePrivateKey as _;
    use rsa::signature::{SignatureEncoding as _, Signer as _};

    let key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| rsa::RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|error| {
            ChannelError::RefreshRejected(format!("service-account private_key: {error}"))
        })?;
    let signing_key = rsa::pkcs1v15::SigningKey::<rsa::sha2::Sha256>::new(key);
    let signature = signing_key
        .try_sign(message)
        .map_err(|error| ChannelError::RefreshRejected(format!("JWT signature: {error}")))?;
    Ok(signature.to_vec())
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    use futures_util::StreamExt as _;
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > MAX_TOKEN_BODY {
                    return Err(ChannelError::InvalidResponse(
                        "token response exceeds the read limit".into(),
                    ));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn unix_seconds() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}
