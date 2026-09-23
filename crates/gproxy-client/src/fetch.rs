//! The JS host's global `fetch` as an `OutboundClient` (wasm32, feature
//! `fetch`). Request bodies stream out as a ReadableStream (`duplex: half`),
//! response bodies stream in; nothing is buffered. With feature `workers` a
//! WebSocket upgrade is a fetch carrying `Upgrade: websocket` whose `101`
//! response exposes `webSocket` (Cloudflare Workers semantics), so the
//! upgrade carries upstream authentication headers like any other request.

use crate::{Error, OutboundClient};
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        UpstreamConnection,
    },
    connection::{Bytes, TransportError},
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use js_sys::{Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

#[derive(Clone)]
pub struct FetchClient {
    fetch: Function,
    default_headers: HeaderMap,
    follow_redirects: bool,
}

impl std::fmt::Debug for FetchClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FetchClient")
    }
}

fn js_message(value: &JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            Reflect::get(value, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| format!("{value:?}"))
}

fn start_error(value: JsValue) -> CapabilityError {
    CapabilityError::new(
        CapabilityErrorKind::Transport,
        CapabilityErrorStage::Start,
        format!("fetch failed: {}", js_message(&value)),
    )
}

fn transport(value: JsValue) -> TransportError {
    Box::new(std::io::Error::other(js_message(&value)))
}

fn invalid(message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(
        CapabilityErrorKind::Invalid,
        CapabilityErrorStage::Start,
        message,
    )
}

impl FetchClient {
    /// Resolve the global `fetch` once; a host without one is a configuration
    /// error at construction, not a failure per request.
    pub fn new() -> Result<Self, Error> {
        let fetch = Reflect::get(&js_sys::global(), &"fetch".into())
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .ok_or_else(|| Error::Host("no global fetch function".into()))?;
        Ok(Self {
            fetch,
            default_headers: HeaderMap::new(),
            follow_redirects: false,
        })
    }

    pub(crate) fn with_config(config: &crate::ConnectionConfig) -> Result<Self, Error> {
        let mut client = Self::new()?;
        client.default_headers = config.default_headers()?;
        client.follow_redirects = config.redirect_max_hops > 0;
        Ok(client)
    }

    async fn call(&self, request: web_sys::Request) -> Result<web_sys::Response, CapabilityError> {
        let promise = self
            .fetch
            .call1(&JsValue::UNDEFINED, &request)
            .map_err(start_error)?
            .dyn_into::<Promise>()
            .map_err(|_| invalid("fetch did not return a promise"))?;
        JsFuture::from(promise)
            .await
            .map_err(start_error)?
            .dyn_into::<web_sys::Response>()
            .map_err(|_| invalid("fetch did not resolve to a Response"))
    }

    fn request(
        &self,
        parts: &http::request::Parts,
        body: Option<HttpBody>,
    ) -> Result<web_sys::Request, CapabilityError> {
        let init = web_sys::RequestInit::new();
        init.set_method(parts.method.as_str());
        init.set_redirect(if self.follow_redirects {
            web_sys::RequestRedirect::Follow
        } else {
            web_sys::RequestRedirect::Manual
        });
        let headers = web_sys::Headers::new().map_err(start_error)?;
        for (name, value) in &self.default_headers {
            if !parts.headers.contains_key(name) {
                headers
                    .append(
                        name.as_str(),
                        value
                            .to_str()
                            .map_err(|_| invalid("default header is not text"))?,
                    )
                    .map_err(start_error)?;
            }
        }
        for (name, value) in &parts.headers {
            let value = value
                .to_str()
                .map_err(|_| invalid(format!("header `{name}` is not text")))?;
            headers.append(name.as_str(), value).map_err(start_error)?;
        }
        init.set_headers_headers(&headers);
        match body {
            None => {}
            Some(HttpBody::Bytes(bytes)) => {
                if !bytes.is_empty() {
                    init.set_body_opt_u8_array(Some(&Uint8Array::from(&bytes[..])));
                }
            }
            Some(HttpBody::Stream(stream)) => {
                let stream = stream.map(|chunk| {
                    chunk
                        .map(|bytes| JsValue::from(Uint8Array::from(&bytes[..])))
                        .map_err(|e| JsValue::from_str(&e.to_string()))
                });
                let readable = wasm_streams::ReadableStream::from_stream(stream);
                init.set_body_opt_readable_stream(Some(&readable.into_raw()));
                // Streaming request bodies require half duplex in the Fetch spec.
                let _ = Reflect::set(init.as_ref(), &"duplex".into(), &"half".into());
            }
        }
        web_sys::Request::new_with_str_and_init(&parts.uri.to_string(), &init).map_err(start_error)
    }

