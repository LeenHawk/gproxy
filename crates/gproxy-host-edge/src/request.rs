use bytes::Bytes;
use http::header::{CONNECTION, HOST, UPGRADE};
use http::request::Parts;
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, Url};

pub(crate) const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;

pub(crate) struct Incoming {
    pub parts: Parts,
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub body: Bytes,
}

pub(crate) struct RequestError {
    pub status: http::StatusCode,
    pub message: &'static str,
}

pub(crate) async fn read(
    request: &Request,
    client_source: String,
) -> Result<Incoming, RequestError> {
    let url = Url::new(&request.url()).map_err(|_| bad_request("invalid request URL"))?;
    let method = Method::from_bytes(request.method().as_bytes())
        .map_err(|_| bad_request("invalid request method"))?;
    let mut headers = headers(request)?;
    headers.insert(
        http::HeaderName::from_static("x-forwarded-proto"),
        HeaderValue::from_str(url.protocol().trim_end_matches(':'))
            .map_err(|_| bad_request("invalid request scheme"))?,
    );
    if !headers.contains_key(HOST) {
        headers.insert(
            HOST,
            HeaderValue::from_str(&url.host()).map_err(|_| bad_request("invalid request host"))?,
        );
    }
    if let Some(length) = headers
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        && length > MAX_BODY_BYTES
    {
        return Err(RequestError {
            status: http::StatusCode::PAYLOAD_TOO_LARGE,
            message: "request body too large",
        });
    }
    let body = if matches!(method, Method::GET | Method::HEAD) {
        Vec::new()
    } else {
        let buffer = request
            .array_buffer()
            .map_err(|_| bad_request("request body could not be read"))?;
        let buffer = JsFuture::from(buffer)
            .await
            .map_err(|_| bad_request("request body could not be read"))?;
        Uint8Array::new(&buffer).to_vec()
    };
    if body.len() > MAX_BODY_BYTES {
        return Err(RequestError {
            status: http::StatusCode::PAYLOAD_TOO_LARGE,
            message: "request body too large",
        });
    }
    let path = url.pathname();
    let query = url
        .search()
        .strip_prefix('?')
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let mut parts = http::Request::builder()
        .method(method.clone())
        .uri(if let Some(query) = &query {
            format!("{path}?{query}")
        } else {
            path.clone()
        })
        .body(())
        .map_err(|_| bad_request("invalid request URI"))?
        .into_parts()
        .0;
    parts.headers = headers;
    parts
        .extensions
        .insert(gproxy_admin::AuthSource(client_source));
    Ok(Incoming {
        parts,
        method,
        path,
        query,
        body: Bytes::from(body),
    })
}

pub(crate) fn has_websocket_intent(headers: &HeaderMap) -> bool {
    headers
        .keys()
        .any(|name| name.as_str().starts_with("sec-websocket-"))
        || headers
            .get_all(UPGRADE)
            .iter()
            .any(|value| contains_token(value, b"websocket"))
        || headers
            .get_all(CONNECTION)
            .iter()
            .any(|value| contains_token(value, b"upgrade"))
}

fn headers(request: &Request) -> Result<HeaderMap, RequestError> {
    let mut result = HeaderMap::new();
    let iterator = js_sys::try_iter(request.headers().as_ref())
        .map_err(|_| bad_request("request headers are not iterable"))?
        .ok_or_else(|| bad_request("request headers are not iterable"))?;
    for entry in iterator {
        let pair: Array = entry
            .map_err(|_| bad_request("invalid request header"))?
            .dyn_into()
            .map_err(|_| bad_request("invalid request header"))?;
        let name = pair
            .get(0)
            .as_string()
            .ok_or_else(|| bad_request("invalid request header name"))?;
        let value = pair
            .get(1)
            .as_string()
            .ok_or_else(|| bad_request("invalid request header value"))?;
        result.append(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| bad_request("invalid request header name"))?,
            HeaderValue::from_str(&value)
                .map_err(|_| bad_request("invalid request header value"))?,
        );
    }
    Ok(result)
}

fn bad_request(message: &'static str) -> RequestError {
    RequestError {
        status: http::StatusCode::BAD_REQUEST,
        message,
    }
}

fn contains_token(value: &HeaderValue, expected: &[u8]) -> bool {
    value
        .as_bytes()
        .split(|byte| *byte == b',')
        .map(trim_ascii)
        .any(|token| token.eq_ignore_ascii_case(expected))
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}
