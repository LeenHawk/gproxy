use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PreparedRequest};
use http::header::{ACCEPT, CONTENT_TYPE};
use serde_json::Value;

use super::auth::Auth;

#[derive(Clone)]
pub(super) struct Requests {
    auth: Auth,
    base: String,
    settings: Value,
}

impl Requests {
    pub(super) fn new(secret: &Value, settings: &Value) -> Result<Self, ChannelError> {
        Ok(Self {
            auth: Auth::read(secret)?,
            base: super::auth::base(settings).into(),
            settings: settings.clone(),
        })
    }

    pub(super) fn is_pro(&self) -> bool {
        self.auth.pro
    }

    pub(super) fn upload(
        &self,
        conversation: &str,
        upload: &super::request::Upload,
    ) -> Result<PreparedRequest, ChannelError> {
        let boundary = format!("----gproxy{}", super::id::fresh("upload"));
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
        let mut request =
            self.request("claudeweb_upload", &path, conversation, Bytes::from(body))?;
        request.request.headers_mut().insert(
            CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}")
                .parse()
                .map_err(|error| ChannelError::Prepare(format!("multipart header: {error}")))?,
        );
        request
            .request
            .headers_mut()
            .insert(ACCEPT, "application/json".parse().expect("static"));
        Ok(request)
    }

    pub(super) fn create(&self, conversation: &str) -> Result<PreparedRequest, ChannelError> {
        let path = format!(
            "/api/organizations/{}/chat_conversations",
            self.auth.organization
        );
        self.json(
            http::Method::POST,
            "claudeweb_conversation_create",
            &path,
            conversation,
            &serde_json::json!({"uuid":conversation,"name":"","is_temporary":true}),
        )
    }

    pub(super) fn settings(
        &self,
        conversation: &str,
        extended: bool,
    ) -> Result<PreparedRequest, ChannelError> {
        let path = super::endpoint::conversation(&self.auth.organization, conversation);
        self.json(
            http::Method::PUT,
            "claudeweb_conversation_settings",
            &path,
            conversation,
            &serde_json::json!({
                "settings":{"paprika_mode":if extended && self.is_pro(){Value::from("extended")}else{Value::Null}}
            }),
        )
    }

    pub(super) fn completion(
        &self,
        conversation: &str,
        body: &Value,
    ) -> Result<PreparedRequest, ChannelError> {
        let path = format!(
            "{}/completion",
            super::endpoint::conversation(&self.auth.organization, conversation)
        );
        let mut request = self.json(
            http::Method::POST,
            "claudeweb_completion",
            &path,
            conversation,
            body,
        )?;
        request
            .request
            .headers_mut()
            .insert(ACCEPT, "text/event-stream".parse().expect("static"));
        Ok(request)
    }

    pub(super) fn tool_result(
        &self,
        conversation: &str,
        body: &Value,
    ) -> Result<PreparedRequest, ChannelError> {
        let path = format!(
            "{}/tool_result",
            super::endpoint::conversation(&self.auth.organization, conversation)
        );
        self.json(
            http::Method::POST,
            "claudeweb_tool_result",
            &path,
            conversation,
            body,
        )
    }

    pub(super) fn cleanup(&self, conversation: &str) -> Result<PreparedRequest, ChannelError> {
        let path = super::endpoint::conversation(&self.auth.organization, conversation);
        let mut request = self.request("claudeweb_cleanup", &path, conversation, Bytes::new())?;
        *request.request.method_mut() = http::Method::DELETE;
        Ok(request)
    }

    fn json(
        &self,
        method: http::Method,
        key: &str,
        path: &str,
        conversation: &str,
        value: &Value,
    ) -> Result<PreparedRequest, ChannelError> {
        let body = serde_json::to_vec(value)
            .map(Bytes::from)
            .map_err(|error| ChannelError::Prepare(error.to_string()))?;
        let mut request = self.request(key, path, conversation, body)?;
        *request.request.method_mut() = method;
        request
            .request
            .headers_mut()
            .insert(CONTENT_TYPE, "application/json".parse().expect("static"));
        Ok(request)
    }

    fn request(
        &self,
        key: &str,
        path: &str,
        conversation: &str,
        body: Bytes,
    ) -> Result<PreparedRequest, ChannelError> {
        let url = super::endpoint::url(
            &self.settings,
            &self.base,
            &self.auth.organization,
            conversation,
            key,
            path,
        )?;
        let referer = format!("{}/chat/{conversation}", self.base);
        let mut request = http::Request::post(url)
            .body(body)
            .map_err(|error| ChannelError::Prepare(error.to_string()))?;
        *request.headers_mut() = self.auth.headers(&self.base, &referer)?;
        Ok(PreparedRequest {
            request,
            framing: Some(gproxy_protocol::StreamFraming::Sse),
            websocket: false,
            profile: Some(&super::profile::CLIENT_PROFILE),
        })
    }
}
