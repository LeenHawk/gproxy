use bytes::Bytes;
use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

// The sibling `http` module shadows the external crate's bare path.
use ::http::{HeaderMap, Method, Uri, header::HeaderName};

use crate::app::AppState;

use super::{bridge, init};

/// WinterCG fetch entry-point: receives an inbound Request, dispatches it
/// through the same pipeline native uses, directly rather than via axum.
/// Returns 503 if [`super::init`] has not yet been called.
// Do not export this as plain `fetch`: wasm-bindgen's Deno loader also calls a
// top-level `fetch(wasmUrl)`, and an exported function with that name shadows
// the runtime global during module initialisation.
#[wasm_bindgen(js_name = gproxyFetch)]
pub async fn fetch(req: web_sys::Request) -> Result<Response, JsValue> {
    let Some(state) = init::state() else {
        return bridge::service_unavailable("GPROXY edge not initialised: call init() first");
    };

    // §7.2 lazy snapshot refresh: edge has no pub/sub listener, so poll the
    // shared config-version stamp (throttled) and rebuild when it moved.
    init::refresh_snapshot_if_stale(state).await;

    // Body cap (shared with native's DefaultBodyLimit): reject via
    // content-length before buffering when the header is present.
    if content_length_exceeds(&req, crate::config::MAX_BODY_BYTES) {
        return bridge::payload_too_large();
    }
    let (parts, body) = ws_request_to_parts(req).await?;
    // Re-check actual buffered length because content-length can be absent or
    // incorrect. Both paths produce a clean 413 rather than a JS exception.
    if body.len() > crate::config::MAX_BODY_BYTES {
        return bridge::payload_too_large();
    }
    let path = parts.uri.path().to_string();

    // Operational endpoints share the admin auth used by /admin/*.
    match path.as_str() {
        "/healthz" => {
            return if admin_ok(state, &parts.headers).await {
                bridge::ops_response(crate::http::ops::healthz())
            } else {
                bridge::unauthorized()
            };
        }
        "/version" => {
            return if admin_ok(state, &parts.headers).await {
                bridge::ops_response(crate::http::ops::version())
            } else {
                bridge::unauthorized()
            };
        }
        "/metrics" => {
            if !admin_ok(state, &parts.headers).await {
                return bridge::unauthorized();
            }
            let aggregate = match crate::store::persistence::PersistenceBackend::metrics_aggregate(
                state.persistence.as_ref(),
            )
            .await
            {
                Ok(aggregate) => Some(aggregate),
                Err(error) => {
                    tracing::warn!(error = %error, "metrics aggregate failed");
                    None
                }
            };
            return bridge::ops_response(crate::http::ops::metrics(aggregate.as_ref()));
        }
        _ => {}
    }

    // Admin control-plane + portal paths use the cross-target dispatcher. An
    // unhandled path beneath either prefix is a 404, never a gateway fallback.
    if path.starts_with("/admin/") || path.starts_with("/user/") {
        let request = crate::http::admin_api::Request::new(
            parts.method.clone(),
            parts.uri.clone(),
            parts.headers.clone(),
        );
        return match crate::http::admin_api::dispatch(state, &request, &body).await {
            Some(resp) => bridge::resp_to_ws(resp),
            None => super::http::api_err_response(&crate::api::error::ApiError::NotFound(
                "not found".into(),
            )),
        };
    }

    // Gateway: `/v1/...` and gemini's `/v1beta/...` are aggregated; anything
    // else is `/{provider}/v1[beta]/...` scoped.
    let scoped = !(path == "/v1"
        || path.starts_with("/v1/")
        || path == "/v1beta"
        || path.starts_with("/v1beta/"));
    let ctx = match crate::http::server::extract::build_ctx(parts, body, scoped) {
        Ok(c) => c,
        Err(e) => return bridge::error_to_ws(&e),
    };
    let request_id = ctx.request_id.clone();
    match crate::pipeline::execute(state, ctx).await {
        Ok(outcome) => bridge::outcome_to_ws(outcome, &request_id),
        Err(e) => bridge::error_to_ws(&e),
    }
}

