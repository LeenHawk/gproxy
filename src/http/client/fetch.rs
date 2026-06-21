//! Edge (wasm32) implementation of [`UpstreamClient`] using the host fetch API.
//!
//! Dispatches via `WorkerGlobalScope.fetch()` (Cloudflare Workers / WinterCG).
//!
// TODO: unverified end-to-end — no edge runtime to round-trip against yet;
//       compile-checked only.

use bytes::Bytes;
use js_sys::{Uint8Array, global};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response, WorkerGlobalScope};

use super::{ClientError, UpstreamClient};

fn js_err(e: wasm_bindgen::JsValue) -> ClientError {
    ClientError::Transport(format!("{e:?}"))
}

/// Upstream client that delegates to the host `fetch` (Cloudflare Workers / WinterCG).
pub struct FetchClient;

impl FetchClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FetchClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl UpstreamClient for FetchClient {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        let (parts, body_bytes) = req.into_parts();

        // Build web_sys::Headers from http::HeaderMap.
        let js_headers = Headers::new().map_err(js_err)?;
        for (name, value) in &parts.headers {
            let val_str = value
                .to_str()
                .map_err(|e| ClientError::Transport(e.to_string()))?;
            js_headers.append(name.as_str(), val_str).map_err(js_err)?;
        }

        // Set up RequestInit: method, headers, body (as Uint8Array).
        let init = RequestInit::new();
        init.set_method(parts.method.as_str());
        init.set_headers_headers(&js_headers);
        // The Fetch standard throws TypeError if a body is set on GET/HEAD.
        // Only set body when the method allows it and the body is non-empty.
        if parts.method != http::Method::GET
            && parts.method != http::Method::HEAD
            && !body_bytes.is_empty()
        {
            let body_arr = Uint8Array::from(body_bytes.as_ref());
            init.set_body_opt_u8_array(Some(&body_arr));
        }

        // Build Request from the URI string.
        let uri_str = parts.uri.to_string();
        let js_req = Request::new_with_str_and_init(&uri_str, &init).map_err(js_err)?;

        // Dispatch via WorkerGlobalScope.fetch().
        let scope = global().unchecked_into::<WorkerGlobalScope>();
        let resp_val = JsFuture::from(scope.fetch_with_request(&js_req))
            .await
            .map_err(js_err)?;
        let js_resp: Response = resp_val.unchecked_into();

        let status_code = js_resp.status();
        let js_resp_headers = js_resp.headers();

        // Read body via array_buffer().
        let buf_promise = js_resp.array_buffer().map_err(js_err)?;
        let buf_val = JsFuture::from(buf_promise).await.map_err(js_err)?;
        let body_out: Bytes = Uint8Array::new(&buf_val).to_vec().into();

        // Convert headers back into http::HeaderMap.
        let mut http_headers = http::HeaderMap::new();
        let header_iter = js_sys::try_iter(&js_resp_headers).map_err(js_err)?;
        if let Some(iter) = header_iter {
            for entry in iter {
                let entry = entry.map_err(js_err)?;
                let arr: js_sys::Array = entry.unchecked_into();
                let name = arr.get(0).as_string().unwrap_or_default();
                let val = arr.get(1).as_string().unwrap_or_default();
                if let (Ok(hn), Ok(hv)) = (
                    http::header::HeaderName::try_from(name.as_str()),
                    http::header::HeaderValue::try_from(val.as_str()),
                ) {
                    http_headers.append(hn, hv);
                } else {
                    tracing::warn!("dropping unparseable response header: {name}");
                }
            }
        }

        let status = http::StatusCode::from_u16(status_code)
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        let mut builder = http::Response::builder().status(status);
        if let Some(hmap) = builder.headers_mut() {
            *hmap = http_headers;
        }
        builder
            .body(body_out)
            .map_err(|e| ClientError::Transport(e.to_string()))
    }
}
