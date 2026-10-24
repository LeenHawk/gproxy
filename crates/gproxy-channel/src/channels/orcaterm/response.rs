use super::{
    auth,
    config::{OrcaTermConfig, invalid, new_id, now_ms},
    images, models, request,
    tools::{Pending, Tools, json_content},
};
use crate::channel::{
    ChannelError, ChannelState, CredentialView, OperationContext, PrepareContext,
};
use crate::channels::shared::compatible::http::read_body;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::{ByteStream, Bytes},
};
use http::{HeaderValue, header};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

const MAX_MESSAGE: usize = 4 * 1024 * 1024;
const LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: MAX_MESSAGE as u64,
    max_value_bytes: MAX_MESSAGE as u64,
    max_body_bytes: 64 * 1024 * 1024,
    max_line_bytes: MAX_MESSAGE as u64,
    max_part_bytes: 0,
    max_parts: 0,
};

/// Locate a JSON string's closing quote without mistaking an escaped quote for
/// the delimiter. Outer SSE JSON already guarantees valid UTF-8.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

/// Root string fields only: a tool argument called taskCompletion is never an
/// answer. An incomplete terminal escape (including a surrogate pair) is held
/// until the next delta rather than emitted as a replacement character.
fn partial_strings(input: &str) -> BTreeMap<String, String> {
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut fields = BTreeMap::new();
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'{') {
        return fields;
    }
    index += 1;
    loop {
        while bytes
            .get(index)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
        {
            index += 1;
        }
        if bytes.get(index) != Some(&b'"') {
            break;
        }
        let Some(end) = string_end(bytes, index) else {
            break;
        };
        let Ok(key) = serde_json::from_slice::<String>(&bytes[index..end]) else {
            break;
        };
        index = end;
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) != Some(&b':') {
            break;
        }
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'"') {
            if let Some(end) = string_end(bytes, index) {
                if let Ok(value) = serde_json::from_slice::<String>(&bytes[index..end]) {
                    fields.insert(key, value);
                }
                index = end;
            } else {
                let raw = &input[index..];
                // Twelve bytes cover the longest incomplete JSON escape:
                // a high-surrogate followed by an unfinished low-surrogate.
                for trim in 0..=12.min(raw.len().saturating_sub(1)) {
                    let end = raw.len() - trim;
                    if !raw.is_char_boundary(end) {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<String>(&format!("{}\"", &raw[..end]))
                    {
                        fields.insert(key, value);
                        break;
                    }
                }
                break;
            }
        } else {
            let mut depth = 0usize;
            while let Some(byte) = bytes.get(index) {
                match byte {
                    b'"' => {
                        let Some(end) = string_end(bytes, index) else {
                            return fields;
                        };
                        index = end;
                        continue;
                    }
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' if depth > 0 => depth -= 1,
                    b',' | b'}' if depth == 0 => break,
                    _ => {}
                }
                index += 1;
            }
        }
    }
    fields
}

struct Translator {
    decoder: SseDecoder,
    id: String,
    model: String,
    created: i64,
    started: bool,
    done: bool,
    saw_answer: bool,
    current_id: String,
    current: String,
    emitted_text: String,
    emitted_thinking: String,
    native_reasoning: bool,
    collect: bool,
    text: String,
    reasoning: String,
    tools: Tools,
    tool_calls: Vec<Value>,
    pending: Option<(String, Pending)>,
    capture_web: bool,
    web_results: BTreeMap<String, (Value, String)>,
    input_tokens: u64,
    output_characters: u64,
}

