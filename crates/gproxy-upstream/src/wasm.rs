use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{
    BoxFuture, ByteStream, ChannelError, SimpleHttp, TransportError, WsDuplex,
};
use gproxy_core::UpstreamTransport;
use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

#[wasm_bindgen(inline_js = r#"
export function gproxyFetch(request) {
  return globalThis.fetch(request);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = gproxyFetch)]
    fn gproxy_fetch(request: &Request) -> Promise;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FetchTransport;

impl FetchTransport {
    pub const fn new() -> Self {
        Self
    }
}

impl UpstreamTransport for FetchTransport {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<ByteStream>, TransportError>> {
        Box::pin(fetch_response(request))
    }

    fn open_websocket<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>> {
        let (parts, _) = request.into_parts();
        Box::pin(crate::wasm_socket::WasmSocket::open(
            parts.uri.to_string(),
            parts.headers,
        ))
    }
}

impl SimpleHttp for FetchTransport {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
        crate::buffered::send(self, request)
    }
}

async fn fetch_response(
    request: http::Request<Bytes>,
) -> Result<http::Response<ByteStream>, TransportError> {
    let (parts, body) = request.into_parts();
    let headers = request_headers(&parts.headers)?;
    let init = RequestInit::new();
    init.set_method(parts.method.as_str());
    init.set_headers_headers(&headers);
    if parts.method != http::Method::GET && parts.method != http::Method::HEAD && !body.is_empty() {
        let body = Uint8Array::from(body.as_ref());
        init.set_body_opt_u8_array(Some(&body));
    }

    let request = Request::new_with_str_and_init(&parts.uri.to_string(), &init)
        .map_err(|_| TransportError::Connect("invalid upstream fetch request".into()))?;
    let value = JsFuture::from(gproxy_fetch(&request))
        .await
        .map_err(|_| TransportError::Connect("fetch request failed".into()))?;
    let response: Response = value
        .dyn_into()
        .map_err(|_| TransportError::Interrupted("fetch returned a non-response".into()))?;
    let status = http::StatusCode::from_u16(response.status())
        .map_err(|_| TransportError::Interrupted("fetch returned an invalid status".into()))?;
    let headers = response_headers(&response)?;
    let stream: ByteStream = match response.body() {
        Some(body) => Box::pin(
            wasm_streams::ReadableStream::from_raw(body)
                .into_stream()
                .map(|chunk| {
                    let value = chunk.map_err(|_| {
                        TransportError::Interrupted("fetch response stream failed".into())
                    })?;
                    let bytes: Uint8Array = value.dyn_into().map_err(|_| {
                        TransportError::Interrupted(
                            "fetch response stream yielded a non-byte chunk".into(),
                        )
                    })?;
                    Ok(Bytes::from(bytes.to_vec()))
                }),
        ),
        None => Box::pin(futures_util::stream::empty()),
    };

    let mut output = http::Response::new(stream);
    *output.status_mut() = status;
    *output.headers_mut() = headers;
    Ok(output)
}

fn request_headers(source: &http::HeaderMap) -> Result<Headers, TransportError> {
    let headers =
        Headers::new().map_err(|_| TransportError::Connect("fetch headers unavailable".into()))?;
    for (name, value) in source {
        let value = value.to_str().map_err(|_| {
            TransportError::Connect(format!(
                "request header `{name}` cannot be represented by fetch"
            ))
        })?;
        headers
            .append(name.as_str(), value)
            .map_err(|_| TransportError::Connect(format!("fetch rejected header `{name}`")))?;
    }
    Ok(headers)
}

fn response_headers(response: &Response) -> Result<http::HeaderMap, TransportError> {
    let mut headers = http::HeaderMap::new();
    let entries = js_sys::try_iter(&response.headers())
        .map_err(|_| TransportError::Interrupted("fetch response headers are not iterable".into()))?
        .ok_or_else(|| {
            TransportError::Interrupted("fetch response headers are not iterable".into())
        })?;
    for entry in entries {
        let entry = entry
            .map_err(|_| TransportError::Interrupted("fetch response header failed".into()))?;
        let pair: Array = entry.dyn_into().map_err(|_| {
            TransportError::Interrupted("fetch response header is not a pair".into())
        })?;
        let name = pair.get(0).as_string().ok_or_else(|| {
            TransportError::Interrupted("fetch response header name is not text".into())
        })?;
        let value = pair.get(1).as_string().ok_or_else(|| {
            TransportError::Interrupted("fetch response header value is not text".into())
        })?;
        let name = http::header::HeaderName::try_from(name).map_err(|_| {
            TransportError::Interrupted("fetch response header name is invalid".into())
        })?;
        let value = http::header::HeaderValue::try_from(value).map_err(|_| {
            TransportError::Interrupted("fetch response header value is invalid".into())
        })?;
        headers.append(name, value);
    }
    Ok(headers)
}
