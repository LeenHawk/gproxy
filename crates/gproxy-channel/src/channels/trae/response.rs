//! Raw Trae SSE: text/reasoning deltas, legacy output snapshots, indexed native
//! tool calls, token_usage and explicit done/error events. EOF is not success.
use super::{
    common::{invalid, now_ms, random_hex},
    models, request,
};
use crate::channel::{ChannelError, OperationContext, PrepareContext};
use futures_util::{StreamExt, stream};
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::{ByteStream, Bytes, TransportError},
};
use http::{HeaderValue, StatusCode, header};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

const LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 8 * 1024 * 1024,
    max_value_bytes: 8 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 8 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

fn encode(value: &Value, out: &mut Vec<Bytes>) {
    out.push(Bytes::from(format!("data: {value}\n\n")));
}
fn snapshot(previous: &mut String, current: &str) -> Result<String, ChannelError> {
    if current.is_empty() || previous.starts_with(current) {
        return Ok(String::new());
    }
    let delta = current
        .strip_prefix(previous.as_str())
        .ok_or_else(|| invalid("non-prefix text snapshot"))?
        .to_owned();
    *previous = current.into();
    Ok(delta)
}

struct Translator {
    decoder: SseDecoder,
    id: String,
    model: String,
    created: i64,
    collect: bool,
    text: String,
    reasoning: String,
    text_snapshot: String,
    reasoning_snapshot: String,
    tools: BTreeMap<u64, Value>,
    usage: Value,
    done: bool,
    error: Option<Value>,
    finish_reason: String,
}
impl Translator {
    fn new(model: String, collect: bool) -> Result<Self, ChannelError> {
        Ok(Self {
            decoder: SseDecoder::new(LIMITS),
            id: format!("chatcmpl-{}", random_hex::<16>()?),
            model,
            created: now_ms() / 1000,
            collect,
            text: String::new(),
            reasoning: String::new(),
            text_snapshot: String::new(),
            reasoning_snapshot: String::new(),
            tools: BTreeMap::new(),
            usage: json!({}),
            done: false,
            error: None,
            finish_reason: "stop".into(),
        })
    }
    fn chunk(&self, delta: Value, finish: Option<&str>) -> Value {
        json!({"id":self.id,"object":"chat.completion.chunk","created":self.created,"model":self.model,
            "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    }
    fn text(&mut self, text: String, reasoning: bool, out: &mut Vec<Bytes>) {
        if text.is_empty() {
            return;
        }
        if self.collect {
            if reasoning {
                self.reasoning.push_str(&text);
            } else {
                self.text.push_str(&text);
            }
        }
        let key = if reasoning {
            "reasoning_content"
        } else {
            "content"
        };
        encode(&self.chunk(json!({"role":"assistant",key:text}), None), out);
    }
    fn usage(&mut self, raw: &Value) {
        let raw = raw.get("usage").unwrap_or(raw);
        for (dst, keys) in [
            ("prompt_tokens", ["prompt_tokens", "input_token"]),
            ("completion_tokens", ["completion_tokens", "output_token"]),
            ("total_tokens", ["total_tokens", "total_token"]),
        ] {
            if let Some(value) = keys.iter().find_map(|k| raw.get(*k).filter(|v| v.is_u64())) {
                self.usage[dst] = value.clone();
            }
        }
        for (keys, dst) in [
            (
                ["cache_read_tokens", "cache_read_input_tokens"],
                "cached_tokens",
            ),
            (
                ["cache_write_tokens", "cache_creation_input_tokens"],
                "cache_write_tokens",
            ),
        ] {
            if let Some(value) = keys
                .iter()
                .find_map(|key| raw.get(*key).filter(|v| v.is_u64()))
            {
                if !self.usage["prompt_tokens_details"].is_object() {
                    self.usage["prompt_tokens_details"] = json!({});
                }
                self.usage["prompt_tokens_details"][dst] = value.clone();
            }
        }
        if let (Some(input), Some(output)) = (
            self.usage["prompt_tokens"].as_u64(),
            self.usage["completion_tokens"].as_u64(),
        ) && raw.get("total_tokens").is_none()
            && raw.get("total_token").is_none()
        {
            self.usage["total_tokens"] = json!(input.saturating_add(output));
        }
    }
    fn calls(&mut self, calls: &[Value], out: &mut Vec<Bytes>) -> Result<(), ChannelError> {
        let mut deltas = Vec::new();
        for (position, call) in calls.iter().enumerate() {
            let index = call["index"].as_u64().unwrap_or_else(|| {
                call["id"]
                    .as_str()
                    .map(|id| {
                        self.tools
                            .iter()
                            .find(|(_, tool)| tool["id"].as_str() == Some(id))
                            .map(|(index, _)| *index)
                            .unwrap_or_else(|| {
                                self.tools.keys().next_back().map_or(0, |index| index + 1)
                            })
                    })
                    .unwrap_or(position as u64)
            });
            let function = call
                .get("function")
                .or_else(|| call.get("function_call"))
                .ok_or_else(|| invalid("tool call has no function"))?;
            let name = function["name"].as_str().unwrap_or_default();
            let arguments = match function.get("arguments") {
                Some(Value::String(v)) => v.clone(),
                Some(Value::Object(v)) => serde_json::to_string(v).map_err(invalid)?,
                None | Some(Value::Null) => String::new(),
                _ => return Err(invalid("unsupported tool arguments")),
            };
            let new = !self.tools.contains_key(&index);
            let id = call["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("call_{}_{}", self.id, index));
            let entry = self.tools.entry(index).or_insert_with(
                || json!({"id":id,"type":"function","function":{"name":"","arguments":""}}),
            );
            if !name.is_empty() {
                let previous = entry["function"]["name"].as_str().unwrap_or_default();
                entry["function"]["name"] = json!(format!("{previous}{name}"));
            }
            let old = entry["function"]["arguments"].as_str().unwrap_or_default();
            entry["function"]["arguments"] = json!(format!("{old}{arguments}"));
            let mut delta = json!({"index":index,"function":{"arguments":arguments}});
            if new {
                delta["id"] = entry["id"].clone();
                delta["type"] = json!("function");
            }
            if !name.is_empty() {
                delta["function"]["name"] = json!(name);
            }
            deltas.push(delta);
        }
        if !deltas.is_empty() {
            encode(&self.chunk(json!({"tool_calls":deltas}), None), out);
        }
        Ok(())
    }
    fn frames(&mut self, frames: Vec<SseFrame>) -> Result<Vec<Bytes>, ChannelError> {
        let mut out = Vec::new();
        for frame in frames {
            let SseFrame::Event(event) = frame else {
                self.done = true;
                continue;
            };
            if event.event.as_deref().is_some_and(|name| {
                !matches!(
                    name,
                    "error"
                        | "done"
                        | "token_usage"
                        | "text_delta"
                        | "reasoning_delta"
                        | "translated_text"
                        | "output"
                        | "accumulated"
                        | "tool_calls"
                )
            }) {
                continue;
            }
            let value: Value = serde_json::from_str(&event.data).map_err(invalid)?;
            let kind = event
                .event
                .as_deref()
                .or_else(|| value["type"].as_str())
                .unwrap_or("output");
            if self.error.is_some() {
                continue;
            }
            match kind {
                "error" => {
                    let error = json!({"error":{"type":"upstream_error","code":value["code"],"message":value["message"].as_str().or(value["msg"].as_str()).unwrap_or("Trae upstream error")}});
                    encode(&error, &mut out);
                    self.error = Some(error);
                }
                "done" => {
                    self.done = true;
                    if let Some(reason) = value["finish_reason"]
                        .as_str()
                        .or(value["stop_reason"].as_str())
                    {
                        self.finish_reason = reason.into();
                    }
                }
                "token_usage" => {
                    self.usage(&value);
                    let mut chunk = self.chunk(json!({}), None);
                    chunk["choices"] = json!([]);
                    chunk["usage"] = self.usage.clone();
                    encode(&chunk, &mut out);
                }
                "text_delta" | "reasoning_delta" | "translated_text" => {
                    let text = value
                        .as_str()
                        .or(value["delta"].as_str())
                        .or(value["text"].as_str())
                        .or(value["content"].as_str())
                        .ok_or_else(|| invalid("text event has no text"))?;
                    self.text(text.into(), kind == "reasoning_delta", &mut out);
                }
                "output" | "accumulated" => {
                    if let Some(text) = value["response"].as_str() {
                        let delta = snapshot(&mut self.text_snapshot, text)?;
                        self.text(delta, false, &mut out);
                    } else if let Some(text) = value["content"].as_str() {
                        self.text(text.into(), value["type"] == "reasoning", &mut out);
                    }
                    if let Some(text) = value["reasoning_content"].as_str() {
                        let delta = snapshot(&mut self.reasoning_snapshot, text)?;
                        self.text(delta, true, &mut out);
                    } else if let Some(text) = value["reasoning"].as_str() {
                        self.text(text.into(), true, &mut out);
                    }
                    if let Some(calls) = value["tool_calls"].as_array() {
                        self.calls(calls, &mut out)?;
                    }
                }
                "tool_calls" => {
                    let calls = value
                        .as_array()
                        .or(value["tool_calls"].as_array())
                        .ok_or_else(|| invalid("tool event has no calls"))?;
                    self.calls(calls, &mut out)?;
                }
                _ => {} // Queue/progress/metadata events are not assistant text.
            }
        }
        Ok(out)
    }
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self.decoder.push(bytes).map_err(invalid)?;
        self.frames(frames)
    }
    fn finish(&mut self) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self.decoder.finish().map_err(invalid)?;
        let mut out = self.frames(frames)?;
        if self.error.is_some() {
            return Ok(out);
        }
        if !self.done {
            return Err(invalid("stream ended without a terminal event"));
        }
        let reason = self.reason();
        encode(&self.chunk(json!({}), Some(reason)), &mut out);
        if self.usage.as_object().is_some_and(|v| !v.is_empty()) {
            let mut chunk = self.chunk(json!({}), None);
            chunk["choices"] = json!([]);
            chunk["usage"] = self.usage.clone();
            encode(&chunk, &mut out);
        }
        out.push(Bytes::from_static(b"data: [DONE]\n\n"));
        Ok(out)
    }
    fn reason(&self) -> &str {
        if !self.tools.is_empty() {
            "tool_calls"
        } else {
            match self.finish_reason.as_str() {
                "max_tokens" => "length",
                "end_turn" => "stop",
                other => other,
            }
        }
    }
    fn result(&self) -> Value {
        if let Some(error) = &self.error {
            return error.clone();
        }
        let mut message = json!({"role":"assistant","content":self.text});
        if !self.reasoning.is_empty() {
            message["reasoning_content"] = json!(self.reasoning);
        }
        if !self.tools.is_empty() {
            message["tool_calls"] = json!(self.tools.values().collect::<Vec<_>>());
            if self.text.is_empty() {
                message["content"] = Value::Null;
            }
        }
        let mut result = json!({"id":self.id,"object":"chat.completion","created":self.created,"model":self.model,"choices":[{"index":0,"message":message,"finish_reason":self.reason()}]});
        if self.usage.as_object().is_some_and(|v| !v.is_empty()) {
            result["usage"] = self.usage.clone();
        }
        result
    }
}

fn byte_stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Stream(s) => s,
        HttpBody::Bytes(b) => Box::pin(stream::once(async move { Ok(b) })),
    }
}