/// Edge host hook for downstream Responses WebSocket frames.
///
/// Platform JS owns the WebSocket upgrade and calls this once per inbound
/// message. Returned array items are JSON text messages to send on the socket.
#[wasm_bindgen]
pub async fn responses_websocket_frame(
    req: web_sys::Request,
    frame: String,
) -> Result<Array, JsValue> {
    let Some(state) = init::state() else {
        return Ok(bridge::messages_to_js_array(vec![
            crate::http::responses_ws::WsFrameError::plain(
                ::http::StatusCode::SERVICE_UNAVAILABLE,
                "GPROXY edge not initialised: call init() first",
            )
            .to_frame(),
        ]));
    };

    init::refresh_snapshot_if_stale(state).await;

    let parts = ws_request_metadata_to_parts(&req)?;
    let path = parts.uri.path().to_string();
    if !crate::http::responses_ws::is_responses_websocket_path(&path) {
        return Ok(bridge::messages_to_js_array(vec![
            crate::http::responses_ws::WsFrameError::plain(
                ::http::StatusCode::NOT_FOUND,
                "unsupported path",
            )
            .to_frame(),
        ]));
    }
    let scoped = crate::http::responses_ws::is_scoped_responses_websocket_path(&path);
    let base = crate::http::responses_ws::ResponsesWsRequestBase::from_parts(&parts);
    let messages =
        crate::http::responses_ws::execute_frame_collect(state, &base, scoped, &frame).await;
    Ok(bridge::messages_to_js_array(messages))
}

/// Shared `/healthz` + `/version` + `/metrics` gate.
async fn admin_ok(state: &AppState, headers: &HeaderMap) -> bool {
    crate::admin::authenticate_admin(state, headers)
        .await
        .is_some()
}

/// `true` when a present, parseable `content-length` already exceeds `max`.
fn content_length_exceeds(req: &web_sys::Request, max: usize) -> bool {
    matches!(
        req.headers().get("content-length"),
        Ok(Some(v)) if v.trim().parse::<u64>().is_ok_and(|n| n > max as u64)
    )
}

/// Convert `web_sys::Request` to HTTP request parts and a buffered body.
async fn ws_request_to_parts(
    req: web_sys::Request,
) -> Result<(::http::request::Parts, Bytes), JsValue> {
    let body_bytes: Bytes = {
        let buf_promise = req.array_buffer().map_err(js_err)?;
        let buf_val = JsFuture::from(buf_promise).await.map_err(js_err)?;
        Uint8Array::new(&buf_val).to_vec().into()
    };

    let (parts, _) = request_builder_for(&req)?
        .body(())
        .map_err(js_err)?
        .into_parts();
    Ok((parts, body_bytes))
}

fn ws_request_metadata_to_parts(req: &web_sys::Request) -> Result<::http::request::Parts, JsValue> {
    let (parts, _) = request_builder_for(req)?
        .body(())
        .map_err(js_err)?
        .into_parts();
    Ok(parts)
}

fn request_builder_for(req: &web_sys::Request) -> Result<::http::request::Builder, JsValue> {
    let method = Method::from_bytes(req.method().as_bytes()).map_err(js_err)?;
    let uri: Uri = req.url().parse().map_err(js_err)?;

    // Copy headers; skip empty/unparseable names so a bad header cannot poison
    // the whole builder.
    let mut builder = ::http::Request::builder().method(method).uri(uri);
    let ws_headers = req.headers();
    if let Some(iter) = js_sys::try_iter(&ws_headers).map_err(js_err)? {
        for entry in iter {
            let entry = entry.map_err(js_err)?;
            let arr: js_sys::Array = entry.unchecked_into();
            let name = arr.get(0).as_string().unwrap_or_default();
            let val = arr.get(1).as_string().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            if let Ok(hn) = HeaderName::try_from(name.as_str()) {
                builder = builder.header(hn, val.as_str());
            }
        }
    }

    Ok(builder)
}

fn js_err(e: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{e:?}"))
}
