use super::super::quota_balances::{amount, entry};
use super::{field, invalid};
use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, QuotaAvailability, QuotaBalance, QuotaComponent, QuotaEntry, QuotaSubject,
    QuotaValue,
};
use hmac::{Hmac, KeyInit as _, Mac as _};
use http::Request;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

pub(super) fn identity(secret: &Value) -> Option<(&str, &str, Option<&str>)> {
    let separate = field(secret, "quota_access_key_id").is_some()
        || field(secret, "quota_access_key_secret").is_some();
    if separate {
        Some((
            field(secret, "quota_access_key_id")?,
            field(secret, "quota_access_key_secret")?,
            field(secret, "quota_session_token"),
        ))
    } else {
        Some((
            field(secret, "access_key_id")?,
            field(secret, "access_key_secret").or_else(|| field(secret, "secret_access_key"))?,
            field(secret, "session_token"),
        ))
    }
}

pub(super) fn prepare(secret: &Value) -> Result<Request<Bytes>, ChannelError> {
    let (access, key, token) = identity(secret).ok_or_else(|| {
        ChannelError::Secret("Alibaba Cloud management access key pair is required".into())
    })?;
    let uri = super::prepare::endpoint(secret, "https://business.aliyuncs.com", "/")?;
    if uri.path() != "/" {
        return Err(invalid(
            "Alibaba Cloud RPC quota endpoint must use the root path",
        ));
    }
    let mut request = Request::get(uri)
        .body(Bytes::new())
        .map_err(|_| invalid("Invalid Alibaba Cloud quota request"))?;
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs() as i64;
    let timestamp = time::OffsetDateTime::from_unix_timestamp(now).expect("valid epoch timestamp");
    let date = format!(
        "{}T{:02}:{:02}:{:02}Z",
        timestamp.date(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second()
    );
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|_| invalid("Quota signing random source unavailable"))?;
    sign(&mut request, access, key, token, &date, &hex(&nonce))?;
    Ok(request)
}

fn sign(
    request: &mut Request<Bytes>,
    access: &str,
    key: &str,
    token: Option<&str>,
    date: &str,
    nonce: &str,
) -> Result<(), ChannelError> {
    let payload = hex(&Sha256::digest(request.body()));
    let host = request
        .uri()
        .authority()
        .ok_or_else(|| invalid("Alibaba Cloud quota endpoint has no host"))?
        .as_str()
        .to_owned();
    let mut headers = BTreeMap::from([
        ("host", host.as_str()),
        ("x-acs-action", "QueryAccountBalance"),
        ("x-acs-content-sha256", payload.as_str()),
        ("x-acs-date", date),
        ("x-acs-signature-nonce", nonce),
        ("x-acs-version", "2017-12-14"),
    ]);
    if let Some(token) = token {
        headers.insert("x-acs-security-token", token);
    }
    let signed = headers.keys().copied().collect::<Vec<_>>().join(";");
    let canonical_headers = headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect::<String>();
    let canonical = format!("GET\n/\n\n{canonical_headers}\n{signed}\n{payload}");
    let to_sign = format!("ACS3-HMAC-SHA256\n{}", hex(&Sha256::digest(canonical)));
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
        .map_err(|_| invalid("Invalid Alibaba signing key"))?;
    mac.update(to_sign.as_bytes());
    let authorization = format!(
        "ACS3-HMAC-SHA256 Credential={access},SignedHeaders={signed},Signature={}",
        hex(&mac.finalize().into_bytes())
    );
    for (name, value) in headers {
        request.headers_mut().insert(
            name,
            value
                .parse()
                .map_err(|_| invalid("Invalid Alibaba quota signing header"))?,
        );
    }
    request.headers_mut().insert(
        "authorization",
        authorization
            .parse()
            .map_err(|_| invalid("Invalid Alibaba signing authorization"))?,
    );
    request
        .headers_mut()
        .insert("accept", "application/json".parse().expect("static header"));
    Ok(())
}

pub(super) fn parse(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    if raw.get("Success").and_then(Value::as_bool) != Some(true) {
        return Err(invalid(
            "Alibaba account balance query did not report success",
        ));
    }
    let data = &raw["Data"];
    let unit =
        field(data, "Currency").ok_or_else(|| invalid("Missing Alibaba balance currency"))?;
    let components = [
        ("AvailableCashAmount", "cash"),
        ("CreditAmount", "credit"),
        ("MybankCreditAmount", "mybank_credit"),
    ]
    .into_iter()
    .filter(|(field, _)| data.get(*field).is_some())
    .map(|(field, kind)| {
        Ok(QuotaComponent {
            kind: kind.into(),
            amount: amount(data, field)?,
        })
    })
    .collect::<Result<Vec<_>, ChannelError>>()?;
    Ok(vec![entry(
        "balance",
        &format!("account:{unit}"),
        QuotaSubject::Account,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(amount(data, "AvailableAmount")?),
            unit: Some(unit.into()),
            availability: QuotaAvailability::Unknown,
            components,
        }),
    )])
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(&mut out, "{byte:02x}").expect("String write");
        out
    })
}
