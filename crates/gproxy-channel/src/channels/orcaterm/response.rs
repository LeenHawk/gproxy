use super::{
    auth,
    config::{OrcaTermConfig, invalid, new_id, now_ms},
    images, models, request,
};
use crate::channel::{ChannelError, CredentialView, OperationContext, PrepareContext};
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::{ByteStream, Bytes},
};
use http::{HeaderValue, header};
use serde_json::{Value, json};
use std::collections::BTreeMap;

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
}

impl Translator {
    fn new(model: String, collect: bool) -> Result<Self, ChannelError> {
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

    fn project(&mut self, out: &mut Vec<u8>) -> Result<(), ChannelError> {
        let fields = partial_strings(&self.current);
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
        let value: Value = serde_json::from_str(&self.current)
            .map_err(|_| invalid("incomplete or invalid Agent JSON in SSE content"))?;
        let key = match value["action"].as_str() {
            Some("completion") => "taskCompletion",
            Some("ask") => "question",
            Some("review") => {
                return Err(invalid(
                    "upstream requested a tool; this channel does not execute OrcaTerm tools",
                ));
            }
            _ => return Err(invalid("unsupported Agent response action")),
        };
        if !value[key].is_string() {
            return Err(invalid("Agent response has no text answer"));
        }
        self.project(out)?;
        self.saw_answer = true;
        Ok(())
    }

    fn event(&mut self, frame: SseFrame, out: &mut Vec<u8>) -> Result<(), ChannelError> {
        if self.done {
            return Ok(());
        }
        let SseFrame::Event(event) = frame else {
            self.finish_message(out)?;
            if !self.saw_answer {
                return Err(invalid("stream completed without an answer"));
            }
            if !self.started {
                self.chunk(json!({"role":"assistant","content":""}), None, out);
                self.started = true;
            }
            self.chunk(json!({}), Some("stop"), out);
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
        if id.starts_with("remove-") {
            return Err(invalid("upstream retracted an Agent message"));
        }
        if value["type"] == "tool" {
            return Err(invalid("unexpected server tool output"));
        }
        if !id.is_empty() && id != self.current_id {
            self.finish_message(out)?;
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
            self.project(out)?;
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

    fn finish(&mut self) -> Result<(), ChannelError> {
        self.decoder.finish().map_err(|e| invalid(e.to_string()))?;
        if !self.done {
            return Err(invalid("stream ended without [DONE]"));
        }
        Ok(())
    }

    fn message(&self) -> Value {
        let mut message = json!({"role":"assistant","content":self.text});
        if !self.reasoning.is_empty() {
            message["reasoning_content"] = json!(self.reasoning);
        }
        json!({"id":self.id,"object":"chat.completion","created":self.created,"model":self.model,
            "choices":[{"index":0,"message":message,"finish_reason":"stop"}]})
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
                        Ok(bytes) if translator.done => return Some((Ok(bytes), None)),
                        Ok(bytes) if bytes.is_empty() => continue,
                        Ok(bytes) => return Some((Ok(bytes), Some((upstream, translator)))),
                        Err(error) => return Some((Err(error.to_string().into()), None)),
                    },
                    Some(Err(error)) => return Some((Err(error), None)),
                    None => {
                        return Some((
                            translator
                                .finish()
                                .map(|()| Bytes::new())
                                .map_err(|e| e.to_string().into()),
                            None,
                        ));
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
    let mut chat = request::build(&input)?;
    drop(input);
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
    images::upload_all(
        &mut chat.body,
        chat.uploads,
        ctx.provider,
        credential,
        ctx.client.as_ref(),
        ctx.state.as_ref(),
    )
    .await?;
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
                dialect: ctx.dialect,
            },
            request: ctx.request,
            endpoint_override: ctx.endpoint_override,
        },
        chat.body,
    )?;
    let mut response = ctx.client.send(prepared).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let mut translator = Translator::new(model, !streaming)?;
    response.body = if streaming {
        HttpBody::Stream(translate(response.body, translator))
    } else {
        let mut upstream = byte_stream(response.body);
        while let Some(chunk) = upstream.next().await {
            translator.push(&chunk.map_err(|e| invalid(e.to_string()))?)?;
            if translator.done {
                break;
            }
        }
        translator.finish()?;
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
