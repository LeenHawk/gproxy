use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsValue;
use web_sys::{Headers, Response, ResponseInit};

fn js_err(e: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{e:?}"))
}

/// Build a 503 (init-not-called) plain-text response.
pub(super) fn service_unavailable(msg: &str) -> Result<Response, JsValue> {
    text_response(503, "text/plain", msg.as_bytes())
}

/// Convert the cross-target admin/portal response into a WinterCG response.
pub(super) fn resp_to_ws(resp: crate::http::admin_api::Resp) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    for (name, value) in &resp.headers {
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v).map_err(js_err)?;
        }
    }
    js_response(resp.status.as_u16(), &headers, &resp.body)
}

/// Convert target-independent ops response data to a WinterCG response.
pub(super) fn ops_response(resp: crate::http::ops::OpsResponse) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    for (name, value) in &resp.headers {
        if let Ok(value) = value.to_str() {
            headers.append(name.as_str(), value).map_err(js_err)?;
        }
    }
    js_response(resp.status.as_u16(), &headers, &resp.body)
}

/// Build the 401 for missing/invalid admin auth on an ops endpoint.
pub(super) fn unauthorized() -> Result<Response, JsValue> {
    text_response(401, "text/plain", b"unauthorized")
}

/// Build the 413 for an over-cap request body.
pub(super) fn payload_too_large() -> Result<Response, JsValue> {
    text_response(413, "text/plain", b"request body too large")
}

/// Build a response with a single `Content-Type` header and a body.
pub(super) fn text_response(
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    headers
        .append("content-type", content_type)
        .map_err(js_err)?;
    js_response(status, &headers, body)
}

/// Render a pipeline error to a JSON response, redacted identically to native.
pub(super) fn error_to_ws(e: &crate::pipeline::error::PipelineError) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    headers
        .append("content-type", "application/json")
        .map_err(js_err)?;
    if let Some(secs) = e.retry_after_secs() {
        headers
            .append("retry-after", &secs.to_string())
            .map_err(js_err)?;
    }
    js_response(e.status().as_u16(), &headers, e.error_json().as_bytes())
}

/// Map a pipeline outcome to a WinterCG response, preserving streaming.
pub(super) fn outcome_to_ws(
    outcome: crate::pipeline::outcome::ExecOutcome,
    request_id: &str,
) -> Result<Response, JsValue> {
    use crate::pipeline::outcome::ResponseBody;

    let metadata = crate::http::egress::metadata(&outcome, request_id);
    let headers = Headers::new().map_err(js_err)?;
    for (name, value) in &metadata.headers {
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v).map_err(js_err)?;
        }
    }

    match outcome.body {
        ResponseBody::Full(bytes) => js_response(metadata.status.as_u16(), &headers, &bytes),
        ResponseBody::Stream(stream) => {
            js_stream_response(metadata.status.as_u16(), &headers, stream)
        }
    }
}

/// Build a JS response whose body pulls chunks from a Rust byte stream.
fn js_stream_response(
    status: u16,
    headers: &Headers,
    stream: crate::pipeline::outcome::ByteStream,
) -> Result<Response, JsValue> {
    use futures_util::StreamExt;

    let stream = stream.map(|item| match item {
        Ok(bytes) => {
            // Own each chunk on the JS heap; a wasm-memory view can be invalidated
            // by later allocations before the host consumes it.
            let chunk = Uint8Array::new_with_length(bytes.len() as u32);
            chunk.copy_from(&bytes);
            Ok(JsValue::from(chunk))
        }
        Err(error) => Err(JsValue::from_str(&error.to_string())),
    });
    let body = wasm_streams::ReadableStream::from_stream(stream).into_raw();
    let init = ResponseInit::new();
    init.set_status(status);
    init.set_headers_headers(headers);
    Response::new_with_opt_readable_stream_and_init(Some(&body), &init).map_err(js_err)
}

/// Core response builder: status + headers + a JS-owned body copy.
pub(super) fn js_response(
    status: u16,
    headers: &Headers,
    body: &[u8],
) -> Result<Response, JsValue> {
    let init = ResponseInit::new();
    init.set_status(status);
    init.set_headers_headers(headers);
    let js_body = Uint8Array::new_with_length(body.len() as u32);
    js_body.copy_from(body);
    Response::new_with_opt_js_u8_array_and_init(Some(&js_body), &init).map_err(js_err)
}

pub(super) fn messages_to_js_array(messages: Vec<String>) -> Array {
    let out = Array::new();
    for message in messages {
        out.push(&JsValue::from_str(&message));
    }
    out
}
