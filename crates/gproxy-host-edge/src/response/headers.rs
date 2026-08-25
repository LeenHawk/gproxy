use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use wasm_bindgen::JsValue;
use web_sys::{Headers, ResponseInit};

pub(super) fn init(status: StatusCode, headers: &HeaderMap) -> Result<ResponseInit, JsValue> {
    let init = ResponseInit::new();
    init.set_status(status.as_u16());
    init.set_headers_headers(&web_headers(headers)?);
    Ok(init)
}

pub(super) fn sanitize(mut headers: HeaderMap, request_id: &str) -> Result<HeaderMap, JsValue> {
    let nominated = headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect::<Vec<_>>();
    for name in [
        "connection",
        "content-length",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
    for name in nominated {
        headers.remove(name);
    }
    headers.insert(
        "x-request-id",
        HeaderValue::from_str(request_id)
            .map_err(|_| JsValue::from_str("invalid host request id"))?,
    );
    Ok(headers)
}

pub(super) fn omit_body(method: &Method, status: StatusCode) -> bool {
    *method == Method::HEAD
        || matches!(
            status,
            StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT | StatusCode::NOT_MODIFIED
        )
}

fn web_headers(headers: &HeaderMap) -> Result<Headers, JsValue> {
    let result = Headers::new()?;
    for (name, value) in headers {
        result.append(
            name.as_str(),
            value
                .to_str()
                .map_err(|_| JsValue::from_str("response header is not representable by fetch"))?,
        )?;
    }
    Ok(result)
}
