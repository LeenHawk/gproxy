//! Claude refusal retries. Policy is channel-owned; execution uses the same
//! pinned credential and observed native path as the original request.
mod stream;

use super::native::NativeCall;
use gproxy_channel::{ChannelError, channel::ClaudeFallback};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireRequest, WireResponse, codec::read_http_body,
    connection::Bytes,
};
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_SENDS: usize = 8;
const PROMPT_FIELDS: &[&str] = &[
    "system",
    "messages",
    "tools",
    "tool_choice",
    "thinking",
    "cache_control",
    "output_config",
    "mcp_servers",
    "context_management",
    "container",
];

struct Policy {
    capability: ClaudeFallback,
    candidates: Vec<Value>,
    default: bool,
    tried: HashSet<String>,
    original: Value,
    primary: String,
    sent: usize,
}

struct Plan {
    body: Value,
    exact: Value,
    model: String,
    continuing: bool,
    tools: bool,
}

impl Policy {
    fn read(call: &NativeCall, wire: &WireRequest<HttpBody>) -> Result<Option<Self>, ChannelError> {
        if call.operation.dialect != Dialect::Claude
            || !matches!(
                call.operation.operation,
                Operation::GenerateContent | Operation::StreamGenerateContent
            )
        {
            return Ok(None);
        }
        let provider = &call.attempt.request.target.provider;
        let Some(capability) = provider.channel.claude_fallback() else {
            return Ok(None);
        };
        let HttpBody::Bytes(bytes) = &wire.body else {
            return Ok(None);
        };
        let Ok(body) = serde_json::from_slice::<Value>(bytes) else {
            return Ok(None);
        };
        if body
            .get("fallback_credit_token")
            .is_some_and(|v| !v.is_null())
        {
            return Ok(None);
        }
        let Some(primary) = body.get("model").and_then(Value::as_str) else {
            return Ok(None);
        };
        let config = &provider.entity.config;
        let setting = body
            .get("fallbacks")
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| {
                match config
                    .get("fallback_mode")
                    .or_else(|| config.get("claude_fallback_mode"))
                    .and_then(Value::as_str)
                {
                    Some("default") => Some(json!("default")),
                    Some("models") => Some(
                        config
                            .get("fallback_models")
                            .or_else(|| config.get("claude_fallback_models"))
                            .cloned()
                            .unwrap_or(json!([])),
                    ),
                    _ => None,
                }
            });
        let Some(setting) = setting else {
            return Ok(None);
        };
        let mut candidates: Vec<Value> = Vec::new();
        if let Some(entries) = setting.as_array() {
            for entry in entries {
                let model = entry
                    .as_str()
                    .or_else(|| entry.get("model").and_then(Value::as_str))
                    .ok_or_else(|| invalid("fallback entry requires a model"))?
                    .trim();
                if model.is_empty() {
                    continue;
                }
                let model = provider.channel.fallback_model(primary, model);
                if model == primary || candidates.iter().any(|v| v["model"] == model) {
                    continue;
                }
                let mut candidate = if entry.is_object() {
                    entry.clone()
                } else {
                    json!({})
                };
                candidate["model"] = json!(model);
                candidates.push(candidate);
                if candidates.len() == 3 {
                    break;
                }
            }
        } else if setting != "default" {
            return Err(invalid("fallbacks must be default or a model list"));
        }
        let default = setting == "default" || candidates.is_empty();
        if candidates.is_empty() {
            let model = provider
                .channel
                .fallback_model(primary, capability.recommended_model);
            if model != primary {
                candidates.push(json!({"model":model}));
            }
        }
        Ok(Some(Self {
            capability,
            candidates,
            default,
            tried: HashSet::from([primary.to_owned()]),
            primary: primary.to_owned(),
            original: body,
            sent: 1,
        }))
    }

    fn plan(&mut self, call: &NativeCall, refused: &Value, emitted: bool) -> Option<Plan> {
        if refused["stop_reason"] != "refusal" || self.sent >= MAX_SENDS || self.tried.len() >= 4 {
            return None;
        }
        let channel = &call.attempt.request.target.provider.channel;
        let mut entry = None;
        if let Some(model) = refused
            .pointer("/stop_details/recommended_model")
            .and_then(Value::as_str)
        {
            let model = channel.fallback_model(&self.primary, model);
            if !self.tried.contains(&model)
                && (self.default || self.candidates.iter().any(|v| v["model"] == model))
            {
                entry = Some(json!({"model":model}));
            }
        }
        let entry = entry.or_else(|| {
            self.candidates
                .iter()
                .find(|v| v["model"].as_str().is_some_and(|m| !self.tried.contains(m)))
                .cloned()
        })?;
        let model = entry["model"].as_str()?.to_owned();
        let mut exact = self.original.clone();
        let object = exact.as_object_mut()?;
        object.remove("fallbacks");
        object.remove("fallback_credit_token");
        object.insert("model".into(), json!(model));
        for name in ["max_tokens", "thinking", "output_config", "speed"] {
            if let Some(value) = entry.get(name) {
                object.insert(name.into(), value.clone());
            }
        }
        let prompt_matches = PROMPT_FIELDS
            .iter()
            .all(|key| self.original.get(*key) == exact.get(*key));
        let token = refused
            .pointer("/stop_details/fallback_credit_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty() && self.capability.credit && prompt_matches);
        // Even a partially executed server tool must not be restarted without
        // a redeemable credit. Client tool calls have not been executed here.
        let tools = refused
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|blocks| {
                blocks.iter().any(|b| {
                    matches!(b["type"].as_str(), Some("server_tool_use" | "mcp_tool_use"))
                        || b["type"]
                            .as_str()
                            .is_some_and(|t| t.ends_with("tool_result"))
                })
            });
        if tools && token.is_none() {
            return None;
        }
        if let Some(token) = token {
            exact["fallback_credit_token"] = json!(token);
        }
        let continuing = token.is_some()
            && refused.pointer("/stop_details/fallback_has_prefill_claim")
                != Some(&Value::Bool(false));
        if emitted && !continuing {
            return None;
        }
        let mut body = exact.clone();
        if continuing {
            let mut content = refused
                .get("content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            // A pending client tool call has no result and cannot be replayed
            // as completed history.
            content.retain(|block| block["type"] != "tool_use");
            if let Some(last) = content.last_mut().filter(|b| b["type"] == "text")
                && let Some(text) = last["text"].as_str()
            {
                last["text"] = json!(text.trim_end());
            }
            body.get_mut("messages")?
                .as_array_mut()?
                .push(json!({"role":"assistant","content":content}));
        }
        self.tried.insert(model.clone());
        Some(Plan {
            body,
            exact,
            model,
            continuing,
            tools,
        })
    }
}

fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

fn replay(template: &WireRequest<Bytes>, body: &Value) -> WireRequest<HttpBody> {
    let mut headers = template.headers.clone();
    headers.remove(http::header::CONTENT_LENGTH);
    WireRequest {
        method: template.method.clone(),
        path: template.path.clone(),
        query: template.query.clone(),
        headers,
        body: HttpBody::Bytes(Bytes::from(body.to_string())),
    }
}

async fn collect(call: &NativeCall, body: HttpBody) -> Result<Bytes, ChannelError> {
    let request = &call.attempt.request;
    let timeout = request
        .deadline
        .map_or(call.limits.operation_total, |deadline| {
            deadline
                .saturating_duration_since(web_time::Instant::now())
                .min(call.limits.operation_total)
        });
    tokio::select! {
        biased;
        () = request.cancellation.cancelled() => Err(invalid("request cancelled")),
        result = crate::rt::timeout(timeout, read_http_body(body, request.snapshot.limits.codec())) =>
            result.ok_or_else(|| invalid("fallback response deadline exceeded"))?.map_err(|e| invalid(e.to_string())),
    }
}

/// Redeem a continuation first. Before any output is exposed, a rejected
/// prefill may fall back to an exact replay; a rejected credit may be dropped
/// only when no server tool ran. Every physical exchange remains observed.
async fn send_plan(
    call: &NativeCall,
    template: &WireRequest<Bytes>,
    policy: &mut Policy,
    plan: &mut Plan,
    emitted: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let mut temporary_retries = 0;
    loop {
        policy.sent += 1;
        let mut response = call.send_once(replay(template, &plan.body)).await?;
        if response.status != http::StatusCode::BAD_REQUEST {
            return Ok(response);
        }
        let bytes = collect(call, response.body).await?;
        let message = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or_else(|| v.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        response.body = HttpBody::Bytes(bytes);
        if policy.sent >= MAX_SENDS {
            return Ok(response);
        }
        if plan.body.get("fallback_credit_token").is_some()
            && message.contains("redemption temporarily unavailable")
            && temporary_retries < 2
        {
            temporary_retries += 1;
            continue;
        }
        if emitted {
            return Ok(response);
        }
        if plan.continuing {
            plan.body = plan.exact.clone();
            plan.continuing = false;
        } else if plan.body.get("fallback_credit_token").is_some()
            && message.contains("fallback_credit_token")
            && !plan.tools
        {
            plan.exact
                .as_object_mut()
                .unwrap()
                .remove("fallback_credit_token");
            plan.body = plan.exact.clone();
        } else {
            return Ok(response);
        }
    }
}

pub(super) async fn run(
    call: NativeCall,
    wire: WireRequest<HttpBody>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let Some(mut policy) = Policy::read(&call, &wire)? else {
        return call.send_once(wire).await;
    };
    // These upstreams do not accept the gateway's fallback extension.
    let mut original = policy.original.clone();
    original.as_object_mut().unwrap().remove("fallbacks");
    let wire = WireRequest {
        method: wire.method,
        path: wire.path,
        query: wire.query,
        headers: wire.headers,
        body: Bytes::new(),
    };
    let mut response = call.send_once(replay(&wire, &original)).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    if call.operation.operation == Operation::StreamGenerateContent {
        return Ok(stream::wrap(call, wire, policy, response));
    }
    let mut prefix = Vec::new();
    loop {
        let bytes = collect(&call, response.body).await?;
        let body = serde_json::from_slice::<Value>(&bytes).ok();
        let plan = body
            .as_ref()
            .and_then(|body| policy.plan(&call, body, false));
        let Some(mut plan) = plan else {
            response.body = if prefix.is_empty() {
                HttpBody::Bytes(bytes)
            } else if let Some(mut body) = body {
                if let Some(content) = body.get_mut("content").and_then(Value::as_array_mut) {
                    prefix.append(content);
                    *content = prefix;
                }
                response.headers.remove(http::header::CONTENT_LENGTH);
                HttpBody::Bytes(Bytes::from(body.to_string()))
            } else {
                HttpBody::Bytes(bytes)
            };
            return Ok(response);
        };
        let refused = body.unwrap();
        response = send_plan(&call, &wire, &mut policy, &mut plan, false).await?;
        if !response.status.is_success() {
            return Ok(response);
        }
        if plan.continuing {
            prefix.extend(
                refused
                    .get("content")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
        } else {
            prefix.clear();
        }
        prefix.push(json!({"type":"fallback","trigger":{"type":"refusal"},"from":{"model":refused["model"]},"to":{"model":plan.model}}));
    }
}
