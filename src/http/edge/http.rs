//! Edge `web_sys::Response` builders for the admin/portal surface.
//!
//! Compiled only under the wasm edge build (this module lives inside
//! `crate::http::edge`, which is already cfg-gated). The PURE parse helpers
//! (path segmenting, JSON/query decoding) live in the cross-target
//! `crate::http::admin_api` so both hosts use the same dispatcher;
//! this module keeps ONLY the `web_sys`-specific response construction.

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
    super::bridge::js_response(status.as_u16(), &headers, &bytes)
}

fn js_err(e: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{e:?}"))
}
