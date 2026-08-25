use js_sys::{Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

use super::{HttpFuture, HttpSender};
use crate::StoreError;

#[wasm_bindgen(inline_js = r#"
export function gproxyLibsqlFetch(request) {
  return globalThis.fetch(request);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = gproxyLibsqlFetch)]
    fn gproxy_libsql_fetch(request: &Request) -> Promise;
}

pub(super) struct WasmSender;

impl HttpSender for WasmSender {
    fn post<'a>(&'a self, url: &'a str, auth_token: &'a str, body: Vec<u8>) -> HttpFuture<'a> {
        Box::pin(async move {
            let headers = Headers::new().map_err(js_error)?;
            headers
                .append("content-type", "application/json")
                .map_err(js_error)?;
            headers
                .append("authorization", &format!("Bearer {auth_token}"))
                .map_err(js_error)?;

            let body = Uint8Array::from(body.as_slice());
            let init = RequestInit::new();
            init.set_method("POST");
            init.set_headers_headers(&headers);
            init.set_body_opt_u8_array(Some(&body));
            let request = Request::new_with_str_and_init(url, &init).map_err(js_error)?;
            let response = JsFuture::from(gproxy_libsql_fetch(&request))
                .await
                .map_err(js_error)?
                .dyn_into::<Response>()
                .map_err(js_error)?;
            let status = response.status();
            if !(200..300).contains(&status) {
                return Err(StoreError::Database(format!("libSQL HTTP status {status}")));
            }
            let buffer = JsFuture::from(response.array_buffer().map_err(js_error)?)
                .await
                .map_err(js_error)?;
            Ok(Uint8Array::new(&buffer).to_vec())
        })
    }
}

fn js_error(value: wasm_bindgen::JsValue) -> StoreError {
    StoreError::Database(format!("libSQL fetch failed: {value:?}"))
}
