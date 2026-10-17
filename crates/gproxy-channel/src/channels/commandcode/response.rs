use super::{CommandCode, request::CLI_VERSION};
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::compatible::http::read_body;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    connection::{ByteStream, Bytes},
};
use http::{HeaderValue, StatusCode, header};
use serde_json::{Value, json};

fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("Command Code: {}", message.into()))
}
fn encode(value: &Value, out: &mut Vec<u8>) {
    out.extend_from_slice(
        format!(
            "event: {}\ndata: {value}\n\n",
            value["type"].as_str().unwrap_or("message")
        )
        .as_bytes(),
    );
}

/// Each complete line is one AI SDK fullStream event (CLI consumeStream).
struct Translator {
    pending: Vec<u8>,
    started: bool,
    finished: bool,
    active: Option<(&'static str, usize)>,
    next_index: usize,
    content: Option<Vec<Value>>,
    model: String,
    id: String,
    stop_reason: &'static str,
    usage: Value,
    cache_write: Option<u64>,
    cache_write_1h: Option<u64>,
}

impl Translator {
    fn new(model: String, id: String, collect: bool) -> Self {
        Self {
            pending: Vec::new(),
            started: false,
            finished: false,
            active: None,
            next_index: 0,
            content: collect.then(Vec::new),
            model,
            id,
            stop_reason: "end_turn",
            usage: json!({}),
            cache_write: None,
            cache_write_1h: None,
        }
    }

    fn message(&self) -> Value {
        json!({"id":self.id, "type":"message", "role":"assistant", "model":self.model,
            "content":self.content.as_deref().unwrap_or_default(), "stop_reason":self.stop_reason,
            "stop_sequence":null, "usage":self.claude_usage()})
    }

    fn claude_usage(&self) -> Value {
        let mut usage = json!({});
        let read = self
            .usage
            .pointer("/inputTokenDetails/cacheReadTokens")
            .and_then(Value::as_u64);
        let write = self
            .usage
            .pointer("/inputTokenDetails/cacheWriteTokens")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0)
            .or(self.cache_write);
        if let Some(total) = self.usage.get("inputTokens").and_then(Value::as_u64) {
            usage["input_tokens"] = json!(
                self.usage
                    .pointer("/inputTokenDetails/noCacheTokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(|| total
                        .saturating_sub(read.unwrap_or(0))
                        .saturating_sub(write.unwrap_or(0)))
            );
        }
        if let Some(value) = self.usage.get("outputTokens") {
            usage["output_tokens"] = value.clone();
        }
        if let Some(value) = read {
            usage["cache_read_input_tokens"] = json!(value);
        }
        if let Some(value) = write {
            usage["cache_creation_input_tokens"] = json!(value);
        }
        if let Some(value) = self.cache_write_1h {
            usage["cache_creation"] = json!({"ephemeral_1h_input_tokens":value});
            if let Some(total) = write {
                usage["cache_creation"]["ephemeral_5m_input_tokens"] =
                    json!(total.saturating_sub(value));
            }
        }
        usage
    }

    fn start(&mut self, out: &mut Vec<u8>) {
        if self.started {
            return;
        }
        self.started = true;
        encode(
            &json!({"type":"message_start", "message":{"id":self.id, "type":"message", "role":"assistant",
            "model":self.model, "content":[], "stop_reason":null, "stop_sequence":null, "usage":{"input_tokens":0,"output_tokens":0}}}),
            out,
        );
    }

    fn close(&mut self, out: &mut Vec<u8>) {
        if let Some((_, index)) = self.active.take() {
            encode(&json!({"type":"content_block_stop", "index":index}), out);
        }
    }

    fn block(&mut self, kind: &'static str, block: Value, out: &mut Vec<u8>) -> usize {
        self.close(out);
        let index = self.next_index;
        self.next_index += 1;
        if let Some(content) = &mut self.content {
            content.push(block.clone());
        }
        self.active = Some((kind, index));
        encode(
            &json!({"type":"content_block_start", "index":index, "content_block":block}),
            out,
        );
        index
    }