pub(super) async fn invoke(
    ctx: OperationContext<'_>,
    streaming: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let HttpBody::Bytes(bytes) = &ctx.request.body else {
        return Err(ChannelError::InvalidConfig(
            "Trae requires a JSON request".into(),
        ));
    };
    let input: Value = serde_json::from_slice(bytes).map_err(invalid)?;
    let model = input["model"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::InvalidConfig("Trae: model is required".into()))?
        .to_owned();
    let selected =
        models::selected(ctx.provider, ctx.credential, ctx.client.as_ref(), &model).await?;
    let request = request::chat(
        PrepareContext {
            provider: ctx.provider,
            credential: ctx.credential,
            operation: OperationKey {
                operation: if streaming {
                    Operation::StreamGenerateContent
                } else {
                    Operation::GenerateContent
                },
                dialect: ctx.dialect,
            },
            request: ctx.request,
            endpoint_override: ctx.endpoint_override,
        },
        &selected,
    )?;
    let response = ctx.client.send(request).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let mut headers = response.headers;
    headers.remove(header::CONTENT_LENGTH);
    headers.remove(header::CONTENT_ENCODING);
    let mut translator = Translator::new(model, !streaming)?;
    let mut upstream = byte_stream(response.body);
    if !streaming {
        while let Some(chunk) = upstream.next().await {
            translator.push(&chunk.map_err(|e| invalid(e.to_string()))?)?;
        }
        translator.finish()?;
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        return Ok(WireResponse {
            status: if translator.error.is_some() {
                StatusCode::BAD_GATEWAY
            } else {
                response.status
            },
            headers,
            body: HttpBody::Bytes(Bytes::from(translator.result().to_string())),
        });
    }
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    let state = (
        upstream,
        translator,
        VecDeque::<Result<Bytes, TransportError>>::new(),
        false,
    );
    let body = HttpBody::Stream(Box::pin(stream::unfold(
        state,
        |(mut upstream, mut codec, mut queue, mut ended)| async move {
            loop {
                if let Some(item) = queue.pop_front() {
                    return Some((item, (upstream, codec, queue, ended)));
                }
                if ended {
                    return None;
                }
                let result = match upstream.next().await {
                    Some(Ok(bytes)) => codec.push(&bytes),
                    Some(Err(e)) => {
                        ended = true;
                        queue.push_back(Err(e));
                        continue;
                    }
                    None => {
                        ended = true;
                        codec.finish()
                    }
                };
                match result {
                    Ok(chunks) => queue.extend(chunks.into_iter().map(Ok)),
                    Err(e) => {
                        ended = true;
                        queue.push_back(Err(Box::new(e)));
                    }
                }
            }
        },
    )));
    Ok(WireResponse {
        status: response.status,
        headers,
        body,
    })
}
