use gproxy_core::RequestCtx;
use wasm_bindgen::prelude::*;
use web_sys::{Request, Response};

const ADMIN_PREFIX: &str = "/admin/api";
const PORTAL_API_PREFIX: &str = "/portal/api";

#[wasm_bindgen]
pub struct EdgeHost {
    app: gproxy_app::AppHandle,
}

#[wasm_bindgen]
pub struct EdgeConfig {
    libsql_url: String,
    libsql_auth_token: String,
    secret_key: Option<String>,
    secret_key_next: Option<String>,
    secret_key_rotate: bool,
    upstash_url: Option<String>,
    upstash_token: Option<String>,
}

#[wasm_bindgen]
impl EdgeConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(
        libsql_url: String,
        libsql_auth_token: String,
        secret_key: Option<String>,
        secret_key_next: Option<String>,
        secret_key_rotate: bool,
        upstash_url: Option<String>,
        upstash_token: Option<String>,
    ) -> Self {
        Self {
            libsql_url,
            libsql_auth_token,
            secret_key,
            secret_key_next,
            secret_key_rotate,
            upstash_url,
            upstash_token,
        }
    }
}

#[wasm_bindgen]
pub struct EdgeReply {
    response: Option<Response>,
    continuation: Option<js_sys::Promise>,
}

#[wasm_bindgen]
impl EdgeReply {
    #[wasm_bindgen(js_name = takeResponse)]
    pub fn take_response(&mut self) -> Option<Response> {
        self.response.take()
    }

    #[wasm_bindgen(getter)]
    pub fn continuation(&self) -> Option<js_sys::Promise> {
        self.continuation.clone()
    }
}

#[wasm_bindgen]
pub async fn start(config: EdgeConfig) -> Result<EdgeHost, JsValue> {
    let upstash = (config.upstash_url, config.upstash_token);
    let secret_keys = gproxy_app::MasterKeyConfig::from_encoded(
        config.secret_key,
        config.secret_key_next,
        config.secret_key_rotate,
    )
    .map_err(js_error)?;
    let mut config =
        gproxy_app::Config::libsql(config.libsql_url, config.libsql_auth_token, secret_keys)
            .map_err(js_error)?;
    match upstash {
        (Some(url), Some(token)) if !url.is_empty() && !token.is_empty() => {
            config = config.with_upstash(url, token).map_err(js_error)?;
        }
        (None, None) => {}
        _ => {
            return Err(JsValue::from_str(
                "UPSTASH_URL and UPSTASH_TOKEN must be set together",
            ));
        }
    }
    let app = gproxy_app::App::start(config).await.map_err(js_error)?;
    Ok(EdgeHost { app })
}

#[wasm_bindgen]
impl EdgeHost {
    pub async fn fetch(
        &self,
        request: Request,
        client_source: String,
    ) -> Result<EdgeReply, JsValue> {
        self.app.sync_invalidation().await.map_err(js_error)?;
        let request_id = request_id()?;
        let client_ip = client_source.parse().ok();
        let mut incoming = match crate::request::read(&request, client_source).await {
            Ok(incoming) => incoming,
            Err(error) => {
                return crate::response::local_error(error.status, error.message, &request_id)
                    .map(EdgeReply::from);
            }
        };
        if is_prefix(&incoming.path, ADMIN_PREFIX)
            && let Some(response) = self
                .app
                .admin_dispatch(&incoming.parts, incoming.body.clone())
                .await
        {
            return crate::response::buffered(response, &request_id).map(EdgeReply::from);
        }
        if is_prefix(&incoming.path, PORTAL_API_PREFIX)
            && let Some(response) = self
                .app
                .portal_dispatch(&incoming.parts, incoming.body.clone())
                .await
        {
            return crate::response::buffered(response, &request_id).map(EdgeReply::from);
        }
        incoming.body = match gproxy_app::ingress::decode_body(
            &mut incoming.parts.headers,
            incoming.body,
            crate::request::MAX_BODY_BYTES,
        ) {
            Ok(body) => body,
            Err(error) => {
                return crate::response::local_error(error.status, error.message, &request_id)
                    .map(EdgeReply::from);
            }
        };
        let websocket_intent = crate::request::has_websocket_intent(&incoming.parts.headers);
        let upgrade = if websocket_intent {
            let upgrade = match crate::websocket::prepare(&request) {
                Ok(Some(upgrade)) => upgrade,
                Ok(None) => {
                    return crate::response::local_error(
                        http::StatusCode::NOT_IMPLEMENTED,
                        "websocket upgrades are unavailable in this fetch runtime",
                        &request_id,
                    )
                    .map(EdgeReply::from);
                }
                Err(_) => {
                    return crate::response::local_error(
                        http::StatusCode::BAD_REQUEST,
                        "websocket upgrade failed",
                        &request_id,
                    )
                    .map(EdgeReply::from);
                }
            };
            Some(upgrade)
        } else {
            None
        };
        let (mode, path) = gproxy_app::ingress::normalize_path(
            &self.app,
            &incoming.method,
            &incoming.path,
            upgrade.is_some(),
        );
        let context = RequestCtx {
            request_id: request_id.clone(),
            client_ip,
            method: incoming.method.clone(),
            path,
            query: incoming.query,
            headers: incoming.parts.headers,
            body: incoming.body,
            upgrade: upgrade.is_some(),
            mode,
        };
        crate::response::outcome(
            incoming.method,
            self.app.execute(context).await,
            upgrade,
            &request_id,
        )
        .await
    }
}

impl From<Response> for EdgeReply {
    fn from(response: Response) -> Self {
        Self {
            response: Some(response),
            continuation: None,
        }
    }
}

impl EdgeReply {
    pub(crate) fn websocket(response: Response, continuation: js_sys::Promise) -> Self {
        Self {
            response: Some(response),
            continuation: Some(continuation),
        }
    }
}

fn is_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn request_id() -> Result<String, JsValue> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| JsValue::from_str("secure randomness unavailable"))?;
    let mut value = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(value)
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
