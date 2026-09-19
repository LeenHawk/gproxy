//! One `http::Request` per claude.ai step (v3 `claudeweb/prepare.rs` and
//! `endpoint.rs`). Paths and methods:
//!
//! | step         | method | path                                                          |
//! |--------------|--------|---------------------------------------------------------------|
//! | upload       | POST   | `/api/{org}/upload` (multipart `file`)                        |
//! | create       | POST   | `/api/organizations/{org}/chat_conversations`                 |
//! | settings     | PUT    | `/api/organizations/{org}/chat_conversations/{conv}`          |
//! | completion   | POST   | `.../chat_conversations/{conv}/completion` (SSE)              |
//! | tool_result  | POST   | `.../chat_conversations/{conv}/tool_result`                   |
//! | cleanup      | DELETE | `/api/organizations/{org}/chat_conversations/{conv}`          |
//! | bootstrap    | GET    | `/api/bootstrap`                                              |
//! | usage        | GET    | `/api/organizations/{org}/usage`                              |
//!
//! Each step can be redirected through `config.endpoints.<key>` with
//! `{organization}` and `{conversation}` placeholders, as v3 allowed.

use gproxy_protocol::{HttpBody, connection::Bytes};
use http::{
    HeaderMap, HeaderValue, Method,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde_json::{Value, json};

use super::{
    ClaudeWebConfig,
    auth::{Auth, browser_headers, json_accept},
    request::Upload,
};
use crate::channel::ChannelError;

pub(super) const UPLOAD: &str = "claudeweb_upload";
pub(super) const CREATE: &str = "claudeweb_conversation_create";
pub(super) const SETTINGS: &str = "claudeweb_conversation_settings";
pub(super) const COMPLETION: &str = "claudeweb_completion";
pub(super) const TOOL_RESULT: &str = "claudeweb_tool_result";
pub(super) const CLEANUP: &str = "claudeweb_cleanup";
pub(super) const BOOTSTRAP: &str = "claudeweb_bootstrap";
pub(super) const USAGE: &str = "claudeweb_usage";

/// Everything needed to address one conversation, owned so it can outlive
/// the operation call inside the returned stream.
#[derive(Clone)]
pub(super) struct Requests {
    auth: Auth,
    base: String,
    endpoints: std::collections::BTreeMap<String, String>,
    /// Client headers that survived the allow-list plus static config headers.
    headers: HeaderMap,
    /// The host's complete method URL for the operation; used for the
    /// completion step only, the other steps have no client-visible URL.
    completion_override: Option<String>,
}

impl Requests {
    pub(super) fn new(
        auth: Auth,
        base: String,
        config: &ClaudeWebConfig,
        headers: HeaderMap,
        completion_override: Option<&str>,
    ) -> Self {
        Self {
            auth,
            base,
            endpoints: config.endpoints.clone(),
            headers,
            completion_override: completion_override.map(str::to_owned),
        }
    }

    pub(super) fn upload(
        &self,
        conversation: &str,
        upload: &Upload,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let boundary = format!("----gproxy{}", super::id::fresh("upload")?);
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
                upload.file_name
            )
            .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", upload.media_type).as_bytes());
        body.extend_from_slice(&upload.bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let path = format!("/api/{}/upload", self.auth.organization);
        let mut request = self.request(Method::POST, UPLOAD, &path, conversation, body)?;
        request.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
                .map_err(|error| ChannelError::InvalidConfig(format!("multipart: {error}")))?,
        );
        json_accept(request.headers_mut());
        Ok(request)
    }

    pub(super) fn create(
        &self,
        conversation: &str,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let path = format!(
            "/api/organizations/{}/chat_conversations",
            self.auth.organization
        );
        self.json(
            Method::POST,
            CREATE,
            &path,
            conversation,
            &json!({"uuid": conversation, "name": "", "is_temporary": true}),
        )
    }

    /// `paprika_mode: extended` switches extended thinking on; claude.ai
    /// only honours it for paid organizations.
    pub(super) fn settings(
        &self,
        conversation: &str,
        extended: bool,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let path = conversation_path(&self.auth.organization, conversation);
        let mode = if extended && self.auth.pro {
            Value::from("extended")
        } else {
            Value::Null
        };
        self.json(
            Method::PUT,
            SETTINGS,
            &path,
            conversation,
            &json!({"settings": {"paprika_mode": mode}}),
        )
    }

    pub(super) fn completion(
        &self,
        conversation: &str,
        body: &Value,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let path = format!(
            "{}/completion",
            conversation_path(&self.auth.organization, conversation)
        );
        let mut request = match &self.completion_override {
            Some(url) => {
                let url = substitute(url, &self.auth.organization, conversation);
                self.build(Method::POST, &url, conversation, Bytes::new())?
            }
            None => self.request(Method::POST, COMPLETION, &path, conversation, Vec::new())?,
        };
        *request.body_mut() = HttpBody::Bytes(Bytes::from(
            serde_json::to_vec(body).map_err(|e| ChannelError::InvalidConfig(e.to_string()))?,
        ));
        request
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        request
            .headers_mut()
            .insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        Ok(request)
    }

    pub(super) fn tool_result(
        &self,
        conversation: &str,
        body: &Value,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let path = format!(
            "{}/tool_result",
            conversation_path(&self.auth.organization, conversation)
        );
        self.json(Method::POST, TOOL_RESULT, &path, conversation, body)
    }

    pub(super) fn cleanup(
        &self,
        conversation: &str,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let path = conversation_path(&self.auth.organization, conversation);
        self.request(Method::DELETE, CLEANUP, &path, conversation, Vec::new())
    }

    fn json(
        &self,
        method: Method,
        key: &str,
        path: &str,
        conversation: &str,
        value: &Value,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let body =
            serde_json::to_vec(value).map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
        let mut request = self.request(method, key, path, conversation, body)?;
        request
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(request)
    }

    fn request(
        &self,
        method: Method,
        key: &str,
        path: &str,
        conversation: &str,
        body: Vec<u8>,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let url = endpoint_url(
            &self.endpoints,
            key,
            &self.base,
            path,
            &self.auth.organization,
            conversation,
        );
        self.build(method, &url, conversation, Bytes::from(body))
    }

    /// Client headers first, then the browser identity on top so nothing
    /// the client sent can override the session's own headers.
    fn build(
        &self,
        method: Method,
        url: &str,
        conversation: &str,
        body: Bytes,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let referer = format!("{}/chat/{conversation}", self.base);
        let mut headers = self.headers.clone();
        headers.extend(browser_headers(
            &self.auth.cookie,
            self.auth.device_id.as_deref(),
            &self.base,
            &referer,
        )?);
        let mut builder = http::Request::builder().method(method).uri(url);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(HttpBody::Bytes(body))
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

pub(super) fn conversation_path(organization: &str, conversation: &str) -> String {
    format!("/api/organizations/{organization}/chat_conversations/{conversation}")
}

/// `config.endpoints.<key>` wins over `base + path`.
pub(super) fn endpoint_url(
    endpoints: &std::collections::BTreeMap<String, String>,
    key: &str,
    base: &str,
    path: &str,
    organization: &str,
    conversation: &str,
) -> String {
    match endpoints
        .get(key)
        .map(|url| url.trim())
        .filter(|url| !url.is_empty())
    {
        Some(url) => substitute(url, organization, conversation),
        None => format!("{base}{path}"),
    }
}

fn substitute(url: &str, organization: &str, conversation: &str) -> String {
    url.replace("{organization}", &encode_component(organization))
        .replace("{conversation}", &encode_component(conversation))
}

fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

/// A bare browser call outside any conversation (bootstrap, usage).
pub(super) fn session_get(
    url: &str,
    cookie: &str,
    device_id: Option<&str>,
    base: &str,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let mut headers = browser_headers(cookie, device_id, base, &format!("{base}/new"))?;
    json_accept(&mut headers);
    let mut builder = http::Request::builder().method(Method::GET).uri(url);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    builder
        .body(HttpBody::Bytes(Bytes::new()))
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
}
