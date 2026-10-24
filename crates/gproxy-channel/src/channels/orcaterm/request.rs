use super::{
    OrcaTerm, auth,
    config::{OrcaTermConfig, new_id},
    models,
};
use crate::channel::{
    BaseChannel, ChannelError, ChannelHeaders, HeaderAllowlist, PrepareContext, forwardable,
};
use crate::channels::shared::compatible::http::insert_configured;
use gproxy_protocol::{HttpBody, Operation};
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

fn bad(message: &str) -> ChannelError {
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

fn envelope(input: Value, user_id: &str) -> Result<Value, ChannelError> {
    let model = input["model"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| bad("missing model"))?;
    for name in ["tools", "functions"] {
        if input
            .get(name)
            .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err(bad(
                "client function tools are not supported by the audited OrcaTerm wire",
            ));
        }
    }
    if input
        .get("tool_choice")
        .is_some_and(|v| !v.is_null() && v != "none" && v != "auto")
        || input
            .get("function_call")
            .is_some_and(|v| !v.is_null() && v != "none")
    {
        return Err(bad("forced tool choice is not supported"));
    }
    if input.get("n").is_some_and(|v| !v.is_null() && v != 1)
        || input
            .get("response_format")
            .is_some_and(|v| !v.is_null() && v["type"] != "text")
    {
        return Err(bad("only one text completion is supported"));
    }
    if input.get("audio").is_some_and(|v| !v.is_null())
        || input.get("modalities").is_some_and(|v| {
            !v.is_null() && v.as_array().is_none_or(|a| a.iter().any(|v| v != "text"))
        })
    {
        return Err(bad("audio output is not supported"));
    }
    let messages = input["messages"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| bad("messages must be a nonempty array"))?;
    let mut history = Vec::new();
    let mut system = Vec::new();
    let mut last = None;
    for (index, message) in messages.iter().enumerate() {
        if message
            .get("tool_calls")
            .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
            || message.get("function_call").is_some_and(|v| !v.is_null())
        {
            return Err(bad("tool history is not supported"));
        }
        let content = text(&message["content"])?;
        match message["role"].as_str() {
            Some("system" | "developer") => system.push(content),
            Some("user") if index + 1 == messages.len() => last = Some(content),
            Some("user") => history.push(json!({"id":new_id()?,"type":"human","content":content})),
            Some("assistant") => history.push(json!({"id":new_id()?,"type":"ai","content":json!({
                "action":"completion","thinking":message["reasoning_content"].as_str().unwrap_or_default(),"taskCompletion":content
            }).to_string()})),
            _ => return Err(bad("unsupported message role")),
        }
    }
    let last = last.ok_or_else(|| bad("the last message must be a user message"))?;
    let mut kwargs = serde_json::Map::new();
    for key in [
        "temperature",
        "top_p",
        "max_tokens",
        "stop",
        "reasoning_effort",
    ] {
        if let Some(value) = input.get(key) {
            kwargs.insert(key.into(), value.clone());
        }
    }
    if let Some(value) = input.get("max_completion_tokens") {
        kwargs.insert("max_tokens".into(), value.clone());
    }
    let thinking = input.get("reasoning_effort").is_none_or(|v| v != "none");
    if !thinking {
        kwargs.remove("reasoning_effort");
    }
    Ok(json!({
        "conversationId":format!("cid-{}-orcaterm", new_id()?),
        "user":{"id":user_id,"setting":{
            "model":model,"modelDesc":{"kwargs":kwargs},"mcpServers":[],"uiServers":[],"tools":[],
            "approvedTools":[],"approvedMCPTools":[],"selectedMCPTools":{},"selectedUITools":{},
            "enableAutoSubtaskExecution":false,"env":[],"extra":{},"language":"zh",
            "multimodal":false,"enableThinking":thinking
        }},
        "input":{"id":new_id()?,"type":"start","input":last,"historyMessages":history,
            "emphasisPrompt":system.join("\n\n"),"files":[]},"stream":true
    }))
}

pub(super) fn prepare(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
    if !OrcaTerm
        .native_dialects(ctx.provider, ctx.operation.operation)
        .contains(&ctx.operation.dialect)
    {
        return Err(ChannelError::UnsupportedOperation(ctx.operation));
    }
    if ctx.operation.operation == Operation::ListModels {
        return models::prepare(ctx);
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
    let HttpBody::Bytes(bytes) = ctx.request.body else {
        return Err(bad("requires a buffered JSON request"));
    };
    let input = serde_json::from_slice(&bytes).map_err(|_| bad("invalid JSON"))?;
    let user_id = auth::fact(ctx.credential, "user_id").ok_or(ChannelError::InvalidCredential)?;
    let body = envelope(input, user_id)?;
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
