//! The WebAssembly module a JavaScript shell loads: the engine behind
//! `fetch(Request) -> Response`, with the platform's key-value store handed
//! in as an object.
//!
//! The store object has two methods. Values and versions are opaque strings.
//!
//! - `get(key)` resolves to `{ value, version }`, or `null` when absent.
//! - `compareExchange(key, expectedVersion, value, expiresAtMs)` replaces the
//!   value only when the stored version is `expectedVersion` (`null`: only
//!   when absent); a `null` value deletes. It resolves to `{ version }` when
//!   applied (`version` is `null` after a delete) and to `null` on conflict.
//!   `expiresAtMs` is when the engine stops reading the value; a store may
//!   use it to reclaim the key.

use std::sync::Arc;

use base64::Engine as _;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, StateEntry, StateWrite,
        Version,
    },
    connection::Bytes,
};
use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::{
    Engine, KeyValue,
    store::{LIMITS, storage},
};

#[wasm_bindgen]
extern "C" {
    pub type Store;

    #[wasm_bindgen(method, catch)]
    async fn get(this: &Store, key: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = compareExchange)]
    async fn compare_exchange(
        this: &Store,
        key: &str,
        expected: JsValue,
        value: JsValue,
        expires_at_ms: JsValue,
    ) -> Result<JsValue, JsValue>;
}

fn message(value: &JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            Reflect::get(value, &"message".into())
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| format!("{value:?}"))
}

fn store_error(value: JsValue) -> CapabilityError {
    storage(format!("the key-value store failed: {}", message(&value)))
}

fn field(object: &JsValue, name: &str) -> Result<Option<String>, CapabilityError> {
    Reflect::get(object, &name.into())
        .map(|value| value.as_string())
        .map_err(store_error)
}

fn version(text: String) -> Version {
    Version::from_bytes(text.into_bytes())
}

fn version_text(version: &Version) -> Result<JsValue, CapabilityError> {
    std::str::from_utf8(version.as_bytes())
        .map(JsValue::from_str)
        .map_err(|_| storage("a store version is not text"))
}

impl KeyValue for Store {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move {
            let entry = Store::get(self, key).await.map_err(store_error)?;
            if entry.is_null() || entry.is_undefined() {
                return Ok(None);
            }
            let (Some(value), Some(stored)) = (field(&entry, "value")?, field(&entry, "version")?)
            else {
                return Err(storage("the key-value store returned no value or version"));
            };
            let payload = base64::engine::general_purpose::STANDARD
                .decode(value)
                .map_err(|_| storage("a stored value is not base64"))?;
            Ok(Some(StateEntry {
                payload: Bytes::from(payload),
                version: version(stored),
                expires_at: None,
            }))
        })
    }

    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let expected = match &expected {
                Some(version) => version_text(version)?,
                None => JsValue::NULL,
            };
            let (value, expires_at_ms) = match &replacement {
                Some(write) => (
                    JsValue::from_str(
                        &base64::engine::general_purpose::STANDARD.encode(&write.payload),
                    ),
                    write.expires_at.map_or(JsValue::NULL, |at| {
                        let since = at
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap_or_default();
                        JsValue::from_f64(since.as_millis() as f64)
                    }),
                ),
                None => (JsValue::NULL, JsValue::NULL),
            };
            let applied = Store::compare_exchange(self, key, expected, value, expires_at_ms)
                .await
                .map_err(store_error)?;
            if applied.is_null() || applied.is_undefined() {
                return Ok(CasResult::Conflict);
            }
            Ok(CasResult::Applied(field(&applied, "version")?.map(version)))
        })
    }

    fn limits(&self) -> CapabilityLimits {
        LIMITS
    }
}

fn head(request: &web_sys::Request) -> Result<http::request::Builder, JsValue> {
    let uri: http::Uri = request
        .url()
        .parse()
        .map_err(|_| JsValue::from_str("the request URL is not valid"))?;
    let mut builder = http::Request::builder()
        .method(request.method().as_str())
        .uri(uri.path_and_query().map_or("/", |target| target.as_str()));
    let entries = js_sys::try_iter(request.headers().as_ref())?
        .ok_or_else(|| JsValue::from_str("request headers are not iterable"))?;
    for entry in entries {
        let pair = js_sys::Array::from(&entry?);
        if let (Some(name), Some(value)) = (pair.get(0).as_string(), pair.get(1).as_string()) {
            builder = builder.header(name, value);
        }
    }
    Ok(builder)
}