    fn delta(&mut self, thinking: bool, text: &str, out: &mut Vec<u8>) {
        if text.is_empty() {
            return;
        }
        let kind = if thinking { "thinking" } else { "text" };
        let index = match self.active {
            Some((active, index)) if active == kind => index,
            _ => self.block(
                kind,
                if thinking {
                    json!({"type":"thinking", "thinking":"", "signature":""})
                } else {
                    json!({"type":"text", "text":""})
                },
                out,
            ),
        };
        if let Some(content) = &mut self.content
            && let Value::String(value) = &mut content[index][kind]
        {
            value.push_str(text);
        }
        let delta = if thinking {
            json!({"type":"thinking_delta", "thinking":text})
        } else {
            json!({"type":"text_delta", "text":text})
        };
        encode(
            &json!({"type":"content_block_delta", "index":index, "delta":delta}),
            out,
        );
    }

    fn event(&mut self, line: &[u8], out: &mut Vec<u8>) -> Result<(), ChannelError> {
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(());
        }
        let event: Value = serde_json::from_slice(line)
            .map_err(|e| invalid(format!("invalid NDJSON event: {e}")))?;
        let kind = event["type"]
            .as_str()
            .ok_or_else(|| invalid("event has no type"))?;
        if kind == "error" {
            let error = &event["error"];
            let message = error
                .as_str()
                .or_else(|| error["message"].as_str())
                .unwrap_or("upstream stream error");
            let status = error["statusCode"]
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .and_then(|n| StatusCode::from_u16(n).ok())
                .filter(|status| status.is_client_error() || status.is_server_error())
                .unwrap_or(StatusCode::BAD_GATEWAY);
            return Err(ChannelError::UpstreamResponse {
                status,
                body: json!({"error":{"type":"upstream_error", "message":message}})
                    .to_string()
                    .into(),
            });
        }
        if kind == "abort" {
            return Err(invalid("upstream aborted generation"));
        }
        self.start(out);
        match kind {
            "text-delta" => self.delta(false, event["text"].as_str().unwrap_or_default(), out),
            "reasoning-delta" => self.delta(true, event["text"].as_str().unwrap_or_default(), out),
            "reasoning-start" | "reasoning-end" => self.close(out),
            "tool-call" => {
                if event["providerExecuted"] == true {
                    return Err(invalid("server-executed tools are not supported"));
                }
                let id = event["toolCallId"]
                    .as_str()
                    .ok_or_else(|| invalid("tool call has no id"))?;
                let name = event["toolName"]
                    .as_str()
                    .ok_or_else(|| invalid("tool call has no name"))?;
                let raw = event
                    .get("input")
                    .or_else(|| event.get("args"))
                    .cloned()
                    .unwrap_or(json!({}));
                let input = match raw.as_str() {
                    Some(text) => serde_json::from_str::<Value>(text)
                        .map_err(|e| invalid(format!("tool arguments: {e}")))?,
                    None => raw,
                };
                if !input.is_object() {
                    return Err(invalid("tool arguments must be an object"));
                }
                let index = self.block(
                    "tool_use",
                    json!({"type":"tool_use", "id":id, "name":name, "input":{}}),
                    out,
                );
                if let Some(content) = &mut self.content {
                    content[index]["input"] = input.clone();
                }
                encode(
                    &json!({"type":"content_block_delta", "index":index, "delta":{"type":"input_json_delta", "partial_json":input.to_string()}}),
                    out,
                );
                self.close(out);
            }
            "tool-result" => return Err(invalid("server-executed tool results are not supported")),
            "finish" => {
                self.finished = true;
                self.usage = event.get("totalUsage").cloned().unwrap_or(json!({}));
                let raw = event["rawFinishReason"]
                    .as_str()
                    .or_else(|| event["finishReason"].as_str())
                    .unwrap_or_default();
                self.stop_reason = match raw {
                    "tool_use" | "tool-calls" | "tool_calls" => "tool_use",
                    "length" | "max_tokens" => "max_tokens",
                    "refusal" | "content_filter" | "content-filter" => "refusal",
                    "pause_turn" => "pause_turn",
                    _ => "end_turn",
                };
            }
            "cache-write-tokens" => self.cache_write = event["cacheWriteTokens"].as_u64(),
            "provider-metadata" => self.cache_write_1h = event
                .pointer(
                    "/providerMetadata/anthropic/usage/cache_creation/ephemeral_1h_input_tokens",
                )
                .and_then(Value::as_u64),
            _ => {} // start/step/tool-input events are completed by tool-call/finish.
        }
        Ok(())
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Bytes, ChannelError> {
        self.pending.extend_from_slice(chunk);
        let mut out = Vec::new();
        let mut offset = 0;
        while let Some(end) = self.pending[offset..].iter().position(|b| *b == b'\n') {
            let end = offset + end;
            let line = self.pending[offset..end].to_vec();
            self.event(&line, &mut out)?;
            offset = end + 1;
        }
        self.pending.drain(..offset);
        Ok(out.into())
    }

