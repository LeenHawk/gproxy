//! https://docs.volcengine.com/docs/ark/base-url-and-authentication
//! https://www.volcengine.com/docs/6369/67269
//! Reference algorithm: volcengine/volcengine-python-sdk,
//! volcenginesdkcore/signv4.py (HMAC-SHA256, no AWS4 secret prefix).

use super::Config;
use crate::channel::{ChannelError, CredentialContext};
use crate::channels::shared::compatible::{
    ability::{require_success, send},
    http::invalid_response,
};
use hmac::{Hmac, KeyInit as _, Mac as _};
use http::{HeaderMap, HeaderValue, Method};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

const QUERY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub(super) async fn call(
    ctx: CredentialContext<'_>,
    action: &str,
    body: Value,
    endpoint: Option<&str>,
) -> Result<Value, ChannelError> {
    let config = Config::read(ctx.provider)?;
    let url = endpoint.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "{}/?Action={action}&Version=2024-01-01",
            config.control_base_url.trim_end_matches('/')
        )
    });
    let body = body.to_string();
    let seconds = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?
        .as_secs();
    let date = time::OffsetDateTime::from_unix_timestamp(seconds as i64)
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    let timestamp = format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        date.year(),
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second()
    );
    let headers = sign(&url, body.as_bytes(), ctx.credential.secret, &timestamp)?;
    let (status, _, bytes) = send(
        ctx.client,
        Method::POST,
        &url,
        headers,
        Some(body.into_bytes()),
    )
    .await?;
    let bytes = require_success(status, bytes)?;
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
    if let Some(error) = value
        .pointer("/ResponseMetadata/Error")
        .filter(|v| !v.is_null())
    {
        return Err(invalid_response(format!("Ark {action}: {error}")));
    }
    value
        .get_mut("Result")
        .map(Value::take)
        .ok_or_else(|| invalid_response("Ark control reply missing Result"))
}

fn hmac(key: &[u8], message: &[u8]) -> Result<Vec<u8>, ChannelError> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|_| ChannelError::InvalidCredential)?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sign(
    url: &str,
    body: &[u8],
    secret: &Value,
    timestamp: &str,
) -> Result<HeaderMap, ChannelError> {
    let key = |name| {
        secret
            .get(name)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                ChannelError::InvalidConfig(format!(
                    "Ark control APIs require credential secret.{name}"
                ))
            })
    };
    let ak = key("access_key_id")?;
    let sk = key("secret_access_key")?;
    let url = url::Url::parse(url).map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| ChannelError::InvalidConfig("Ark control URL has no host".into()))?;
    let host = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.into(),
    };
    let mut query: Vec<_> = url
        .query_pairs()
        .map(|(key, value)| {
            (
                utf8_percent_encode(&key, QUERY).to_string(),
                utf8_percent_encode(&value, QUERY).to_string(),
            )
        })
        .collect();
    query.sort();
    let query = query
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let digest = hex(Sha256::digest(body));
    let mut signed = std::collections::BTreeMap::from([
        ("content-type", "application/json".to_owned()),
        ("host", host),
        ("x-content-sha256", digest.clone()),
        ("x-date", timestamp.into()),
    ]);
    if let Some(token) = secret.get("session_token").and_then(Value::as_str) {
        signed.insert("x-security-token", token.into());
    }
    let names = signed.keys().copied().collect::<Vec<_>>().join(";");
    let canonical_headers: String = signed
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect();
    let canonical = format!(
        "POST\n{}\n{query}\n{canonical_headers}\n{names}\n{digest}",
        url.path()
    );
    let date = &timestamp[..8];
    let scope = format!("{date}/cn-beijing/ark/request");
    let to_sign = format!(
        "HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex(Sha256::digest(canonical.as_bytes()))
    );
    let mut signing_key = sk.as_bytes().to_vec();
    for part in [date, "cn-beijing", "ark", "request"] {
        signing_key = hmac(&signing_key, part.as_bytes())?;
    }
    let authorization = format!(
        "HMAC-SHA256 Credential={ak}/{scope}, SignedHeaders={names}, Signature={}",
        hex(hmac(&signing_key, to_sign.as_bytes())?)
    );
    signed.insert("authorization", authorization);
    signed
        .into_iter()
        .map(|(name, value)| {
            Ok((
                http::HeaderName::from_static(name),
                HeaderValue::from_str(&value).map_err(|_| ChannelError::InvalidCredential)?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::sign;
    use serde_json::json;

    #[test]
    fn signatures_match_the_official_python_sdk_with_and_without_sts() {
        // Generated independently with volcenginesdkcore/signv4.py, fixed
        // datetime 2026-10-20 00:00:00 UTC, fixture keys, and this exact body.
        for (token, signature) in [
            (
                None,
                "453cd4fd5b61487d3d7b29c9a464ec592084c53f1507fa4e929ebad1a08e5221",
            ),
            (
                Some("fixture-session-token"),
                "5751d4391ed61e93638bbcb181dc7d966588fac1a66b9cb34cb1cca3e8198a07",
            ),
        ] {
            let secret = json!({"access_key_id": "fixture-ak", "secret_access_key": "fixture-sk", "session_token": token});
            let headers = sign(
                "https://ark.cn-beijing.volcengineapi.com/?Version=2024-01-01&Action=ListArkAgentPlanModel",
                br#"{"Edition":"personal"}"#, &secret, "20261020T000000Z",
            ).unwrap();
            assert!(
                headers["authorization"]
                    .to_str()
                    .unwrap()
                    .ends_with(signature)
            );
            assert_eq!(
                headers["x-content-sha256"],
                "be6aff1f0d67674666086588f30ec531d8263e1e138de36a505ed26a4b90dd80"
            );
            assert_eq!(
                headers.get("x-security-token").map(|v| v.to_str().unwrap()),
                token
            );
        }
    }
}
