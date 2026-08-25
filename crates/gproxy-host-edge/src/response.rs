mod headers;

use bytes::Bytes;
use gproxy_core::{CoreError, ExecOutcome, ResponseBody};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use js_sys::Uint8Array;
use wasm_bindgen::JsValue;
use web_sys::Response;

use crate::edge::EdgeReply;
use crate::websocket::PreparedWebSocket;

pub(crate) fn buffered(
    response: http::Response<Bytes>,
    request_id: &str,
) -> Result<Response, JsValue> {
    let (parts, body) = response.into_parts();
    full(parts.status, parts.headers, body, request_id, false)
}

pub(crate) fn local_error(
    status: StatusCode,
    message: &'static str,
    request_id: &str,
) -> Result<Response, JsValue> {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    full(
        status,
        headers,
        Bytes::from(format!(r#"{{"error":{{"message":"{message}"}}}}"#)),
        request_id,
        false,
    )
}

pub(crate) async fn outcome(
    method: Method,
    result: Result<ExecOutcome, CoreError>,
    mut upgrade: Option<PreparedWebSocket>,
    request_id: &str,
) -> Result<EdgeReply, JsValue> {
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(upgrade) = upgrade {
                upgrade.close(None);
            }
            return core_error(error, request_id).map(EdgeReply::from);
        }
    };
    let ExecOutcome {
        status,
        headers,
        body,
        ..
    } = outcome;
    if upgrade.is_some() && !matches!(&body, ResponseBody::WebSocket(_)) {
        upgrade.take().expect("checked above").close(None);
        if status.is_success() {
            dispose(body).await;
            return local_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "websocket request returned a non-websocket response",
                request_id,
            )
            .map(EdgeReply::from);
        }
    }
    let headers = match headers::sanitize(headers, request_id) {
        Ok(headers) => headers,
        Err(error) => {
            dispose(body).await;
            if let Some(upgrade) = upgrade {
                upgrade.close(None);
            }
            return Err(error);
        }
    };
    match body {
        ResponseBody::Full(body) => full(
            status,
            headers,
            body,
            request_id,
            headers::omit_body(&method, status),
        )
        .map(EdgeReply::from),
        ResponseBody::Stream(stream) => {
            if headers::omit_body(&method, status) {
                crate::stream::drain_stream(stream).await;
                return empty(status, headers).map(EdgeReply::from);
            }
            let body = crate::stream::StreamBody::new(stream).await?;
            let init = headers::init(status, &headers)?;
            match Response::new_with_opt_readable_stream_and_init(Some(&body.readable()), &init) {
                Ok(response) => Ok(EdgeReply::from(response)),
                Err(error) => {
                    body.drain().await;
                    Err(error)
                }
            }
        }
        ResponseBody::WebSocket(mut upstream) => {
            let Some(upgrade) = upgrade else {
                let _ = upstream
                    .send(gproxy_channel_api::WsFrame::Close(None))
                    .await;
                return local_error(
                    StatusCode::BAD_REQUEST,
                    "websocket upgrade required",
                    request_id,
                )
                .map(EdgeReply::from);
            };
            upgrade.start(upstream, &headers).await
        }
    }
}

async fn dispose(body: ResponseBody) {
    match body {
        ResponseBody::Full(_) => {}
        ResponseBody::Stream(stream) => crate::stream::drain_stream(stream).await,
        ResponseBody::WebSocket(mut socket) => {
            let _ = socket.send(gproxy_channel_api::WsFrame::Close(None)).await;
        }
    }
}

fn core_error(error: CoreError, request_id: &str) -> Result<Response, JsValue> {
    let status = error.status();
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    full(
        status,
        headers,
        Bytes::from(error.body_json().to_string()),
        request_id,
        false,
    )
}

fn full(
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    request_id: &str,
    omit: bool,
) -> Result<Response, JsValue> {
    let headers = headers::sanitize(headers, request_id)?;
    let init = headers::init(status, &headers)?;
    if omit {
        return Response::new_with_opt_str_and_init(None, &init);
    }
    let body = Uint8Array::from(body.as_ref());
    Response::new_with_opt_js_u8_array_and_init(Some(&body), &init)
}

fn empty(status: StatusCode, headers: HeaderMap) -> Result<Response, JsValue> {
    let init = headers::init(status, &headers)?;
    Response::new_with_opt_str_and_init(None, &init)
}
