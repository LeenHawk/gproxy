use js_sys::{Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

use gproxy_core::error::StoreError;

use super::super::error;

#[wasm_bindgen(
    inline_js = "export function gproxyUpstashFetch(request) { return globalThis.fetch(request); }"
)]
extern "C" {
    #[wasm_bindgen(js_name = gproxyUpstashFetch)]
    fn fetch(request: &Request) -> Promise;
}

pub(super) struct WasmSender;

impl WasmSender {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) async fn post(
        &self,
        url: &str,
        token: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, StoreError> {
        let headers = Headers::new().map_err(|_| error("Upstash", "request"))?;
        headers
            .append("content-type", "application/json")
            .map_err(|_| error("Upstash", "request"))?;
        headers
            .append("authorization", &format!("Bearer {token}"))
            .map_err(|_| error("Upstash", "request"))?;
        let body = Uint8Array::from(body.as_slice());
        let init = RequestInit::new();
        init.set_method("POST");
        init.set_headers_headers(&headers);
        init.set_body_opt_u8_array(Some(&body));
        let request =
            Request::new_with_str_and_init(url, &init).map_err(|_| error("Upstash", "request"))?;
        let response = JsFuture::from(fetch(&request))
            .await
            .map_err(|_| error("Upstash", "request"))?
            .dyn_into::<Response>()
            .map_err(|_| error("Upstash", "response"))?;
        if !(200..300).contains(&response.status()) {
            return Err(error("Upstash", "request"));
        }
        let buffer = JsFuture::from(
            response
                .array_buffer()
                .map_err(|_| error("Upstash", "response"))?,
        )
        .await
        .map_err(|_| error("Upstash", "response"))?;
        Ok(Uint8Array::new(&buffer).to_vec())
    }
}
