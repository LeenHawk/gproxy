//! Edge (wasm32) implementation of [`UpstreamClient`] using the host fetch API.
//!
//! Dispatches via `WorkerGlobalScope.fetch()` (Cloudflare Workers / WinterCG).
//!
// TODO: unverified end-to-end — no edge runtime to round-trip against yet;
//       compile-checked only.

use bytes::Bytes;
use js_sys::{Array, Uint8Array, global};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response, WorkerGlobalScope};

use super::{ClientError, UpstreamClient};

#[wasm_bindgen(inline_js = r#"
export async function gproxyResponsesWebSocketRoundTrip(url, headerEntries, frame) {
  const headers = new Headers();
  for (const pair of headerEntries) {
    headers.append(pair[0], pair[1]);
  }
  headers.set("Upgrade", "websocket");

  const response = await fetch(url, { method: "GET", headers });
  const socket = response.webSocket;
  if (!socket) {
    throw new Error(`websocket upgrade failed with status ${response.status}`);
  }

  if (typeof socket.accept === "function") {
    socket.accept();
  }

  const decoder = new TextDecoder();
  const messages = [];
  const terminal = new Set(["response.completed", "response.done", "response.failed", "error"]);

  return await new Promise((resolve, reject) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      try { socket.close(); } catch (_) {}
      resolve(messages);
    };
    const fail = (message) => {
      if (settled) return;
      settled = true;
      try { socket.close(); } catch (_) {}
      reject(new Error(message));
    };

    socket.addEventListener("message", (event) => {
      const text = typeof event.data === "string" ? event.data : decoder.decode(event.data);
      messages.push(text);
      let kind = null;
      try { kind = JSON.parse(text)?.type ?? null; } catch (_) {}
      if (terminal.has(kind)) {
        finish();
      }
    });
    socket.addEventListener("close", () => {
      if (settled) return;
      fail("websocket closed before terminal response");
    });
    socket.addEventListener("error", () => fail("websocket error"));

    try {
      socket.send(frame);
    } catch (error) {
      fail(error?.message ?? String(error));
    }
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = gproxyResponsesWebSocketRoundTrip)]
    async fn responses_websocket_round_trip(
        url: String,
        header_entries: Array,
        frame: String,
    ) -> Result<JsValue, JsValue>;
}

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

    async fn send_websocket(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<http::Response<Bytes>, ClientError> {
        let (parts, body) = req.into_parts();
        let frame = String::from_utf8(body.to_vec()).map_err(|error| {
            ClientError::Transport(format!(
                "responses websocket request is not UTF-8 JSON: {error}"
            ))
        })?;
        let header_entries = Array::new();
        for (name, value) in &parts.headers {
            let value = value
                .to_str()
                .map_err(|error| ClientError::Transport(error.to_string()))?;
            let pair = Array::new();
            pair.push(&JsValue::from_str(name.as_str()));
            pair.push(&JsValue::from_str(value));
            header_entries.push(&pair);
        }

        let messages = responses_websocket_round_trip(parts.uri.to_string(), header_entries, frame)
            .await
            .map_err(js_err)?;
        let messages: Array = messages
            .dyn_into()
            .map_err(|_| ClientError::Transport("websocket result was not an array".into()))?;
        let mut body = Vec::new();
        for message in messages.iter() {
            let text = message
                .as_string()
                .ok_or_else(|| ClientError::Transport("websocket message was not text".into()))?;
            body.extend_from_slice(&text_to_sse(&text));
        }

        http::Response::builder()
            .status(http::StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "text/event-stream")
            .body(Bytes::from(body))
            .map_err(|error| ClientError::Transport(error.to_string()))
    }
}

fn text_to_sse(text: &str) -> Vec<u8> {
    let name = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "message".to_owned());
    crate::transform::common::sse::SseFrame::event(name, text.to_owned())
        .encode()
        .into_bytes()
}
