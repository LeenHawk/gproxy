use super::{
    CommandCode,
    config::{CommandCodeConfig, api_key, base_url},
};
use crate::channel::{BaseChannel, ChannelHeaders, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::claude_images;
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{HttpBody, Operation};
use gproxy_upstream::UpstreamError;
use http::{HeaderMap, HeaderValue, Method, header};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const CLI_VERSION: &str = "1.88.2";
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["x-session-id"],
    prefixes: &[],
};
const IDENTITY_HEADERS: &[&str] = &[
    "user-agent",
    "x-command-code-version",
    "x-cli-environment",
    "x-cli-surface",
    "x-project-slug",
    "x-taste-learning",
    "x-cmd-zdr",
    "x-oauth-token",
    "x-oauth-provider",
    "x-oss-primary-provider",
    "x-cmd-provider-deepseek-internal",
];

pub(super) fn headers(key: &str) -> Result<HeaderMap, UpstreamError> {
    let mut out = HeaderMap::new();
    out.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| UpstreamError::InvalidCredential)?,
    );
    out.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    out.insert(header::USER_AGENT, HeaderValue::from_static("cli"));
    out.insert(
        "x-command-code-version",
        HeaderValue::from_static(CLI_VERSION),
    );
    out.insert("x-cli-environment", HeaderValue::from_static("production"));
    Ok(out)
}

fn invalid_request(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(format!("Command Code: {message}"))
}

