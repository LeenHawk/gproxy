use js_sys::{Uint8Array, global};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response, WorkerGlobalScope};

use super::{HttpFuture, HttpSender};
use crate::StoreError;

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
            let scope = global()
                .dyn_into::<WorkerGlobalScope>()
                .map_err(|_| StoreError::Database("libSQL fetch requires a worker scope".into()))?;
            let response = JsFuture::from(scope.fetch_with_request(&request))
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
