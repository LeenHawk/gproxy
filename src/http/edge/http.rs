//! Edge `web_sys::Response` builders for the admin/portal surface.
//!
//! Compiled only under the wasm edge build (this module lives inside
//! `crate::http::edge`, which is already cfg-gated). The PURE parse helpers
//! (path segmenting, JSON/query decoding) live in the cross-target
//! `crate::http::admin_api` so the dispatcher can be driven by native tests;
//! this module keeps ONLY the `web_sys`-specific response construction.
//!
//! `with_set_cookie` is consumed by the login/logout flows (B6.3); the
//! `dead_code` lint is suppressed until then.
#![allow(dead_code)]

use wasm_bindgen::JsValue;
use web_sys::{Headers, Response};

use crate::api::error::ApiError;

/// Convert an [`ApiError`] into a `web_sys::Response` using [`ApiError::to_parts`].
pub fn api_err_response(e: &ApiError) -> Result<Response, JsValue> {
    let extra = e.extra_headers();
    let (status, bytes) = e.to_parts();
    let headers = Headers::new().map_err(js_err)?;
    headers
        .append("content-type", "application/json")
        .map_err(js_err)?;
    for (name, value) in &extra {
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v).map_err(js_err)?;
        }
    }
    super::js_response(status.as_u16(), &headers, &bytes)
}

/// Build a response with a `Set-Cookie` header.
///
/// Used by the login/logout flows (B6.3) to mint or clear the session cookie.
/// The `body` is assumed to be JSON; `content-type: application/json` is added.
pub fn with_set_cookie(status: u16, set_cookie: &str, body: &[u8]) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    headers
        .append("content-type", "application/json")
        .map_err(js_err)?;
    headers.append("set-cookie", set_cookie).map_err(js_err)?;
    super::js_response(status, &headers, body)
}

fn js_err(e: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{e:?}"))
}
