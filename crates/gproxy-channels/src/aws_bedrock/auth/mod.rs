use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use hmac::{Hmac, Mac as _};
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

type HmacSha256 = Hmac<Sha256>;

mod canonical;
mod time;

pub(super) fn apply(
    request: &mut http::Request<Bytes>,
    secret: &Value,
    region: &str,
) -> Result<(), ChannelError> {
    if let Some(key) = field(secret, "api_key") {
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
        );
        return Ok(());
    }
    sigv4(request, secret, region, unix_now())
}

fn sigv4(
    request: &mut http::Request<Bytes>,
    secret: &Value,
    region: &str,
    now: u64,
) -> Result<(), ChannelError> {
    let access = required(secret, "access_key_id")?;
    let secret_key = required(secret, "secret_access_key")?;
    let (date, timestamp) = time::aws(now);
    insert(request.headers_mut(), "x-amz-date", &timestamp)?;
    let payload_hash = hex(Sha256::digest(request.body()));
    insert(request.headers_mut(), "x-amz-content-sha256", &payload_hash)?;
    if let Some(token) = field(secret, "session_token") {
        insert(request.headers_mut(), "x-amz-security-token", token)?;
    }
    let (canonical_headers, signed_headers) = canonical::headers(request)?;
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.method(),
        canonical::uri(request.uri().path())?,
        canonical::query(request.uri().query().unwrap_or_default())?,
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let scope = format!("{date}/{region}/bedrock/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex(Sha256::digest(canonical_request.as_bytes()))
    );
    let date_key = hmac(format!("AWS4{secret_key}").as_bytes(), date.as_bytes())?;
    let region_key = hmac(&date_key, region.as_bytes())?;
    let service_key = hmac(&region_key, b"bedrock")?;
    let signing_key = hmac(&service_key, b"aws4_request")?;
    let signature = hex(hmac(&signing_key, string_to_sign.as_bytes())?);
    insert(
        request.headers_mut(),
        "authorization",
        &format!(
            "AWS4-HMAC-SHA256 Credential={access}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
        ),
    )
}

fn hmac(key: &[u8], value: &[u8]) -> Result<Vec<u8>, ChannelError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|error| ChannelError::Prepare(format!("AWS signing key: {error}")))?;
    mac.update(value);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn insert(
    headers: &mut http::HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Prepare(format!("AWS header is invalid: {error}")))?,
    );
    Ok(())
}

fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    field(value, name).ok_or_else(|| ChannelError::Secret(format!("{name} missing")))
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    bytes
        .as_ref()
        .iter()
        .fold(String::new(), |mut output, byte| {
            write!(&mut output, "{byte:02x}").expect("writing to String succeeds");
            output
        })
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_secs()
}