    fn head(response: &web_sys::Response) -> Result<(StatusCode, HeaderMap), CapabilityError> {
        let status = StatusCode::from_u16(response.status())
            .map_err(|_| invalid("fetch returned an invalid status"))?;
        let mut headers = HeaderMap::new();
        let entries = js_sys::try_iter(response.headers().as_ref())
            .map_err(start_error)?
            .ok_or_else(|| invalid("response headers are not iterable"))?;
        for entry in entries {
            let entry = entry.map_err(start_error)?;
            let pair = js_sys::Array::from(&entry);
            let (Some(name), Some(value)) = (pair.get(0).as_string(), pair.get(1).as_string())
            else {
                continue;
            };
            if let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(&value),
            ) {
                headers.append(name, value);
            }
        }
        Ok((status, headers))
    }

    fn body(response: &web_sys::Response) -> HttpBody {
        match response.body() {
            Some(raw) => HttpBody::Stream(Box::pin(
                wasm_streams::ReadableStream::from_raw(raw)
                    .into_stream()
                    .map(|chunk| {
                        chunk
                            .map(|value| Bytes::from(Uint8Array::new(&value).to_vec()))
                            .map_err(transport)
                    }),
            )),
            None => HttpBody::Bytes(Bytes::new()),
        }
    }
}

impl OutboundClient for FetchClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let response = self.call(self.request(&parts, Some(body))?).await?;
            let (status, headers) = Self::head(&response)?;
            Ok(WireResponse {
                status,
                headers,
                body: Self::body(&response),
            })
        })
    }

    #[cfg(feature = "workers")]
    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            let (mut parts, ()) = request.into_parts();
            // Workers upgrades over the http(s) origin; the handshake headers
            // belong to the runtime, the rest (authentication) pass through.
            let uri = parts.uri.to_string();
            let uri = uri
                .replacen("wss://", "https://", 1)
                .replacen("ws://", "http://", 1);
            parts.uri = uri.parse().map_err(|_| invalid("invalid WebSocket URL"))?;
            let protocols = workers::split_handshake_headers(&mut parts.headers);
            parts
                .headers
                .insert(http::header::UPGRADE, HeaderValue::from_static("websocket"));
            if !protocols.is_empty()
                && let Ok(value) = HeaderValue::from_str(&protocols.join(", "))
            {
                parts
                    .headers
                    .insert(http::header::SEC_WEBSOCKET_PROTOCOL, value);
            }
            let response = self.call(self.request(&parts, None)?).await?;
            let (status, headers) = Self::head(&response)?;
            if status != StatusCode::SWITCHING_PROTOCOLS {
                return Ok(UpstreamConnection::Rejected(WireResponse {
                    status,
                    headers,
                    body: Self::body(&response),
                }));
            }
            let socket = Reflect::get(&response, &"webSocket".into())
                .ok()
                .and_then(|s| s.dyn_into::<web_sys::WebSocket>().ok())
                .ok_or_else(|| invalid("101 response carries no webSocket"))?;
            Ok(UpstreamConnection::Connected {
                handshake: WireResponse {
                    status,
                    headers,
                    body: (),
                },
                socket: workers::accept(socket)?,
            })
        })
    }

    #[cfg(not(feature = "workers"))]
    fn connect<'a>(
        &'a self,
        _request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                "WebSocket upgrades through fetch need the `workers` feature or a host client",
            ))
        })
    }
}

#[cfg(feature = "workers")]
mod workers {
    use super::*;
    use gproxy_protocol::connection::{WebSocket, WsClose, WsFrame};
    use wasm_bindgen::closure::Closure;

