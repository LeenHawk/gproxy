//! AWS Signature Version 4, ported from the v3 channel
//! (`crates/gproxy-channels/src/aws_bedrock/auth/` on `main`).
//!
//! Signing is pure computation over the request and the credential, which is
//! why it belongs in a synchronous `prepare` rather than in a refresh: no
//! network call, no clock reading of its own (the caller passes Unix seconds),
//! no hidden state.
//!
//! The four steps are AWS's own (SigV4 reference, "Create a signed request"):
//!
//! 1. canonical request = method, canonical URI, canonical query string,
//!    canonical headers, signed header names, payload hash, joined by `\n`;
//! 2. string to sign = `AWS4-HMAC-SHA256`, the `x-amz-date` timestamp, the
//!    credential scope `{date}/{region}/{service}/aws4_request` and the
//!    SHA-256 of the canonical request;
//! 3. signing key = HMAC-SHA256 chained over date, region, service and the
//!    literal `aws4_request`, starting from `AWS4` + the secret access key;
//! 4. `Authorization: AWS4-HMAC-SHA256 Credential=…, SignedHeaders=…,
//!    Signature=…`, alongside `x-amz-date`, `x-amz-content-sha256` and, for a
//!    temporary credential, `x-amz-security-token`.
//!
//! `tests/aws_bedrock.rs` checks this against published known answers: the
//! signing-key derivation example from the AWS documentation and three cases
//! of AWS's `aws-sig-v4-test-suite`.

use std::collections::BTreeMap;

use hmac::{Hmac, KeyInit as _, Mac as _};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use sha2::{Digest as _, Sha256};

use crate::channel::ChannelError;

type HmacSha256 = Hmac<Sha256>;

pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// Headers excluded from `SignedHeaders`. `authorization` carries the result;
/// the rest are rewritten, added or removed by HTTP transports between
/// signing and the wire, so signing them would invalidate the signature.
/// Excluding a header is always allowed except for `host` and `x-amz-*`,
/// which AWS requires to be signed and which this list never touches.
const UNSIGNED: &[&str] = &[
    "authorization",
    "connection",
    "content-length",
    "transfer-encoding",
    "user-agent",
    "accept-encoding",
    "expect",
];

/// An AWS access key pair. `session_token` is present exactly when the
/// credential is a temporary one (STS, an instance role, AWS SSO); it is sent
/// as `x-amz-security-token` and signed with the rest.
#[derive(Clone, Copy)]
pub struct Credentials<'a> {
    pub access_key_id: &'a str,
    pub secret_access_key: &'a str,
    pub session_token: Option<&'a str>,
}

/// The region and service halves of the credential scope. Bedrock's data and
/// control planes both sign as `bedrock`; other AWS APIs reached with the same
/// key pair (Service Quotas, CloudWatch) name their own service here.
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    pub region: &'a str,
    pub service: &'a str,
}

/// The request halves that enter the canonical request. The body must be
/// complete: SigV4 hashes the payload, so a streamed request body cannot be
/// signed.
#[derive(Clone, Copy)]
pub struct SigningRequest<'a> {
    pub method: &'a Method,
    pub uri: &'a Uri,
    pub payload: &'a [u8],
}

/// Add `x-amz-date`, `x-amz-content-sha256`, an optional
/// `x-amz-security-token` and the `Authorization` signature to `headers`.
///
/// `now_secs` is Unix seconds; the caller owns the clock so this function is a
/// pure function of its arguments and can be replayed against test vectors.
pub fn sign(
    request: SigningRequest<'_>,
    headers: &mut HeaderMap,
    credentials: Credentials<'_>,
    scope: Scope<'_>,
    now_secs: u64,
) -> Result<(), ChannelError> {
    let (_, timestamp) = format_time(now_secs);
    insert(headers, "x-amz-date", &timestamp)?;
    insert(
        headers,
        "x-amz-content-sha256",
        &hex(Sha256::digest(request.payload)),
    )?;
    if let Some(token) = credentials.session_token {
        insert(headers, "x-amz-security-token", token)?;
    }
    let value = authorization(request, headers, credentials, scope, now_secs)?;
    insert(headers, "authorization", &value)
}

/// The `Authorization` header value for a request whose headers are already
/// final. `headers` must carry the `x-amz-date` for `now_secs` and, for a
/// temporary credential, `x-amz-security-token`; `sign` writes both before
/// calling this. Kept separate from `sign` so AWS's published test vectors,
/// which sign an exact header set, can be replayed verbatim.
pub fn authorization(
    request: SigningRequest<'_>,
    headers: &HeaderMap,
    credentials: Credentials<'_>,
    scope: Scope<'_>,
    now_secs: u64,
) -> Result<String, ChannelError> {
    let (date, timestamp) = format_time(now_secs);
    let payload_hash = hex(Sha256::digest(request.payload));
    let (canonical_headers, signed_headers) = canonical_headers(request.uri, headers)?;
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.method,
        canonical_uri(request.uri.path())?,
        canonical_query(request.uri.query().unwrap_or_default())?,
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let credential_scope = format!("{date}/{}/{}/aws4_request", scope.region, scope.service);
    let string_to_sign = format!(
        "{ALGORITHM}\n{timestamp}\n{credential_scope}\n{}",
        hex(Sha256::digest(canonical_request.as_bytes()))
    );
    let signature = hex(hmac(
        &signing_key(credentials.secret_access_key, &date, scope)?,
        string_to_sign.as_bytes(),
    )?);
    Ok(format!(
        "{ALGORITHM} Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id
    ))
}

