use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, TransportError, WsDuplex, WsFrame};
use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
class GproxySocket {
  constructor(socket) {
    this.socket = socket;
    this.queue = [];
    this.waiter = null;
    this.closed = false;
    this.sequence = Promise.resolve();
    if ("binaryType" in socket) socket.binaryType = "arraybuffer";

    socket.addEventListener("message", (event) => {
      this.sequence = this.sequence.then(async () => {
        if (typeof event.data === "string") {
          this.push(["text", event.data]);
        } else if (event.data instanceof ArrayBuffer) {
          this.push(["binary", new Uint8Array(event.data)]);
        } else if (ArrayBuffer.isView(event.data)) {
          this.push(["binary", new Uint8Array(
            event.data.buffer, event.data.byteOffset, event.data.byteLength
          )]);
        } else if (typeof event.data?.arrayBuffer === "function") {
          this.push(["binary", new Uint8Array(await event.data.arrayBuffer())]);
        } else {
          this.push(["error", null]);
        }
      });
    });
    socket.addEventListener("error", () => {
      this.sequence = this.sequence.then(() => this.push(["error", null]));
    });
    socket.addEventListener("close", (event) => {
      this.sequence = this.sequence.then(() => {
        this.closed = true;
        this.push(["close", event.code || null]);
      });
    });
  }

  push(frame) {
    if (this.waiter !== null) {
      const waiter = this.waiter;
      this.waiter = null;
      waiter(frame);
    } else {
      this.queue.push(frame);
    }
  }

  recv() {
    if (this.queue.length > 0) return Promise.resolve(this.queue.shift());
    if (this.closed) return Promise.resolve(null);
    return new Promise((resolve) => { this.waiter = resolve; });
  }

  send(kind, value) {
    if (kind === "text" || kind === "binary") {
      this.socket.send(value);
    } else if (value === null) {
      this.socket.close();
    } else {
      this.socket.close(value);
    }
  }
}

export async function gproxyOpenSocket(url, headerEntries) {
  const headers = new Headers();
  for (const pair of headerEntries) headers.append(pair[0], pair[1]);
  headers.set("Upgrade", "websocket");
  const response = await globalThis.fetch(url, { method: "GET", headers });
  if (!response.webSocket) {
    throw new Error(`websocket upgrade failed with status ${response.status}`);
  }
  if (typeof response.webSocket.accept === "function") response.webSocket.accept();
  return new GproxySocket(response.webSocket);
}

export function gproxySocketSend(socket, kind, value) {
  socket.send(kind, value);
}

export function gproxySocketRecv(socket) {
  return socket.recv();
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = gproxyOpenSocket)]
    async fn open_socket(url: String, headers: Array) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = gproxySocketSend)]
    fn socket_send(socket: &JsValue, kind: &str, value: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = gproxySocketRecv)]
    fn socket_recv(socket: &JsValue) -> Promise;
}

pub(crate) struct WasmSocket {
    handle: JsValue,
}

impl WasmSocket {
    pub(crate) async fn open(
        url: String,
        headers: http::HeaderMap,
    ) -> Result<Box<dyn WsDuplex>, TransportError> {
        let entries = Array::new();
        for (name, value) in &headers {
            let value = value.to_str().map_err(|_| {
                TransportError::Connect(format!(
                    "websocket header `{name}` cannot be represented by fetch"
                ))
            })?;
            let pair = Array::new();
            pair.push(&JsValue::from_str(name.as_str()));
            pair.push(&JsValue::from_str(value));
            entries.push(&pair);
        }
        let handle = open_socket(url, entries)
            .await
            .map_err(|_| TransportError::Connect("websocket connection failed".into()))?;
        Ok(Box::new(Self { handle }))
    }
}

impl WsDuplex for WasmSocket {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        Box::pin(async move {
            match frame {
                WsFrame::Text(text) => socket_send(&self.handle, "text", &JsValue::from_str(&text)),
                WsFrame::Binary(bytes) => {
                    let bytes = Uint8Array::from(bytes.as_ref());
                    socket_send(&self.handle, "binary", bytes.as_ref())
                }
                WsFrame::Close(code) => {
                    let code = code.map_or(JsValue::NULL, |code| JsValue::from_f64(code.into()));
                    socket_send(&self.handle, "close", &code)
                }
            }
            .map_err(|_| TransportError::Interrupted("websocket send failed".into()))
        })
    }

    fn recv<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<WsFrame>, TransportError>> {
        Box::pin(async move {
            let value = JsFuture::from(socket_recv(&self.handle))
                .await
                .map_err(|_| TransportError::Interrupted("websocket receive failed".into()))?;
            if value.is_null() || value.is_undefined() {
                return Ok(None);
            }
            let frame: Array = value.dyn_into().map_err(|_| {
                TransportError::Interrupted("websocket yielded an invalid frame".into())
            })?;
            let kind = frame.get(0).as_string().ok_or_else(|| {
                TransportError::Interrupted("websocket frame kind is invalid".into())
            })?;
            match kind.as_str() {
                "text" => frame
                    .get(1)
                    .as_string()
                    .map(WsFrame::Text)
                    .map(Some)
                    .ok_or_else(|| {
                        TransportError::Interrupted("websocket text frame is invalid".into())
                    }),
                "binary" => {
                    let bytes: Uint8Array = frame.get(1).dyn_into().map_err(|_| {
                        TransportError::Interrupted("websocket binary frame is invalid".into())
                    })?;
                    Ok(Some(WsFrame::Binary(Bytes::from(bytes.to_vec()))))
                }
                "close" => Ok(Some(WsFrame::Close(
                    frame.get(1).as_f64().map(|code| code as u16),
                ))),
                "error" => Err(TransportError::Interrupted("websocket error".into())),
                _ => Err(TransportError::Interrupted(
                    "websocket frame kind is unknown".into(),
                )),
            }
        })
    }
}