    pub(super) fn split_handshake_headers(headers: &mut HeaderMap) -> Vec<String> {
        let protocols = headers
            .get_all(http::header::SEC_WEBSOCKET_PROTOCOL)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        for name in [
            http::header::UPGRADE,
            http::header::CONNECTION,
            http::header::SEC_WEBSOCKET_KEY,
            http::header::SEC_WEBSOCKET_VERSION,
            http::header::SEC_WEBSOCKET_EXTENSIONS,
            http::header::SEC_WEBSOCKET_PROTOCOL,
        ] {
            headers.remove(name);
        }
        protocols
    }

    /// Event listeners stay registered while the receiving stream lives.
    struct Listeners {
        socket: web_sys::WebSocket,
        _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
        _close: Closure<dyn FnMut(web_sys::CloseEvent)>,
        _error: Closure<dyn FnMut(web_sys::Event)>,
    }
    impl Drop for Listeners {
        fn drop(&mut self) {
            self.socket.set_onmessage(None);
            self.socket.set_onclose(None);
            self.socket.set_onerror(None);
        }
    }

    /// `accept()` the Workers-side socket, then expose it as the protocol's
    /// independent incoming stream and outgoing sink.
    pub(super) fn accept(socket: web_sys::WebSocket) -> Result<WebSocket, CapabilityError> {
        let accept = Reflect::get(&socket, &"accept".into())
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .ok_or_else(|| invalid("webSocket has no accept()"))?;
        accept.call0(&socket).map_err(start_error)?;
        socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<WsFrame, TransportError>>();
        let message = {
            let tx = tx.clone();
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
                let data = event.data();
                let frame = if let Some(text) = data.as_string() {
                    Ok(WsFrame::Text(text))
                } else if let Ok(buffer) = data.clone().dyn_into::<js_sys::ArrayBuffer>() {
                    Ok(WsFrame::Binary(Bytes::from(
                        Uint8Array::new(&buffer).to_vec(),
                    )))
                } else {
                    Err(Box::new(std::io::Error::other(
                        "unsupported WebSocket message payload",
                    )) as TransportError)
                };
                let _ = tx.send(frame);
            })
        };
        let close = {
            let tx = tx.clone();
            Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |event: web_sys::CloseEvent| {
                let _ = tx.send(Ok(WsFrame::Close(Some(WsClose {
                    code: event.code(),
                    reason: event.reason(),
                }))));
            })
        };
        let error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            let _ = tx.send(Err(
                Box::new(std::io::Error::other("WebSocket error")) as TransportError
            ));
        });
        socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
        socket.set_onclose(Some(close.as_ref().unchecked_ref()));
        socket.set_onerror(Some(error.as_ref().unchecked_ref()));
        let listeners = Listeners {
            socket: socket.clone(),
            _message: message,
            _close: close,
            _error: error,
        };

        let incoming =
            futures_util::stream::unfold((rx, listeners), |(mut rx, listeners)| async move {
                match rx.recv().await {
                    Some(item) => {
                        let done = matches!(item, Ok(WsFrame::Close(_)) | Err(_));
                        if done {
                            rx.close();
                        }
                        Some((item, (rx, listeners)))
                    }
                    None => None,
                }
            });
        let outgoing = futures_util::sink::unfold(socket, |socket, frame: WsFrame| async move {
            let result = match &frame {
                WsFrame::Text(text) => socket.send_with_str(text),
                WsFrame::Binary(bytes) => socket.send_with_u8_array(bytes),
                // The JS API has no ping/pong surface; the runtime answers pings.
                WsFrame::Ping(_) | WsFrame::Pong(_) => Ok(()),
                WsFrame::Close(Some(close)) => {
                    socket.close_with_code_and_reason(close.code, &close.reason)
                }
                WsFrame::Close(None) => socket.close(),
            };
            result.map(|()| socket).map_err(transport)
        });
        Ok(WebSocket {
            incoming: Box::pin(incoming),
            outgoing: Box::pin(outgoing),
        })
    }
}