impl Translator {
    fn new(
        model: String,
        collect: bool,
        tools: Tools,
        input_tokens: u64,
    ) -> Result<Self, ChannelError> {
        Ok(Self {
            decoder: SseDecoder::new(LIMITS),
            id: format!("chatcmpl-{}", new_id()?),
            model,
            created: now_ms() / 1000,
            started: false,
            done: false,
            saw_answer: false,
            current_id: String::new(),
            current: String::new(),
            emitted_text: String::new(),
            emitted_thinking: String::new(),
            native_reasoning: false,
            collect,
            text: String::new(),
            reasoning: String::new(),
            tools,
            tool_calls: Vec::new(),
            pending: None,
            capture_web: false,
            web_results: BTreeMap::new(),
            input_tokens,
            output_characters: 0,
        })
    }

    fn chunk(&self, delta: Value, finish: Option<&str>, out: &mut Vec<u8>) {
        let value = json!({"id":self.id,"object":"chat.completion.chunk","created":self.created,
            "model":self.model,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
        out.extend_from_slice(format!("data: {value}\n\n").as_bytes());
    }

    fn delta(&mut self, thinking: bool, text: &str, out: &mut Vec<u8>) {
        if text.is_empty() {
            return;
        }
        self.output_characters += text.chars().count() as u64;
        if !self.started {
            self.chunk(json!({"role":"assistant","content":""}), None, out);
            self.started = true;
        }
        if self.collect {
            if thinking {
                self.reasoning.push_str(text);
            } else {
                self.text.push_str(text);
            }
        }
        self.chunk(
            if thinking {
                json!({"reasoning_content":text})
            } else {
                json!({"content":text})
            },
            None,
            out,
        );
    }

    fn project(&mut self, committed: bool, out: &mut Vec<u8>) -> Result<(), ChannelError> {
        let fields = partial_strings(json_content(&self.current));
        if !self.native_reasoning
            && let Some(thinking) = fields.get("thinking")
        {
            let delta = thinking
                .strip_prefix(&self.emitted_thinking)
                .ok_or_else(|| invalid("reasoning was rewritten during streaming"))?
                .to_owned();
            self.delta(true, &delta, out);
            self.emitted_thinking = thinking.clone();
        }
        // The Agent may retract and regenerate an answer. Keep answer text
        // pending until it is committed, since Chat has no retraction event.
        if !committed {
            return Ok(());
        }
        let key = match fields.get("action").map(String::as_str) {
            Some("completion") => "taskCompletion",
            Some("ask") => "question",
            _ => return Ok(()),
        };
        if let Some(text) = fields.get(key) {
            let delta = text
                .strip_prefix(&self.emitted_text)
                .ok_or_else(|| invalid("answer was rewritten during streaming"))?
                .to_owned();
            self.delta(false, &delta, out);
            self.emitted_text = text.clone();
        }
        Ok(())
    }

    fn finish_message(&mut self, out: &mut Vec<u8>) -> Result<(), ChannelError> {
        if self.current.is_empty() {
            return Ok(());
        }
        let value: Value = serde_json::from_str(json_content(&self.current))
            .map_err(|_| invalid("incomplete or invalid Agent JSON in SSE content"))?;
        let key = match value["action"].as_str() {
            Some("completion") => "taskCompletion",
            Some("ask") => "question",
            Some("review") => {
                self.project(false, out)?;
                let (call, pending) = self.tools.review(&value["tool"], &self.current_id)?;
                if !self.started {
                    self.chunk(json!({"role":"assistant","content":""}), None, out);
                    self.started = true;
                }
                let mut delta = call.clone();
                delta["index"] = json!(0);
                self.chunk(json!({"tool_calls":[delta]}), None, out);
                self.output_characters += call["function"]["arguments"]
                    .as_str()
                    .expect("serialized arguments")
                    .chars()
                    .count() as u64;
                self.pending = Some((
                    call["id"].as_str().expect("generated call ID").to_owned(),
                    pending,
                ));
                self.tool_calls = vec![call];
                self.saw_answer = true;
                return Ok(());
            }
            _ => return Err(invalid("unsupported Agent response action")),
        };
        if !value[key].is_string() {
            return Err(invalid("Agent response has no text answer"));
        }
        self.project(true, out)?;
        self.saw_answer = true;
        Ok(())
    }

    fn event(&mut self, frame: SseFrame, out: &mut Vec<u8>) -> Result<(), ChannelError> {
        if self.done {
            return Ok(());
        }
        let SseFrame::Event(event) = frame else {
            if self.capture_web {
                self.done = true;
                return Ok(());
            }
            self.finish_message(out)?;
            if !self.saw_answer {
                return Err(invalid("stream completed without an answer"));
            }
            if !self.started {
                self.chunk(json!({"role":"assistant","content":""}), None, out);
                self.started = true;
            }
            self.chunk(json!({}), Some(self.finish_reason()), out);
            let usage = json!({"id":self.id,"object":"chat.completion.chunk","created":self.created,
                "model":self.model,"choices":[],"usage":self.usage()});
            out.extend_from_slice(format!("data: {usage}\n\n").as_bytes());
            out.extend_from_slice(b"data: [DONE]\n\n");
            self.done = true;
            return Ok(());
        };
        if let Some(error) = event.data.strip_prefix("[ERROR]:") {
            return Err(invalid(error.trim()));
        }
        if event.data.starts_with("[ABORTED]") {
            return Err(invalid("upstream aborted generation"));
        }
        let value: Value =
            serde_json::from_str(&event.data).map_err(|_| invalid("invalid SSE JSON"))?;
        if let Some(error) = value.get("error") {
            return Err(invalid(
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("upstream error"),
            ));
        }
        let id = value["id"].as_str().unwrap_or(&self.current_id).to_owned();
        if id.starts_with("heartbeat-") {
            return Ok(());
        }
        if let Some(removed) = id.strip_prefix("remove-") {
            if removed == self.current_id {
                self.current.clear();
                self.emitted_text.clear();
                self.emitted_thinking.clear();
                self.native_reasoning = false;
            }
            return Ok(());
        }
        if value["type"] == "tool" {
            // Server-executed OrcaTerm tools are not client function calls.
            // Their result is consumed by the Agent before its final answer.
            if self.capture_web
                && matches!(
                    value["tool"]["name"].as_str(),
                    Some("fetch" | "cloud_ai_assistant_search")
                )
            {
                let entry = self
                    .web_results
                    .entry(id)
                    .or_insert_with(|| (value["tool"].clone(), String::new()));
                if let Some(content) = value["content"].as_str() {
                    if entry.1.len().saturating_add(content.len()) > MAX_MESSAGE {
                        return Err(invalid("web tool result exceeds the size limit"));
                    }
                    entry.1.push_str(content);
                }
            }
            return Ok(());
        }
        if self.capture_web {
            return Ok(());
        }
        if !id.is_empty() && id != self.current_id {
            self.current.clear();
            self.emitted_text.clear();
            self.emitted_thinking.clear();
            self.native_reasoning = false;
            self.current_id = id;
        }
        if let Some(reasoning) = value["reasoningContent"].as_str().filter(|s| !s.is_empty()) {
            // Prefer native reasoning when it precedes the Agent object's own
            // thinking field, as the desktop does; do not duplicate both.
            if self.emitted_thinking.is_empty() {
                self.native_reasoning = true;
                self.delta(true, reasoning, out);
            }
        }
        if let Some(content) = value["content"].as_str() {
            if self.current.len().saturating_add(content.len()) > MAX_MESSAGE {
                return Err(invalid("Agent message exceeds the size limit"));
            }
            self.current.push_str(content);
            if self.current.starts_with("{ERROR}:") {
                return Err(invalid("upstream reported an Agent error"));
            }
            self.project(false, out)?;
        }
        Ok(())
    }

    fn push(&mut self, bytes: &[u8]) -> Result<Bytes, ChannelError> {
        let frames = self
            .decoder
            .push(bytes)
            .map_err(|e| invalid(e.to_string()))?;
        let mut out = Vec::new();
        for frame in frames {
            self.event(frame, &mut out)?;
        }
        Ok(out.into())
    }

    fn finish(&mut self) -> Result<Bytes, ChannelError> {
        // OrcaTerm ends with `data: [DONE]` without a line terminator. Its
        // desktop client flushes the last line at EOF; do the same locally
        // while retaining the shared decoder's strict SSE framing.
        let mut out = if self.done {
            Vec::new()
        } else {
            self.push(b"\n\n")?.to_vec()
        };
        self.decoder.finish().map_err(|e| invalid(e.to_string()))?;
        if !self.done {
            // The desktop also accepts clean EOF after a complete Agent
            // object. event() still validates the entire JSON and action.
            self.event(SseFrame::Done, &mut out)?;
        }
        Ok(out.into())
    }

    async fn persist(&mut self, state: &dyn ChannelState) -> Result<(), ChannelError> {
        if let Some((id, pending)) = self.pending.take() {
            pending.save(state, &id).await?;
        }
        Ok(())
    }

    fn message(&self) -> Value {
        let mut message = json!({"role":"assistant","content":self.text});
        if !self.reasoning.is_empty() {
            message["reasoning_content"] = json!(self.reasoning);
        }
        if !self.tool_calls.is_empty() {
            message["tool_calls"] = json!(self.tool_calls);
        }
        json!({"id":self.id,"object":"chat.completion","created":self.created,"model":self.model,
            "choices":[{"index":0,"message":message,"finish_reason":self.finish_reason()}],"usage":self.usage()})
    }

    fn usage(&self) -> Value {
        let output_tokens = self.output_characters.div_ceil(2);
        json!({"prompt_tokens":self.input_tokens,"completion_tokens":output_tokens,
            "total_tokens":self.input_tokens + output_tokens})
    }

    fn finish_reason(&self) -> &'static str {
        if self.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        }
    }
}