/// `kSigning` from the AWS reference: four chained HMAC-SHA256 rounds over the
/// date, the region, the service and the literal `aws4_request`.
fn signing_key(
    secret_access_key: &str,
    date: &str,
    scope: Scope<'_>,
) -> Result<Vec<u8>, ChannelError> {
    let date_key = hmac(
        format!("AWS4{secret_access_key}").as_bytes(),
        date.as_bytes(),
    )?;
    let region_key = hmac(&date_key, scope.region.as_bytes())?;
    let service_key = hmac(&region_key, scope.service.as_bytes())?;
    hmac(&service_key, b"aws4_request")
}

fn hmac(key: &[u8], value: &[u8]) -> Result<Vec<u8>, ChannelError> {
    // HMAC accepts a key of any length, so this only guards against a future
    // backend that refuses one rather than against a reachable input.
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| ChannelError::InvalidCredential)?;
    mac.update(value);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// Canonical headers plus the `;`-joined signed header names. `host` comes
/// from the URI authority because it is not in the map the channel builds.
fn canonical_headers(uri: &Uri, headers: &HeaderMap) -> Result<(String, String), ChannelError> {
    let authority = uri.authority().ok_or_else(|| {
        ChannelError::InvalidConfig("the AWS endpoint URL has no host to sign".into())
    })?;
    let mut sorted = BTreeMap::<String, Vec<String>>::new();
    sorted.insert("host".into(), vec![authority.as_str().into()]);
    for (name, value) in headers {
        if UNSIGNED.contains(&name.as_str()) {
            continue;
        }
        let value = value
            .to_str()
            .map_err(|_| {
                ChannelError::InvalidConfig(format!(
                    "header `{name}` is not text and cannot be signed"
                ))
            })?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        sorted.entry(name.as_str().into()).or_default().push(value);
    }
    let signed = sorted.keys().cloned().collect::<Vec<_>>().join(";");
    let canonical = sorted
        .into_iter()
        .map(|(name, values)| format!("{name}:{}\n", values.join(",")))
        .collect();
    Ok((canonical, signed))
}

/// The path with every segment normalized to AWS's encoding. Bedrock is not
/// S3, so the path is encoded once rather than twice.
fn canonical_uri(path: &str) -> Result<String, ChannelError> {
    if path.is_empty() {
        return Ok("/".into());
    }
    path.split('/')
        .map(|segment| percent_decode(segment).map(|bytes| encode(&bytes)))
        .collect::<Result<Vec<_>, _>>()
        .map(|segments| segments.join("/"))
}

/// Parameters re-encoded and sorted by encoded name then encoded value; a
/// parameter without `=` signs as an empty value.
fn canonical_query(query: &str) -> Result<String, ChannelError> {
    let mut pairs = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            Ok((
                encode(&percent_decode(name)?),
                encode(&percent_decode(value)?),
            ))
        })
        .collect::<Result<Vec<_>, ChannelError>>()?;
    pairs.sort();
    Ok(pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&"))
}

fn percent_decode(value: &str) -> Result<Vec<u8>, ChannelError> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| ChannelError::InvalidConfig("invalid percent encoding".into()))?;
            let text = std::str::from_utf8(pair)
                .map_err(|_| ChannelError::InvalidConfig("invalid percent encoding".into()))?;
            output.push(
                u8::from_str_radix(text, 16)
                    .map_err(|_| ChannelError::InvalidConfig("invalid percent encoding".into()))?,
            );
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    Ok(output)
}

/// RFC 3986 unreserved characters pass; everything else is `%XX` uppercase.
/// Also the encoding for a Bedrock model id inside a URL path segment.
pub(super) fn encode(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(*byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(*byte >> 4)]));
            output.push(char::from(HEX[usize::from(*byte & 0x0f)]));
        }
    }
    output
}

fn insert(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).map_err(|_| {
            ChannelError::InvalidConfig(format!("`{name}` is not a valid header value"))
        })?,
    );
    Ok(())
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    bytes
        .as_ref()
        .iter()
        .fold(String::new(), |mut output, byte| {
            write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}

/// `(YYYYMMDD, YYYYMMDDTHHMMSSZ)` in UTC, computed from Unix seconds so the
/// crate needs no date library on either native or wasm.
pub fn format_time(seconds: u64) -> (String, String) {
    let days = i64::try_from(seconds / 86_400).expect("Unix days fit in i64");
    let clock = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = clock / 3_600;
    let minute = clock % 3_600 / 60;
    let second = clock % 60;
    (
        format!("{year:04}{month:02}{day:02}"),
        format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z"),
    )
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch to a
/// proleptic Gregorian date.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}
