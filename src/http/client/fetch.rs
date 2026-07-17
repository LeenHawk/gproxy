//! Edge (wasm32) implementation of [`UpstreamClient`] using the host fetch API.
//!
//! Dispatches via `WorkerGlobalScope.fetch()` (Cloudflare Workers / WinterCG).
//!
use bytes::Bytes;
use futures_util::StreamExt;
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

/// Dispatch one request and return response metadata immediately, leaving the
/// JS body untouched so callers can choose buffered or streaming consumption.
async fn fetch_raw(
    req: http::Request<Bytes>,
) -> Result<(http::StatusCode, http::HeaderMap, Response), ClientError> {
    let (parts, body_bytes) = req.into_parts();

    let js_headers = Headers::new().map_err(js_err)?;
    for (name, value) in &parts.headers {
        let value = value
            .to_str()
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        js_headers.append(name.as_str(), value).map_err(js_err)?;
    }

    let init = RequestInit::new();
    init.set_method(parts.method.as_str());
    init.set_headers_headers(&js_headers);
    if parts.method != http::Method::GET
        && parts.method != http::Method::HEAD
        && !body_bytes.is_empty()
    {
        let body = Uint8Array::from(body_bytes.as_ref());
        init.set_body_opt_u8_array(Some(&body));
    }

    let request = Request::new_with_str_and_init(&parts.uri.to_string(), &init).map_err(js_err)?;
    let scope = global().unchecked_into::<WorkerGlobalScope>();
    let value = JsFuture::from(scope.fetch_with_request(&request))
        .await
        .map_err(js_err)?;
    let response: Response = value.unchecked_into();
    let status = http::StatusCode::from_u16(response.status())
        .map_err(|error| ClientError::Transport(error.to_string()))?;
    let headers = response_headers(&response)?;
    Ok((status, headers, response))
}

fn response_headers(response: &Response) -> Result<http::HeaderMap, ClientError> {
    let mut headers = http::HeaderMap::new();
    let Some(iter) = js_sys::try_iter(&response.headers()).map_err(js_err)? else {
        return Ok(headers);
    };
    for entry in iter {
        let entry = entry.map_err(js_err)?;
        let pair: Array = entry.unchecked_into();
        let name = pair.get(0).as_string().unwrap_or_default();
        let value = pair.get(1).as_string().unwrap_or_default();
        if let (Ok(name), Ok(value)) = (
            http::header::HeaderName::try_from(name.as_str()),
            http::header::HeaderValue::try_from(value.as_str()),
        ) {
            headers.append(name, value);
        } else {
            tracing::warn!("dropping unparseable response header: {name}");
        }
    }
    Ok(headers)
}

fn response_with_body(
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
) -> Result<http::Response<Bytes>, ClientError> {
    let mut response = http::Response::builder().status(status);
    if let Some(response_headers) = response.headers_mut() {
        *response_headers = headers;
    }
    response
        .body(body)
        .map_err(|error| ClientError::Transport(error.to_string()))
}

#[async_trait::async_trait(?Send)]
impl UpstreamClient for FetchClient {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        let (status, headers, response) = fetch_raw(req).await?;
        let buf_promise = response.array_buffer().map_err(js_err)?;
        let buf_val = JsFuture::from(buf_promise).await.map_err(js_err)?;
        let body = Uint8Array::new(&buf_val).to_vec().into();
        response_with_body(status, headers, body)
    }

    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<
        (
            http::StatusCode,
            http::HeaderMap,
            crate::http::client::RespStream,
        ),
        ClientError,
    > {
        let (status, headers, response) = fetch_raw(req).await?;
        let stream: crate::http::client::RespStream = match response.body() {
            Some(body) => {
                let chunks = wasm_streams::ReadableStream::from_raw(body)
                    .into_stream()
                    .map(|chunk| {
                        let value = chunk.map_err(js_err)?;
                        let array: Uint8Array = value.dyn_into().map_err(|_| {
                            ClientError::Transport(
                                "fetch response stream yielded a non-Uint8Array chunk".into(),
                            )
                        })?;
                        Ok(Bytes::from(array.to_vec()))
                    });
                Box::pin(chunks)
            }
            None => Box::pin(futures_util::stream::empty()),
        };
        Ok((status, headers, stream))
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