fn byte_stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
    }
}

fn translate(body: HttpBody, translator: Translator, state: Arc<dyn ChannelState>) -> ByteStream {
    Box::pin(futures_util::stream::unfold(
        Some((byte_stream(body), translator, state)),
        |slot| async move {
            let (mut upstream, mut translator, state) = slot?;
            loop {
                match upstream.next().await {
                    Some(Ok(chunk)) => {
                        let bytes = match translator.push(&chunk) {
                            Ok(bytes) => bytes,
                            Err(error) => return Some((Err(error.to_string().into()), None)),
                        };
                        if let Err(error) = translator.persist(state.as_ref()).await {
                            return Some((Err(error.to_string().into()), None));
                        }
                        if translator.done {
                            return Some((Ok(bytes), None));
                        }
                        if bytes.is_empty() {
                            continue;
                        }
                        return Some((Ok(bytes), Some((upstream, translator, state))));
                    }
                    Some(Err(error)) => return Some((Err(error), None)),
                    None => {
                        let bytes = match translator.finish() {
                            Ok(bytes) => bytes,
                            Err(error) => return Some((Err(error.to_string().into()), None)),
                        };
                        if let Err(error) = translator.persist(state.as_ref()).await {
                            return Some((Err(error.to_string().into()), None));
                        }
                        return Some((Ok(bytes), None));
                    }
                }
            }
        },
    ))
}

