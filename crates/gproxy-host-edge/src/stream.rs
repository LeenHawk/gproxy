use std::panic::AssertUnwindSafe;
use std::rc::Rc;

use futures_util::StreamExt;
use futures_util::lock::Mutex;
use gproxy_core::ByteStream;
use js_sys::{Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};
use web_sys::{ReadableStream, ReadableStreamDefaultController};

type StreamState = Rc<Mutex<Option<ByteStream>>>;

pub(crate) struct StreamBody {
    readable: ReadableStream,
    state: StreamState,
}

impl StreamBody {
    pub(crate) fn new(stream: ByteStream) -> Result<Self, JsValue> {
        let state = Rc::new(Mutex::new(Some(stream)));
        let source = Object::new();

        let pull_state = state.clone();
        let pull = Closure::<dyn FnMut(ReadableStreamDefaultController) -> Promise>::new(
            move |controller| {
                future_to_promise(AssertUnwindSafe(pull_once(pull_state.clone(), controller)))
            },
        )
        .into_js_value();
        let cancel_state = state.clone();
        let cancel = Closure::<dyn FnMut(JsValue) -> Promise>::new(move |_| {
            let state = cancel_state.clone();
            future_to_promise(AssertUnwindSafe(async move {
                drain_state(state).await;
                Ok(JsValue::UNDEFINED)
            }))
        })
        .into_js_value();

        let readable = Reflect::set(&source, &JsValue::from_str("pull"), &pull)
            .and_then(|_| Reflect::set(&source, &JsValue::from_str("cancel"), &cancel))
            .and_then(|_| ReadableStream::new_with_underlying_source(&source));
        match readable {
            Ok(readable) => Ok(Self { readable, state }),
            Err(error) => {
                spawn_local(drain_state(state));
                Err(error)
            }
        }
    }

    pub(crate) fn readable(&self) -> ReadableStream {
        self.readable.clone()
    }

    pub(crate) async fn drain(&self) {
        drain_state(self.state.clone()).await;
    }
}

async fn pull_once(
    state: StreamState,
    controller: ReadableStreamDefaultController,
) -> Result<JsValue, JsValue> {
    let mut slot = state.lock().await;
    let Some(mut stream) = slot.take() else {
        controller.close()?;
        return Ok(JsValue::UNDEFINED);
    };
    match stream.next().await {
        Some(Ok(bytes)) => {
            let Ok(length) = u32::try_from(bytes.len()) else {
                let error = JsValue::from_str("response stream chunk exceeds Uint8Array capacity");
                controller.error_with_e(&error);
                drain_stream(stream).await;
                return Ok(JsValue::UNDEFINED);
            };
            let chunk = Uint8Array::new_with_length(length);
            chunk.copy_from(&bytes);
            if let Err(error) = controller.enqueue_with_chunk(&chunk.into()) {
                drain_stream(stream).await;
                return Err(error);
            }
            *slot = Some(stream);
        }
        Some(Err(error)) => {
            controller.error_with_e(&JsValue::from_str(&error.to_string()));
            drain_stream(stream).await;
        }
        None => controller.close()?,
    }
    Ok(JsValue::UNDEFINED)
}

async fn drain_state(state: StreamState) {
    let mut slot = state.lock().await;
    if let Some(stream) = slot.take() {
        drain_stream(stream).await;
    }
}

pub(crate) async fn drain_stream(mut stream: ByteStream) {
    while stream.next().await.is_some() {}
}