fn invalid(error: http::Error) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Headers that described the upstream's transfer, not this one: `fetch` has
/// already decoded the body, and the platform frames the response itself.
const TRANSFER_HEADERS: [http::HeaderName; 4] = [
    http::header::CONTENT_ENCODING,
    http::header::CONTENT_LENGTH,
    http::header::TRANSFER_ENCODING,
    http::header::CONNECTION,
];

fn response(wire: WireResponse<HttpBody>) -> Result<web_sys::Response, JsValue> {
    let headers = web_sys::Headers::new()?;
    for (name, value) in &wire.headers {
        if TRANSFER_HEADERS.contains(name) {
            continue;
        }
        if let Ok(value) = value.to_str() {
            headers.append(name.as_str(), value)?;
        }
    }
    let init = web_sys::ResponseInit::new();
    init.set_status(wire.status.as_u16());
    init.set_headers(&headers);
    match wire.body {
        HttpBody::Bytes(bytes) if bytes.is_empty() => {
            web_sys::Response::new_with_opt_str_and_init(None, &init)
        }
        HttpBody::Bytes(bytes) => web_sys::Response::new_with_opt_buffer_source_and_init(
            Some(&Uint8Array::from(&bytes[..])),
            &init,
        ),
        HttpBody::Stream(stream) => {
            let stream = stream.map(|chunk| {
                chunk
                    .map(|bytes| JsValue::from(Uint8Array::from(&bytes[..])))
                    .map_err(|error| JsValue::from_str(&error.to_string()))
            });
            web_sys::Response::new_with_opt_readable_stream_and_init(
                Some(&wasm_streams::ReadableStream::from_stream(stream).into_raw()),
                &init,
            )
        }
    }
}

#[wasm_bindgen]
pub struct Lite {
    engine: Engine,
}

#[wasm_bindgen]
impl Lite {
    /// `config` is the configuration JSON; `store` the platform's key-value
    /// store. Outbound requests go through the global `fetch`.
    #[wasm_bindgen(constructor)]
    pub fn new(config: &str, store: Store) -> Result<Lite, JsError> {
        let client =
            gproxy_client::FetchClient::new().map_err(|error| JsError::new(&error.to_string()))?;
        let engine = Engine::new(
            config,
            &gproxy_channel::channels::compiled_in(),
            Arc::new(store),
            Arc::new(client),
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(Lite { engine })
    }

    /// Answer one HTTP request.
    pub async fn fetch(&self, request: web_sys::Request) -> Result<web_sys::Response, JsValue> {
        let engine = self.engine.clone();
        let body = JsFuture::from(request.array_buffer()?).await?;
        let body = Bytes::from(Uint8Array::new(&body).to_vec());
        let request = head(&request)?
            .body(HttpBody::Bytes(body))
            .map_err(invalid)?;
        response(engine.handle(request).await)
    }

    /// Renew every credential that is due, for the platform's scheduler.
    /// Resolves to `[{ provider, outcome, reason? }]`.
    pub async fn renew(&self) -> Result<JsValue, JsValue> {
        let engine = self.engine.clone();
        let renewals = serde_json::to_string(&engine.renew_due().await)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        js_sys::JSON::parse(&renewals)
    }

    /// Serve a WebSocket handshake on a platform that hands the server end of
    /// the socket to the Worker (Cloudflare). Resolves to nothing once frames
    /// are being relayed, and to the refusal when the handshake is refused.
    pub async fn websocket(
        &self,
        request: web_sys::Request,
        socket: web_sys::WebSocket,
    ) -> Result<Option<web_sys::Response>, JsValue> {
        let engine = self.engine.clone();
        let request = head(&request)?.body(()).map_err(invalid)?;
        match engine.connect(request).await {
            Ok(upstream) => {
                let client = gproxy_client::accept_workers_websocket(socket)
                    .map_err(|error| JsValue::from_str(&error.to_string()))?;
                wasm_bindgen_futures::spawn_local(crate::relay(client, upstream));
                Ok(None)
            }
            Err(refused) => response(*refused).map(Some),
        }
    }
}
