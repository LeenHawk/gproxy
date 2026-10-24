use super::{
    OrcaTerm, auth,
    config::{OrcaTermConfig, new_id},
    images,
};
use crate::channel::{
    BaseChannel, ChannelError, ChannelHeaders, HeaderAllowlist, PrepareContext, forwardable,
};
use crate::channels::shared::compatible::http::insert_configured;
use gproxy_protocol::HttpBody;
use http::{HeaderMap, HeaderValue, Method, header};
use serde_json::{Value, json};

pub const CLIENT_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["x-product", "x-seqid", "x-csrfcode", "x-referer"],
    prefixes: &[],
};
const OWNED_HEADERS: &[&str] = &[
    "cookie",
    "user-agent",
    "origin",
    "referer",
    "accept",
    "x-product",
    "x-seqid",
    "x-csrfcode",
    "x-referer",
];
// This is the literal identity in the 1.6.6 desktop HTTP interceptor, including
// its older product version and macOS platform on the Windows build.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) OrcaTerm/1.0.0 Chrome/120.0.0.0 Safari/537.36";

pub(super) fn insert(headers: &mut HeaderMap, name: &str, value: &str) -> Result<(), ChannelError> {
    insert_configured(headers, name, value)
}

pub(super) fn configured_headers(config: &OrcaTermConfig) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    for (name, value) in &config.headers {
        insert(&mut headers, name, value)?;
    }
    headers.insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
    Ok(headers)
}

pub(super) fn bad(message: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!("OrcaTerm request: {message}"))
}

fn text(value: &Value) -> Result<String, ChannelError> {
    if let Some(text) = value.as_str() {
        return Ok(text.into());
    }
    let parts = value
        .as_array()
        .ok_or_else(|| bad("message content must be text"))?;
    let mut out = String::new();
    for part in parts {
        if part["type"] != "text" {
            return Err(bad("this channel supports text content only"));
        }
        out.push_str(
            part["text"]
                .as_str()
                .ok_or_else(|| bad("text part has no text"))?,
        );
    }
    Ok(out)
}

pub(super) struct ChatRequest {
    pub body: Value,
    pub uploads: Vec<images::Upload>,
    pub effort: Option<String>,
}

pub(super) fn build(input: &Value) -> Result<ChatRequest, ChannelError> {
    let model = input["model"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| bad("missing model"))?;
    let messages = input["messages"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| bad("messages must be a nonempty array"))?;
    let mut history = Vec::new();
    let mut system = Vec::new();
    let mut last = None;
    let mut files = Vec::new();
    let mut uploads = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        match message["role"].as_str() {
            Some("system" | "developer") => system.push(text(&message["content"])?),
            Some("user") => {
                let current = index + 1 == messages.len();
                let target = if current { "/input/files".to_owned() } else { format!("/input/historyMessages/{}/files", history.len()) };
                let (content, images) = images::content(&message["content"], &target, &mut uploads)?;
                if current {
                    last = Some(content);
                    files = images;
                } else {
                    history.push(json!({"id":new_id()?,"type":"human","content":content,"files":images}));
                }
            }
            Some("tool" | "function") => continue,
            Some("assistant") if message["content"].is_null() => continue,
            Some("assistant") => history.push(json!({"id":new_id()?,"type":"ai","content":json!({
                "action":"completion","thinking":message["reasoning_content"].as_str().unwrap_or_default(),"taskCompletion":text(&message["content"])?
            }).to_string()})),
            _ => return Err(bad("unsupported message role")),
        }
    }
    let last = last.ok_or_else(|| bad("the last message must be a user message"))?;
    let mut kwargs = serde_json::Map::new();
    for key in ["temperature", "top_p", "max_tokens", "stop"] {
        if let Some(value) = input.get(key) {
            kwargs.insert(key.into(), value.clone());
        }
    }
    if let Some(value) = input.get("max_completion_tokens") {
        kwargs.insert("max_tokens".into(), value.clone());
    }
    let effort = match input.get("reasoning_effort") {
        None | Some(Value::Null) => None,
        Some(Value::String(effort)) => Some(effort.clone()),
        _ => None,
    };
    let multimodal = !uploads.is_empty();
    Ok(ChatRequest {
        uploads,
        effort,
        body: json!({
            "conversationId":format!("cid-{}-orcaterm", new_id()?),
            "user":{"id":"","setting":{
                "model":model,"modelDesc":{"kwargs":kwargs},"mcpServers":[],"uiServers":[],"tools":[],
                "approvedTools":[],"approvedMCPTools":[],"selectedMCPTools":{},"selectedUITools":{},
                "enableAutoSubtaskExecution":false,"env":[],"extra":{},"language":"zh",
                "multimodal":multimodal
            }},
            "input":{"id":new_id()?,"type":"start","input":last,"historyMessages":history,
                "emphasisPrompt":system.join("\n\n"),"files":files},"stream":true
        }),
    })
}

pub(super) fn prepare(
    ctx: PrepareContext<'_>,
    body: Value,
) -> Result<http::Request<HttpBody>, ChannelError> {
    if !OrcaTerm
        .native_dialects(ctx.provider, ctx.operation.operation)
        .contains(&ctx.operation.dialect)
    {
        return Err(ChannelError::UnsupportedOperation(ctx.operation));
    }
    let config = OrcaTermConfig::from_view(ctx.provider)?;
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLIENT_HEADERS)?;
    let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), OWNED_HEADERS);
    headers.extend(configured_headers(&config)?);
    headers.extend(auth::headers(ctx.credential.secret)?);
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("text/event-stream"),
    );
    insert(
        &mut headers,
        "x-seqid",
        body["input"]["id"].as_str().expect("generated id"),
    )?;
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => {
            let mut url =
                url::Url::parse(&format!("{}/assistant/chat", config.base_url(ctx.provider)))
                    .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
            url.query_pairs_mut()
                .append_pair("name", "orcaterm")
                .append_pair("mode", "0")
                .append_pair("csrfCode", &auth::csrf(ctx.credential.secret));
            url.to_string()
        }
    };
    let mut request = http::Request::builder()
        .method(Method::POST)
        .uri(url)
        .body(HttpBody::Bytes(body.to_string().into()))
        .map_err(|e| bad(&e.to_string()))?;
    *request.headers_mut() = headers;
    Ok(request)
}