pub(super) async fn invoke(
    ctx: OperationContext<'_>,
    streaming: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    if ctx.dialect != gproxy_protocol::Dialect::OpenAiChat {
        return Err(ChannelError::UnsupportedOperation(OperationKey {
            operation: if streaming {
                Operation::StreamGenerateContent
            } else {
                Operation::GenerateContent
            },
            dialect: ctx.dialect,
        }));
    }
    let HttpBody::Bytes(bytes) = &ctx.request.body else {
        return Err(request::bad("requires a buffered JSON request"));
    };
    let input = serde_json::from_slice(bytes).map_err(|_| request::bad("invalid JSON"))?;
    let chat = request::build(&input)?;
    drop(input);
    let state = ctx.state.clone();
    let (mut response, tools, model, input_tokens) = send_chat(ctx, chat, streaming).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let mut translator = Translator::new(model, !streaming, tools, input_tokens)?;
    response.body = if streaming {
        HttpBody::Stream(translate(response.body, translator, state))
    } else {
        let mut upstream = byte_stream(response.body);
        while let Some(chunk) = upstream.next().await {
            translator.push(&chunk.map_err(|e| invalid(e.to_string()))?)?;
            if translator.done {
                break;
            }
        }
        translator.finish()?;
        translator.persist(state.as_ref()).await?;
        HttpBody::Bytes(translator.message().to_string().into())
    };
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.remove(header::CONTENT_ENCODING);
    response.headers.remove(header::SET_COOKIE);
    response.headers.remove("x-ot-token-refreshed");
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

async fn send_chat(
    ctx: OperationContext<'_>,
    mut chat: request::ChatRequest,
    streaming: bool,
) -> Result<(WireResponse<HttpBody>, Tools, String, u64), ChannelError> {
    let model = chat.body["user"]["setting"]["model"]
        .as_str()
        .expect("validated model")
        .to_owned();
    let settings = models::settings(ctx.client.as_ref(), ctx.state.as_ref(), &model).await?;
    if !chat.uploads.is_empty() && !settings.images {
        return Err(request::bad("this model does not support images"));
    }
    let (thinking, params) = settings.reasoning(chat.effort.as_deref())?;
    chat.body["user"]["setting"]["enableThinking"] = json!(thinking);
    chat.body["user"]["setting"]["modelDesc"]["kwargs"]
        .as_object_mut()
        .expect("built kwargs")
        .extend(params);
    // OAuth handoff tokens must not be lost when the post-login profile call
    // fails. Retry that independent lookup using the already saved token.
    let discovered = if auth::fact(ctx.credential, "user_id").is_none() {
        let config = OrcaTermConfig::from_view(ctx.provider)?;
        let fields = auth::identity(ctx.client.as_ref(), &config, ctx.credential.secret).await?;
        let mut secret = ctx.credential.secret.clone();
        secret["provider_fields"] = json!(fields);
        Some(secret)
    } else {
        None
    };
    let credential = match discovered.as_ref() {
        Some(secret) => CredentialView {
            secret,
            ..ctx.credential
        },
        None => ctx.credential,
    };
    chat.body["user"]["id"] =
        json!(auth::fact(credential, "user_id").ok_or(ChannelError::InvalidCredential)?);
    let origin = OrcaTermConfig::from_view(ctx.provider)?
        .base_url(ctx.provider)
        .to_owned();
    chat.tools
        .prepare(ctx.state.as_ref(), &mut chat.body, chat.tool_result, origin)
        .await?;
    images::upload_all(
        &mut chat.body,
        chat.uploads,
        ctx.provider,
        credential,
        ctx.client.as_ref(),
        ctx.state.as_ref(),
    )
    .await?;
    if chat.body["input"]["type"] == "review" {
        let input = chat.body["input"].as_object_mut().expect("built input");
        input.remove("historyMessages");
        input.remove("input");
    }
    let prepared = request::prepare(
        PrepareContext {
            provider: ctx.provider,
            credential,
            operation: OperationKey {
                operation: if streaming {
                    Operation::StreamGenerateContent
                } else {
                    Operation::GenerateContent
                },
                dialect: gproxy_protocol::Dialect::OpenAiChat,
            },
            request: ctx.request,
            endpoint_override: ctx.endpoint_override,
        },
        chat.body,
    )?;
    let mut response = ctx.client.send(prepared).await?;
    response.headers.remove(header::SET_COOKIE);
    response.headers.remove("x-ot-token-refreshed");
    // LightAI's RPM limiter returns HTTP 200 with a JSON error, instead of
    // opening an SSE stream. Surface the actual status before conversion so
    // the host and client can apply their normal rate-limit backoff.
    if response.status.is_success()
        && response.headers.get(header::CONTENT_TYPE).is_some_and(|v| {
            v.to_str()
                .is_ok_and(|v| v.split(';').next() == Some("application/json"))
        })
    {
        let bytes = read_body(response.body).await?;
        let error: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid JSON error response"))?;
        let status = error["code"]
            .as_u64()
            .and_then(|code| u16::try_from(code).ok())
            .and_then(|code| http::StatusCode::from_u16(code).ok())
            .filter(|status| status.is_client_error() || status.is_server_error())
            .ok_or_else(|| invalid("expected an Agent SSE stream"))?;
        response.status = status;
        response.body = HttpBody::Bytes(json!({"error":{
            "message":error["message"].as_str().unwrap_or("OrcaTerm upstream error"),
            "code":error["code"],
            "type":if status == http::StatusCode::TOO_MANY_REQUESTS {"rate_limit_error"} else {"upstream_error"}
        }}).to_string().into());
        response.headers.remove(header::CONTENT_LENGTH);
        if status == http::StatusCode::TOO_MANY_REQUESTS
            && let Some(reset) = response.headers.get("x-ratelimit-reset").cloned()
        {
            response.headers.insert(header::RETRY_AFTER, reset);
        }
    }
    Ok((response, chat.tools, model, chat.input_tokens))
}

/// Codex's standalone web operation uses OrcaTerm's read-only server tools.
/// Return their actual result text, not an unverified model-generated answer.
pub(super) async fn search(
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let HttpBody::Bytes(bytes) = &ctx.request.body else {
        return Err(request::bad("requires a buffered search request"));
    };
    let input: gproxy_protocol::wire::openai::web_search::WebSearchRequestBody =
        serde_json::from_slice(bytes).map_err(|_| request::bad("invalid search request"))?;
    let model = input.model.clone();
    let prompt = gproxy_protocol::transform::web_search::to_generation(input, model.clone())
        .map_err(|error| request::bad(&error.to_string()))?;
    let content = match prompt.input {
        Some(gproxy_protocol::wire::openai::responses::Input::Text(content)) => content,
        _ => unreachable!("web search converter emits text"),
    };
    let instructions = prompt.instructions.flatten().unwrap_or_default();
    let mut chat = request::build(&json!({"model":model,"messages":[
        {"role":"system","content":instructions},
        {"role":"user","content":content}
    ]}))?;
    chat.body["input"]["emphasisPrompt"] = json!(instructions);
    chat.body["user"]["setting"]["approvedTools"] = json!(["cloud_ai_assistant_search"]);
    chat.body["user"]["setting"]["approvedMCPTools"] = json!(["mcp-server-web-fetch/fetch"]);
    let (mut response, tools, model, input_tokens) = send_chat(ctx, chat, false).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let mut translator = Translator::new(model, false, tools, input_tokens)?;
    translator.capture_web = true;
    let mut upstream = byte_stream(response.body);
    while let Some(chunk) = upstream.next().await {
        translator.push(&chunk.map_err(|error| invalid(error.to_string()))?)?;
        if translator.done {
            break;
        }
    }
    translator.finish()?;
    if translator.web_results.is_empty() {
        return Err(invalid(
            "web request completed without executing a search or fetch tool",
        ));
    }
    let results = translator
        .web_results
        .into_values()
        .map(|(tool, content)| {
            let result = serde_json::from_str::<Value>(&content).unwrap_or(json!(content));
            json!({"tool":tool,"result":result})
        })
        .collect::<Vec<_>>();
    let output = results
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    response.body = HttpBody::Bytes(
        json!({"output":output,"results":results})
            .to_string()
            .into(),
    );
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.remove(header::CONTENT_ENCODING);
    response.headers.remove(header::SET_COOKIE);
    response.headers.remove("x-ot-token-refreshed");
    response.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}