fn text_blocks(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| {
            (part["type"] == "text")
                .then(|| part["text"].as_str())
                .flatten()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Match the CLI's non-git buildServerConfig without reading the gateway's
/// workspace or machine identity. The upstream requires all nine fields.
fn workspace_config() -> Result<Value, UpstreamError> {
    let elapsed = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_err(|e| UpstreamError::Host(format!("Command Code: invalid system clock: {e}")))?;
    let seconds =
        i64::try_from(elapsed.as_secs()).map_err(|_| UpstreamError::Host("Command Code: system clock is out of range".into()))?;
    let date = time::OffsetDateTime::from_unix_timestamp(seconds)
        .map_err(|e| UpstreamError::Host(format!("Command Code: invalid system date: {e}")))?
        .date()
        .to_string();
    Ok(json!({
        "workingDir": "/workspace/gproxy",
        "date": date,
        "environment": "linux",
        "structure": [],
        "isGitRepo": false,
        "currentBranch": "",
        "mainBranch": "",
        "gitStatus": "",
        "recentCommits": [],
    }))
}

/// CLI toWireMessages/toWireTools: Claude blocks become AI SDK model messages.
fn envelope(body: &[u8]) -> Result<Value, UpstreamError> {
    let mut input: Value = serde_json::from_slice(body).map_err(|e| invalid_request(&e.to_string()))?;
    claude_images::fit(&mut input, false);
    let model = input["model"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid_request("missing model"))?;
    let mut messages = Vec::new();
    let mut tools_by_id = BTreeMap::new();
    for message in input["messages"]
        .as_array()
        .ok_or_else(|| invalid_request("messages must be an array"))?
    {
        let role = message["role"]
            .as_str()
            .ok_or_else(|| invalid_request("missing message role"))?;
        if !matches!(role, "assistant" | "user") {
            return Err(invalid_request("expected user or assistant message"));
        }
        let content = if let Some(text) = message["content"].as_str() {
            vec![json!({"type":"text", "text":text})]
        } else {
            message["content"]
                .as_array()
                .ok_or_else(|| invalid_request("invalid message content"))?
                .clone()
        };
        let mut parts = Vec::new();
        let mut results = Vec::new();
        for part in content {
            match (role, part["type"].as_str().unwrap_or_default()) {
                (_, "text") => parts.push(json!({"type":"text", "text":part["text"]})),
                ("assistant", "thinking") => parts.push(json!({"type":"reasoning", "text":part["thinking"]})),
                ("assistant", "tool_use") => {
                    let id = part["id"].as_str().ok_or_else(|| invalid_request("missing tool call id"))?;
                    tools_by_id.insert(id.to_owned(), part["name"].clone());
                    parts.push(json!({"type":"tool-call", "toolCallId":id, "toolName":part["name"], "input":part["input"]}));
                }
                ("user", "tool_result") => results.push(json!({
                    "type":"tool-result", "toolCallId":part["tool_use_id"],
                    "toolName":tools_by_id.get(part["tool_use_id"].as_str().unwrap_or_default()).cloned().unwrap_or(json!("unknown")),
                    "output":{"type":"text", "value":text_blocks(&part["content"])}
                })),
                ("user", "image") if part["source"]["type"] == "base64" => {
                    let mime = part["source"]["media_type"].as_str().ok_or_else(|| invalid_request("missing image media type"))?;
                    let data = part["source"]["data"].as_str().ok_or_else(|| invalid_request("missing image data"))?;
                    parts.push(json!({"type":"image", "image":format!("data:{mime};base64,{data}"), "mimeType":mime}));
                }
                _ => return Err(invalid_request("content block is not supported by the CLI wire")),
            }
        }
        if !results.is_empty() {
            messages.push(json!({"role":"tool", "content":results}));
        }
        if role == "assistant" || !parts.is_empty() {
            messages.push(json!({"role":role, "content":parts}));
        }
    }
    let mut tools = Vec::new();
    if let Some(items) = input.get("tools") {
        for tool in items
            .as_array()
            .ok_or_else(|| invalid_request("tools must be an array"))?
        {
            if !tool["name"].is_string() || !tool["input_schema"].is_object() {
                return Err(invalid_request("only client function tools are supported"));
            }
            tools.push(json!({"name":tool["name"], "description":tool.get("description").cloned().unwrap_or(json!("")), "input_schema":tool["input_schema"]}));
        }
    }
    // The CLI has no forced tool-choice field on this surface.
    if let Some(choice) = input.get("tool_choice")
        && choice.get("type").and_then(Value::as_str) != Some("auto")
    {
        return Err(invalid_request("CLI wire supports only automatic tool choice"));
    }
    let mut params = json!({"model":model, "messages":messages, "tools":tools,
        "system":input.get("system").cloned().unwrap_or(json!("")),
        "max_tokens":input.get("max_tokens").cloned().unwrap_or(json!(64000)), "stream":true});
    if let Some(value) = input.get("temperature") {
        params["temperature"] = value.clone();
    }
    if let Some(value) = input
        .get("reasoning_effort")
        .or_else(|| input.pointer("/output_config/effort"))
    {
        params["reasoning_effort"] = value.clone();
    }
    Ok(
        json!({"config":workspace_config()?, "memory":null, "taste":null, "skills":null,
        "permissionMode":"standard", "params":params}),
    )
}

pub(super) fn build(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, UpstreamError> {
    if !CommandCode
        .native_dialects(ctx.provider, ctx.operation.operation)
        .contains(&ctx.operation.dialect)
    {
        return Err(UpstreamError::UnsupportedOperation(ctx.operation));
    }
    let config = CommandCodeConfig::from_view(ctx.provider)?;
    let catalog = ctx.operation.operation == Operation::ListModels;
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
    let mut out = forwardable(&ctx.request.headers, allowlist.as_ref(), IDENTITY_HEADERS);
    for (name, value) in &config.headers {
        insert_configured(&mut out, name, value)?;
    }
    let body = if catalog {
        // The upstream catalogue is public, as in the CLI's catalog refresh.
        out.remove(header::AUTHORIZATION);
        out.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        HttpBody::Bytes(Default::default())
    } else {
        out.extend(headers(api_key(ctx.credential.secret)?)?);
        insert_configured(&mut out, "x-project-slug", &config.project_slug)?;
        out.insert("x-taste-learning", HeaderValue::from_static("false"));
        if config.zero_data_retention {
            out.insert("x-cmd-zdr", HeaderValue::from_static("1"));
        }
        if !out.contains_key("x-session-id") {
            insert_configured(&mut out, "x-session-id", &new_id()?)?;
        }
        let HttpBody::Bytes(bytes) = ctx.request.body else {
            return Err(UpstreamError::InvalidConfig("Command Code: generation needs a buffered JSON body".into()));
        };
        HttpBody::Bytes(envelope(&bytes)?.to_string().into())
    };
    let url = ctx.endpoint_override.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "{}{}",
            base_url(ctx.provider),
            if catalog {
                "/provider/v1/models?view=client"
            } else {
                "/alpha/generate"
            }
        )
    });
    let url = match ctx
        .request
        .query
        .as_deref()
        .map(strip_query_auth)
        .filter(|q| !q.is_empty())
    {
        Some(query) => format!("{url}{}{query}", if url.contains('?') { "&" } else { "?" }),
        None => url,
    };
    let mut request = http::Request::builder()
        .method(if catalog { Method::GET } else { Method::POST })
        .uri(url)
        .body(body)
        .map_err(|e| UpstreamError::InvalidConfig(e.to_string()))?;
    *request.headers_mut() = out;
    Ok(request)
}

/// A session for callers without a CLI session id; never share unrelated turns.
pub(super) fn new_id() -> Result<String, UpstreamError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| UpstreamError::Host(e.to_string()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let text = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &text[..8],
        &text[8..12],
        &text[12..16],
        &text[16..20],
        &text[20..]
    ))
}