    fn finish(&mut self) -> Result<Bytes, ChannelError> {
        let mut out = Vec::new();
        let tail = std::mem::take(&mut self.pending);
        self.event(&tail, &mut out)?;
        if !self.finished {
            return Err(invalid("stream ended without a finish event"));
        }
        self.close(&mut out);
        encode(
            &json!({"type":"message_delta", "delta":{"stop_reason":self.stop_reason,"stop_sequence":null}, "usage":self.claude_usage()}),
            &mut out,
        );
        encode(&json!({"type":"message_stop"}), &mut out);
        Ok(out.into())
    }
}

fn byte_stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
    }
}

fn translate(body: HttpBody, translator: Translator) -> ByteStream {
    Box::pin(futures_util::stream::unfold(
        Some((byte_stream(body), translator)),
        |state| async move {
            let (mut upstream, mut translator) = state?;
            loop {
                match upstream.next().await {
                    Some(Ok(chunk)) => match translator.push(&chunk) {
                        Ok(bytes) if bytes.is_empty() => continue,
                        Ok(bytes) => return Some((Ok(bytes), Some((upstream, translator)))),
                        Err(error) => return Some((Err(error.to_string().into()), None)),
                    },
                    Some(Err(error)) => return Some((Err(error), None)),
                    None => {
                        return Some((translator.finish().map_err(|e| e.to_string().into()), None));
                    }
                }
            }
        },
    ))
}

pub(super) async fn invoke(
    channel: &CommandCode,
    ctx: OperationContext<'_>,
    streaming: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let operation = if streaming {
        Operation::StreamGenerateContent
    } else {
        Operation::GenerateContent
    };
    let prepared = channel.prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    let model = match prepared.body() {
        HttpBody::Bytes(bytes) => serde_json::from_slice::<Value>(bytes)
            .ok()
            .and_then(|v| {
                v.pointer("/params/model")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default(),
        _ => String::new(),
    };
    let mut response = ctx.client.send(prepared).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let mut translator = Translator::new(
        model,
        format!("msg_{}", super::request::new_id()?),
        !streaming,
    );
    response.body = if streaming {
        HttpBody::Stream(translate(response.body, translator))
    } else {
        let mut stream = byte_stream(response.body);
        while let Some(chunk) = stream.next().await {
            translator.push(&chunk.map_err(|e| invalid(e.to_string()))?)?;
        }
        translator.finish()?;
        HttpBody::Bytes(translator.message().to_string().into())
    };
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(if streaming {
            "text/event-stream"
        } else {
            "application/json"
        }),
    );
    Ok(response)
}

pub(super) async fn models(
    channel: &CommandCode,
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let prepared = channel.prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation: Operation::ListModels,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    let mut response = ctx.client.send(prepared).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let bytes = read_body(response.body).await?;
    let mut catalog: Value = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    let data = catalog["data"]
        .as_array_mut()
        .ok_or_else(|| invalid("catalog has no data array"))?;
    for model in data {
        if !model["id"].is_string() {
            return Err(invalid("catalog entry has no model id"));
        }
        model["object"] = json!("model");
        model["owned_by"] = json!("commandcode");
        model["cli_version"] = json!(CLI_VERSION);
    }
    catalog["object"] = json!("list");
    response.body = HttpBody::Bytes(catalog.to_string().into());
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}
