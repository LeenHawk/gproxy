use base64::Engine as _;
use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{AUTHORIZATION, HeaderValue};
use serde::Serialize;
use serde_json::Value;

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const EXPIRY_SKEW_SECONDS: i64 = 60;

pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "access_token")
        .ok_or_else(|| ChannelError::Secret("access_token missing after refresh".into()))
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    if field(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    match secret.get("expires_at_ms").and_then(Value::as_i64) {
        Some(expires) if expires != 0 => Some(expires / 1_000 - EXPIRY_SKEW_SECONDS),
        _ if field(secret, "private_key").is_some() => Some(i64::MIN),
        _ => None,
    }
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = build_refresh(secret);
    let request = match request {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            let snippet: String = String::from_utf8_lossy(response.body())
                .chars()
                .take(256)
                .collect();
            return Err(ChannelError::Refresh(format!(
                "Google token endpoint {}: {snippet}",
                response.status()
            )));
        }
        let token: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("invalid token response: {error}")))?;
        rotate(secret, &token)
    })
}

pub(super) fn apply(headers: &mut http::HeaderMap, token: &str) -> Result<(), ChannelError> {
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|error| ChannelError::Secret(format!("access_token is invalid: {error}")))?,
    );
    Ok(())
}

fn build_refresh(secret: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    let account = ServiceAccount::parse(secret)?;
    let assertion = account.assertion()?;
    let body = crate::shared::http::form(&[
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", &assertion),
    ]);
    http::Request::post(&account.token_uri)
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(http::header::ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Refresh(error.to_string()))
}

struct ServiceAccount {
    client_email: String,
    private_key: String,
    token_uri: String,
}

impl ServiceAccount {
    fn parse(secret: &Value) -> Result<Self, ChannelError> {
        Ok(Self {
            client_email: required(secret, "client_email")?.to_owned(),
            private_key: required(secret, "private_key")?.replace("\\n", "\n"),
            token_uri: field(secret, "token_uri")
                .unwrap_or(DEFAULT_TOKEN_URI)
                .to_owned(),
        })
    }

    fn assertion(&self) -> Result<String, ChannelError> {
        let now = unix_now().max(0) as u64;
        let claims = Claims {
            iss: &self.client_email,
            scope: SCOPE,
            aud: &self.token_uri,
            iat: now,
            exp: now.saturating_add(3_600),
        };
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = serde_json::to_vec(&claims)
            .map_err(|error| ChannelError::Refresh(format!("JWT claims failed: {error}")))?;
        let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims);
        let signing_input = format!("{header}.{claims}");
        let key = rsa_key(&self.private_key)?;
        let signing_key = rsa::pkcs1v15::SigningKey::<rsa::sha2::Sha256>::new(key);
        let signature = rsa::signature::Signer::sign(&signing_key, signing_input.as_bytes());
        let signature = rsa::signature::SignatureEncoding::to_vec(&signature);
        Ok(format!(
            "{signing_input}.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature)
        ))
    }
}

fn rsa_key(pem: &str) -> Result<rsa::RsaPrivateKey, ChannelError> {
    use rsa::pkcs1::DecodeRsaPrivateKey as _;
    use rsa::pkcs8::DecodePrivateKey as _;

    rsa::RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| rsa::RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|error| ChannelError::Secret(format!("private_key is invalid: {error}")))
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = required(token, "access_token")?;
    let expires = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3_600)
        .max(0);
    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("secret must be a JSON object".into()))?;
    object.insert("access_token".into(), Value::String(access.into()));
    object.insert(
        "expires_at_ms".into(),
        Value::from(unix_now().saturating_add(expires).saturating_mul(1_000)),
    );
    Ok(output)
}

fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    field(value, name).ok_or_else(|| ChannelError::Secret(format!("{name} missing")))
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_secs()
        .try_into()
        .expect("Unix seconds fit in i64")
}
