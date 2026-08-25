use js_sys::Promise;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
class GproxyDownstreamSocket {
  constructor(socket) {
    this.socket = socket;
    this.queue = [];
    this.pending = null;
    this.closed = false;
    this.sequence = Promise.resolve();
    if ("binaryType" in socket) socket.binaryType = "arraybuffer";
    socket.addEventListener("message", (event) => {
      this.sequence = this.sequence.then(async () => {
        if (typeof event.data === "string") this.push(["text", event.data]);
        else if (event.data instanceof ArrayBuffer) {
          this.push(["binary", new Uint8Array(event.data)]);
        } else if (ArrayBuffer.isView(event.data)) {
          this.push(["binary", new Uint8Array(
            event.data.buffer, event.data.byteOffset, event.data.byteLength
          )]);
        } else if (typeof event.data?.arrayBuffer === "function") {
          this.push(["binary", new Uint8Array(await event.data.arrayBuffer())]);
        } else this.push(["error", null]);
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
    if (this.pending !== null && this.pending.resolve !== null) {
      const resolve = this.pending.resolve;
      this.pending.resolve = null;
      resolve(frame);
    } else this.queue.push(frame);
  }
  recv() {
    if (this.pending !== null) return this.pending.promise;
    let resolve = null;
    let promise;
    if (this.queue.length > 0) promise = Promise.resolve(this.queue.shift());
    else if (this.closed) promise = Promise.resolve(null);
    else promise = new Promise((callback) => { resolve = callback; });
    this.pending = { promise, resolve };
    return promise;
  }
  ack() { this.pending = null; }
  send(kind, value) {
    if (kind === "text" || kind === "binary") this.socket.send(value);
    else if (value === null) this.socket.close();
    else this.socket.close(value);
  }
}

export function gproxyPrepareDownstream(request) {
  let response;
  let socket;
  if (typeof globalThis.WebSocketPair === "function") {
    const pair = new globalThis.WebSocketPair();
    const [client, server] = Object.values(pair);
    if (typeof server.accept === "function") server.accept();
    response = new Response(null, { status: 101, webSocket: client });
    socket = server;
  } else if (typeof globalThis.Deno?.upgradeWebSocket === "function") {
    ({ response, socket } = globalThis.Deno.upgradeWebSocket(request));
  } else return null;
  return [response, new GproxyDownstreamSocket(socket)];
}

export function gproxyDownstreamSend(socket, kind, value) {
  socket.send(kind, value);
}
export function gproxyDownstreamRecv(socket) { return socket.recv(); }
export function gproxyDownstreamAck(socket) { socket.ack(); }
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = gproxyPrepareDownstream)]
    pub(super) fn prepare(request: &web_sys::Request) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = gproxyDownstreamSend)]
    pub(super) fn send(socket: &JsValue, kind: &str, value: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = gproxyDownstreamRecv)]
    pub(super) fn recv(socket: &JsValue) -> Promise;

    #[wasm_bindgen(js_name = gproxyDownstreamAck)]
    pub(super) fn ack(socket: &JsValue);
}
