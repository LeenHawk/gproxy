mod js;
mod pump;

use bytes::Bytes;
use gproxy_channel_api::WsFrame;
use http::HeaderMap;
use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, future_to_promise};
use web_sys::{Request, Response};

use crate::edge::EdgeReply;

pub(crate) struct PreparedWebSocket {
    response: Response,
    downstream: DownstreamSocket,
}

pub(crate) fn prepare(request: &Request) -> Result<Option<PreparedWebSocket>, JsValue> {
    let value = js::prepare(request)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let pair: Array = value.dyn_into()?;
    Ok(Some(PreparedWebSocket {
        response: pair.get(0).dyn_into()?,
        downstream: DownstreamSocket {
            handle: pair.get(1),
            pending_recv: None,
        },
    }))
}

impl PreparedWebSocket {
    pub(crate) fn close(mut self, code: Option<u16>) {
        let _ = self.downstream.send(WsFrame::Close(code));
    }

    pub(crate) async fn start(
        mut self,
        mut upstream: Box<dyn gproxy_channel_api::WsDuplex>,
        headers: &HeaderMap,
    ) -> Result<EdgeReply, JsValue> {
        if let Err(error) = append_headers(&self.response, headers) {
            let _ = self.downstream.send(WsFrame::Close(None));
            let _ = upstream.send(WsFrame::Close(None)).await;
            return Err(error);
        }
        let response = self.response;
        let continuation = future_to_promise(async move {
            pump::run(self.downstream, upstream).await;
            Ok(JsValue::UNDEFINED)
        });
        Ok(EdgeReply::websocket(response, continuation))
    }
}

pub(super) struct DownstreamSocket {
    handle: JsValue,
    pending_recv: Option<Promise>,
}

impl DownstreamSocket {
    pub(super) fn send(&mut self, frame: WsFrame) -> Result<(), JsValue> {
        match frame {
            WsFrame::Text(text) => js::send(&self.handle, "text", &JsValue::from_str(&text)),
            WsFrame::Binary(bytes) => {
                let bytes = Uint8Array::from(bytes.as_ref());
                js::send(&self.handle, "binary", bytes.as_ref())
            }
            WsFrame::Close(code) => {
                let value = code.map_or(JsValue::NULL, |code| JsValue::from_f64(code.into()));
                js::send(&self.handle, "close", &value)
            }
        }
    }

    pub(super) async fn recv(&mut self) -> Result<Option<WsFrame>, JsValue> {
        let promise = self
            .pending_recv
            .get_or_insert_with(|| js::recv(&self.handle))
            .clone();
        let value = JsFuture::from(promise).await;
        self.pending_recv = None;
        js::ack(&self.handle);
        decode(value?)
    }
}

fn decode(value: JsValue) -> Result<Option<WsFrame>, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let frame: Array = value.dyn_into()?;
    let kind = frame
        .get(0)
        .as_string()
        .ok_or_else(|| JsValue::from_str("invalid websocket frame"))?;
    match kind.as_str() {
        "text" => frame
            .get(1)
            .as_string()
            .map(WsFrame::Text)
            .map(Some)
            .ok_or_else(|| JsValue::from_str("invalid websocket text frame")),
        "binary" => {
            let bytes: Uint8Array = frame.get(1).dyn_into()?;
            Ok(Some(WsFrame::Binary(Bytes::from(bytes.to_vec()))))
        }
        "close" => Ok(Some(WsFrame::Close(
            frame.get(1).as_f64().map(|code| code as u16),
        ))),
        "error" => Err(JsValue::from_str("downstream websocket error")),
        _ => Err(JsValue::from_str("unknown websocket frame")),
    }
}

fn append_headers(response: &Response, headers: &HeaderMap) -> Result<(), JsValue> {
    for (name, value) in headers {
        response.headers().append(
            name.as_str(),
            value
                .to_str()
                .map_err(|_| JsValue::from_str("websocket header is not representable by fetch"))?,
        )?;
    }
    Ok(())
}
