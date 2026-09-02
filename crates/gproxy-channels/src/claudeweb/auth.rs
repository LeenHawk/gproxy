use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, COOKIE, ORIGIN, REFERER};
use serde_json::Value;

pub(super) const DEFAULT_BASE_URL: &str = "https://claude.ai";
const VALIDATION_SECS: i64 = 12 * 60 * 60;

#[derive(Clone)]
pub(super) struct Auth {
    pub cookie: String,
    pub organization: String,
    pub device_id: Option<String>,
    pub pro: bool,
}

impl Auth {
    pub(super) fn read(secret: &Value) -> Result<Self, ChannelError> {
        let cookie = field(secret, "cookie")
            .or_else(|| field(secret, "session_key"))
            .ok_or_else(|| ChannelError::Secret("Claude sessionKey cookie missing".into()))?;
        let organization = field(secret, "account_uuid")
            .or_else(|| field(secret, "organization_uuid"))
            .ok_or_else(|| ChannelError::Secret("Claude organization UUID missing".into()))?;
        let pro = secret
            .get("capabilities")
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values.iter().filter_map(Value::as_str).any(|value| {
                    ["pro", "max", "team", "enterprise", "raven"]
                        .iter()
                        .any(|tier| value.contains(tier))
                })
            });
        Ok(Self {
            cookie: cookie.into(),
            organization: organization.into(),
            device_id: field(secret, "device_id").map(str::to_owned),
            pro,
        })
    }

    pub(super) fn headers(
        &self,
        base: &str,
        referer: &str,
    ) -> Result<http::HeaderMap, ChannelError> {
        browser_headers(&self.cookie, self.device_id.as_deref(), base, referer)
    }
}

fn browser_headers(
    cookie: &str,
    device_id: Option<&str>,
    base: &str,
    referer: &str,
) -> Result<http::HeaderMap, ChannelError> {
    let mut headers = http::HeaderMap::new();
    insert(&mut headers, COOKIE, &cookie_header(cookie))?;
    insert(&mut headers, ORIGIN, base)?;
    insert(&mut headers, REFERER, referer)?;
    headers.insert(ACCEPT_LANGUAGE, "en-US,en;q=0.9".parse().expect("static"));
    headers.insert(CACHE_CONTROL, "no-cache".parse().expect("static"));
    headers.insert(
        "anthropic-client-platform",
        "web_claude_ai".parse().expect("static"),
    );
    if let Some(device) = device_id {
        insert(
            &mut headers,
            "anthropic-device-id".parse().expect("static"),
            device,
        )?;
        insert(
            &mut headers,
            COOKIE,
            &format!("{}; anthropic-device-id={device}", cookie_header(cookie)),
        )?;
    }
    Ok(headers)
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    Some(
        secret
            .get("validated_at_ms")
            .and_then(Value::as_i64)
            .map_or(i64::MIN, |value| value / 1000 + VALIDATION_SECS),
    )
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    settings: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = validation_request(secret, settings);
    let request = match request {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            return Err(ChannelError::Refresh(format!(
                "Claude bootstrap endpoint {}",
                response.status()
            )));
        }
        super::bootstrap::merge(secret, response.body())
    })
}

fn validation_request(
    secret: &Value,
    settings: &Value,
) -> Result<http::Request<Bytes>, ChannelError> {
    let cookie = field(secret, "cookie")
        .or_else(|| field(secret, "session_key"))
        .ok_or_else(|| ChannelError::Refresh("Claude sessionKey cookie missing".into()))?;
    bootstrap_request(cookie, field(secret, "device_id"), settings)
}

pub(super) fn login_request(
    cookie: &str,
    settings: &Value,
) -> Result<http::Request<Bytes>, ChannelError> {
    bootstrap_request(cookie, None, settings)
}

fn bootstrap_request(
    cookie: &str,
    device_id: Option<&str>,
    settings: &Value,
) -> Result<http::Request<Bytes>, ChannelError> {
    let base = base(settings);
    let url = settings
        .pointer("/endpoints/claudeweb_bootstrap")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{base}/api/bootstrap"));
    let mut request = http::Request::get(url)
        .body(Bytes::new())
        .map_err(|error| ChannelError::Refresh(error.to_string()))?;
    *request.headers_mut() = browser_headers(cookie, device_id, base, &format!("{base}/new"))?;
    request
        .headers_mut()
        .insert(ACCEPT, "application/json".parse().expect("static"));
    request
        .extensions_mut()
        .insert(super::profile::CLIENT_PROFILE.clone());
    Ok(request)
}

pub(super) fn base(settings: &Value) -> &str {
    field(settings, "base_url")
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
}

fn cookie_header(cookie: &str) -> String {
    if cookie.contains("sessionKey=") {
        cookie.into()
    } else {
        format!("sessionKey={cookie}")
    }
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn insert(
    headers: &mut http::HeaderMap,
    name: http::HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        name,
        value
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("invalid browser header: {error}")))?,
    );
    Ok(())
}
