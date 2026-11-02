//! OpenAI Chat is a distinct dialect because its audio history is ID-based.
use super::*;
use gproxy_protocol::openai::chat::{content as c, request as q, response as r, tools as t};
use std::collections::BTreeMap;

#[derive(Default, Serialize, Deserialize)]
struct Envelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chat: Option<Metadata>,
}
#[derive(Default, Serialize, Deserialize)]
struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<r::ResponseMessageOptions>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    refusal: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    legacy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    call_type: Option<c::ChatToolCallType>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    rest: Rest,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    function_rest: Rest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audio: Option<r::ResponseAudioOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_source: Option<ReasoningSource>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReasoningSource {
    Content,
    Reasoning,
    Details,
}
fn metadata(block: &ContentBlock<impl Sized>) -> Result<Option<Metadata>> {
    Ok(read_metadata::<Envelope>(block, Vendor::Openai)?.and_then(|v| v.chat))
}
fn block(content: Content, meta: Metadata) -> Result<Block> {
    with_metadata(content, Vendor::Openai, Envelope { chat: Some(meta) })
}

pub fn render(request: &GenerateRequest, streaming: bool) -> Result<q::GenerateContentRequestBody> {
    render_at(request, streaming, crate::assets::clock()?)
}
pub fn render_at(
    request: &GenerateRequest,
    streaming: bool,
    now: u64,
) -> Result<q::GenerateContentRequestBody> {
    let opts: q::GenerateContentRequestBodyOptions = options(request)?;
    let mut body =
        q::GenerateContentRequestBody::builder(Vec::new(), request.model.id.clone()).build();
    opts.apply_to(&mut body);
    body.model = request.model.id.clone();
    body.stream = Some(Some(streaming));
    let mut messages = Vec::new();
    for message in &request.messages {
        if message.role == Role::Assistant {
            let mut value = c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
            // Apply the message envelope once before portable blocks so moving
            // its owning block cannot overwrite already rendered content.
            for block in &message.content {
                if let Some(native) = metadata(block)?.and_then(|m| m.message) {
                    value.rest.extend(native.rest);
                    value.reasoning_details = native.reasoning_details;
                    value.reasoning_content = native.reasoning_content;
                    value.reasoning = native.reasoning;
                    value.content = Some(None);
                    value.refusal = Some(None);
                    value.audio = native.audio.map(|_| None);
                }
            }
            let mut text = Vec::new();
            let mut parts = false;
            for content in &message.content {
                let meta = metadata(content)?.unwrap_or_default();
                match &content.content {
                    Content::Text(t) => {
                        if meta.refusal {
                            value.refusal = Some(Some(t.text.clone()));
                        } else {
                            let mut part =
                                c::TextPart::builder(c::TextPartType::Text, t.text.clone()).build();
                            part.prompt_cache_breakpoint = media::chat_cache(content)?;
                            parts |= content.cache.is_some();
                            text.push(c::AssistantContentPart::Text(part));
                        }
                    }
                    Content::Reasoning(reasoning) => match meta.reasoning_source {
                        Some(ReasoningSource::Reasoning) => {
                            value.reasoning = Some(Some(reasoning.text.clone()))
                        }
                        Some(ReasoningSource::Details) => {}
                        Some(ReasoningSource::Content) | None => {
                            value.reasoning_content = Some(Some(reasoning.text.clone()))
                        }
                    },
                    Content::Audio(audio) => {
                        if let Some(native) = meta.audio {
                            if let (Some(expiry), Some(id)) = (native.expires_at, native.id)
                                && expiry > 0
                                && count(expiry)? > now
                            {
                                value.audio = Some(Some(
                                    c::AssistantAudio::builder(id).rest(native.rest).build(),
                                ));
                                continue;
                            }
                        }
                        let transcript=audio.transcript.as_ref().ok_or_else(||Error::invalid("expired or foreign assistant audio requires a transcript for Chat continuation"))?;
                        text.push(c::AssistantContentPart::Text(
                            c::TextPart::builder(c::TextPartType::Text, transcript.clone()).build(),
                        ));
                        value.audio = None;
                    }
                    Content::ToolCall(call) => {
                        if call.provider_executed {
                            continue;
                        }
                        let function =
                            c::FunctionCall::builder(call.arguments.clone(), call.name.clone())
                                .rest(meta.function_rest)
                                .build();
                        if meta.legacy {
                            value.function_call = Some(Some(function));
                        } else {
                            let mut tool = c::ChatToolCall::builder(
                                call.id.clone(),
                                function,
                                c::ChatToolCallType::Function,
                            )
                            .rest(meta.rest)
                            .build();
                            tool.type_ = meta.call_type.unwrap_or(c::ChatToolCallType::Function);
                            value
                                .tool_calls
                                .get_or_insert_with(Vec::new)
                                .push(c::MessageToolCall::Function(tool));
                        }
                    }
                    Content::Opaque => {
                        #[derive(Deserialize)]
                        struct Custom {
                            custom: Option<c::CustomToolCall>,
                        }
                        if let Some(custom) =
                            read_metadata::<Custom>(content, Vendor::Openai)?.and_then(|v| v.custom)
                        {
                            value
                                .tool_calls
                                .get_or_insert_with(Vec::new)
                                .push(c::MessageToolCall::Custom(custom));
                        }
                    }
                    _ => {
                        return Err(Error::invalid(
                            "Chat assistant history supports text, reasoning, tool calls and audio references",
                        ));
                    }
                }
            }
            if !text.is_empty() {
                value.content = Some(Some(if parts {
                    c::AssistantContent::Parts(text)
                } else {
                    c::AssistantContent::Text(
                        text.into_iter()
                            .filter_map(|p| match p {
                                c::AssistantContentPart::Text(t) => Some(t.text),
                                _ => None,
                            })
                            .collect(),
                    )
                }));
            }
            messages.push(c::ChatMessage::Assistant(value));
        } else if message.role == Role::Tool {
            for content in &message.content {
                if let Content::ToolResult(result) = &content.content {
                    if result.provider_executed {
                        continue;
                    }
                    let output = tool_output(result)?;
                    messages.push(c::ChatMessage::Tool(
                        c::ToolMessage::builder(c::ToolRole::Tool, output, result.id.clone())
                            .build(),
                    ));
                } else {
                    return Err(Error::invalid(
                        "Chat tool messages require tool-result blocks",
                    ));
                }
            }
        } else {
            let mut parts = Vec::new();
            for content in &message.content {
                parts.push(user_part(content)?);
            }
            if message.role == Role::System {
                let text = parts
                    .into_iter()
                    .map(|p| match p {
                        c::UserContentPart::Text(t) => Ok(t),
                        _ => Err(Error::invalid("Chat system messages require text")),
                    })
                    .collect::<Result<_>>()?;
                messages.push(c::ChatMessage::System(
                    c::SystemMessage::builder(c::SystemRole::System, c::TextContent::Parts(text))
                        .build(),
                ));
            } else {
                messages.push(c::ChatMessage::User(
                    c::UserMessage::builder(c::UserRole::User, c::UserContent::Parts(parts))
                        .build(),
                ));
            }
        }
    }
    body.messages = messages;
    if let Some(max) = request.settings.max_output_tokens {
        body.max_completion_tokens = Some(Some(max.into()));
    }
    if let Some(value) = request.settings.temperature {
        body.temperature = Some(Some(value));
    }
    if let Some(value) = request.settings.top_p {
        body.top_p = Some(Some(value));
    }
    if !request.settings.stop_sequences.is_empty() {
        body.stop = Some(Some(q::Stop::Sequences(
            request.settings.stop_sequences.clone(),
        )));
    }
    if !request.settings.output_modalities.is_empty() {
        body.modalities = Some(Some(
            request
                .settings
                .output_modalities
                .iter()
                .map(|m| match m {
                    Modality::Text => Ok(q::Modality::Text),
                    Modality::Audio => Ok(q::Modality::Audio),
                    Modality::Image => Err(Error::invalid("Chat does not support image output")),
                })
                .collect::<Result<_>>()?,
        ));
    }
    if let Some(speech) = &request.settings.speech {
        body.audio = Some(Some(
            q::ChatAudioParam::builder(
                q::AudioVoice::String(speech.voice.clone()),
                native_format(&speech.format)?,
            )
            .build(),
        ));
    }
    if !request.tools.is_empty() {
        body.tools = Some(
            request
                .tools
                .iter()
                .map(|tool| {
                    let parameters: Rest = serde_json::from_value(tool.input_schema.clone())
                        .map_err(Error::invalid)?;
                    let function = t::FunctionDefinition::builder(tool.name.clone())
                        .description(tool.description.clone())
                        .parameters(parameters)
                        .build();
                    Ok(t::ChatTool::Function(
                        t::FunctionTool::builder(t::FunctionToolType::Function, function).build(),
                    ))
                })
                .collect::<Result<_>>()?,
        );
    }
    if let Some(choice) = &request.settings.tool_choice {
        body.tool_choice = Some(match choice {
            ToolChoice::Auto => t::ToolChoice::Mode(t::ToolChoiceMode::Auto),
            ToolChoice::None => t::ToolChoice::Mode(t::ToolChoiceMode::None),
            ToolChoice::Required => t::ToolChoice::Mode(t::ToolChoiceMode::Required),
            ToolChoice::Tool { name } => t::ToolChoice::Function(
                t::NamedFunctionChoice::builder(
                    t::NamedFunctionChoiceType::Function,
                    t::NamedFunction::builder(name.clone()).build(),
                )
                .build(),
            ),
        });
    }
    if let Some(schema) = &request.settings.output_schema {
        let schema: Rest = serde_json::from_value(schema.clone()).map_err(Error::invalid)?;
        body.response_format = Some(q::ResponseFormat::JsonSchema(
            q::JsonSchemaResponseFormat::builder(
                q::JsonSchemaResponseType::JsonSchema,
                q::JsonSchemaFormat::builder("response".into())
                    .schema(schema)
                    .build(),
            )
            .build(),
        ));
    }
    Ok(body)
}
fn tool_output(result: &ToolResult) -> Result<c::TextContent> {
    Ok(match &result.output {
        ToolOutput::Json(Value::String(text)) => c::TextContent::Text(text.clone()),
        ToolOutput::Json(value) => c::TextContent::Text(value.to_string()),
        ToolOutput::Blocks(blocks) => {
            let mut parts = Vec::with_capacity(blocks.len());
            for block in blocks {
                let ToolResultContent::Text(text) = &block.content else {
                    return Err(Error::invalid(
                        "Chat tool messages require text output; use Responses for media",
                    ));
                };
                let mut part =
                    c::TextPart::builder(c::TextPartType::Text, text.text.clone()).build();
                part.prompt_cache_breakpoint = media::chat_cache(block)?;
                parts.push(part);
            }
            c::TextContent::Parts(parts)
        }
    })
}
fn user_part(block: &Block) -> Result<c::UserContentPart> {
    Ok(match &block.content {
        Content::Text(text) => {
            let mut value = c::TextPart::builder(c::TextPartType::Text, text.text.clone()).build();
            value.prompt_cache_breakpoint = media::chat_cache(block)?;
            c::UserContentPart::Text(value)
        }
        Content::Image(image) => {
            let source = crate::assets::source(block, &image.source, Vendor::Openai, false)?;
            let mut url =
                c::ImageUrl::builder(data_url(&source, image.media_type.as_deref())?).build();
            url.detail = media::chat_detail(image.detail);
            let mut value = c::ImagePart::builder(c::ImagePartType::ImageUrl, url).build();
            value.prompt_cache_breakpoint = media::chat_cache(block)?;
            c::UserContentPart::Image(value)
        }
        Content::Document(document) => {
            let (original, mime) = media::document_bytes(document)?;
            let source = crate::assets::source(block, &original, Vendor::Openai, true)?;
            chat_file(block, &source, mime, document.title.as_deref())?
        }
        Content::File(file) => {
            let source = crate::assets::source(block, &file.source, Vendor::Openai, true)?;
            chat_file(
                block,
                &source,
                file.media_type.as_deref(),
                file.filename.as_deref(),
            )?
        }
        Content::Audio(audio) => {
            let Asset::Inline { bytes } = &audio.source else {
                return Err(Error::invalid(
                    "Chat input_audio requires inline WAV or MP3 bytes",
                ));
            };
            let format = match &audio.format {
                AudioFormat::Encoded { media_type }
                    if matches!(media_type.as_str(), "audio/wav" | "audio/x-wav") =>
                {
                    c::AudioFormat::Wav
                }
                AudioFormat::Encoded { media_type }
                    if matches!(media_type.as_str(), "audio/mpeg" | "audio/mp3") =>
                {
                    c::AudioFormat::Mp3
                }
                _ => {
                    return Err(Error::invalid(
                        "Chat input audio supports WAV/MP3 only; no transcoding is performed",
                    ));
                }
            };
            let audio = c::InputAudio::builder(
                base64::engine::general_purpose::STANDARD.encode(bytes),
                format,
            )
            .build();
            let mut value =
                c::InputAudioPart::builder(c::InputAudioPartType::InputAudio, audio).build();
            value.prompt_cache_breakpoint = media::chat_cache(block)?;
            c::UserContentPart::InputAudio(value)
        }
        _ => return Err(Error::invalid("this content kind is not a Chat user input")),
    })
}
fn chat_file(
    block: &Block,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
) -> Result<c::UserContentPart> {
    let mut file = c::FileRef::builder().build();
    match source {
        Asset::Inline { .. } => {
            file.file_data = Some(data_url(source, mime)?);
            file.filename = filename.map(str::to_owned);
        }
        Asset::ProviderFile {
            vendor: Vendor::Openai,
            id,
        } => file.file_id = Some(id.clone()),
        _ => {
            return Err(Error::invalid(
                "Chat documents require inline bytes or an OpenAI file ID; prepare_assets can upload bytes",
            ));
        }
    }
    let mut part = c::FilePart::builder(c::FilePartType::File, file).build();
    part.prompt_cache_breakpoint = media::chat_cache(block)?;
    Ok(c::UserContentPart::File(part))
}
fn native_format(format: &AudioFormat) -> Result<q::ChatAudioFormat> {
    Ok(match format {
        AudioFormat::Pcm {
            sample_rate: 24000,
            channels: 1,
            ..
        } => q::ChatAudioFormat::Pcm16,
        AudioFormat::Encoded { media_type } => match media_type.as_str() {
            "audio/wav" => q::ChatAudioFormat::Wav,
            "audio/mpeg" | "audio/mp3" => q::ChatAudioFormat::Mp3,
            "audio/aac" => q::ChatAudioFormat::Aac,
            "audio/flac" => q::ChatAudioFormat::Flac,
            "audio/opus" => q::ChatAudioFormat::Opus,
            _ => return Err(Error::invalid("unsupported Chat output audio format")),
        },
        _ => return Err(Error::invalid("Chat PCM output is 24 kHz mono")),
    })
}
fn ir_format(format: q::ChatAudioFormat) -> AudioFormat {
    AudioFormat::Encoded {
        media_type: match format {
            q::ChatAudioFormat::Wav => "audio/wav",
            q::ChatAudioFormat::Mp3 => "audio/mpeg",
            q::ChatAudioFormat::Aac => "audio/aac",
            q::ChatAudioFormat::Flac => "audio/flac",
            q::ChatAudioFormat::Opus => "audio/opus",
            q::ChatAudioFormat::Pcm16 => return AudioFormat::pcm(24000, 1),
        }
        .into(),
    }
}
pub(crate) fn output_format(request: &GenerateRequest) -> Result<Option<AudioFormat>> {
    if let Some(speech) = &request.settings.speech {
        return Ok(Some(speech.format.clone()));
    }
    let options: q::GenerateContentRequestBodyOptions = options(request)?;
    Ok(options.audio.flatten().map(|audio| ir_format(audio.format)))
}
pub fn parse(
    response: r::GenerateContentResponseBody,
    audio_format: Option<AudioFormat>,
) -> Result<GenerateResult> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| Error::invalid("Chat response has no choice"))?;
    let finish_reason = finish_reason(choice.finish_reason);
    let content = parse_message(choice.message, audio_format)?
        .into_iter()
        .map(|(_, block)| block)
        .collect();
    Ok(GenerateResult {
        content,
        finish_reason,
        usage: parse_usage(response.usage.as_ref())?,
    })
}
pub(crate) fn parse_message(
    message: r::ResponseMessage,
    format: Option<AudioFormat>,
) -> Result<Vec<(u32, Block)>> {
    parse_message_inner(message, format, None)
}
fn parse_message_inner(
    mut message: r::ResponseMessage,
    format: Option<AudioFormat>,
    supplied_audio: Option<Block>,
) -> Result<Vec<(u32, Block)>> {
    let mut result = Vec::new();
    let visible = gproxy_protocol::openai::chat::visible_reasoning(
        &message.reasoning_content,
        &message.reasoning,
        &message.reasoning_details,
    );
    if let Some(text) = visible.filter(|s| !s.is_empty()) {
        let source = if message
            .reasoning_content
            .as_ref()
            .and_then(Option::as_ref)
            .is_some_and(|s| !s.is_empty())
        {
            message.reasoning_content = Some(Some(String::new()));
            ReasoningSource::Content
        } else if message
            .reasoning
            .as_ref()
            .and_then(Option::as_ref)
            .is_some()
        {
            message.reasoning = Some(Some(String::new()));
            ReasoningSource::Reasoning
        } else {
            ReasoningSource::Details
        };
        result.push((
            0,
            block(
                Content::Reasoning(Reasoning { text }),
                Metadata {
                    reasoning_source: Some(source),
                    ..Default::default()
                },
            )?,
        ));
    }
    let annotations = message
        .annotations
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(citations::parse_chat)
        .collect::<Result<_>>()?;
    if let Some(text) = message.content.take() {
        result.push((
            1,
            block(
                Content::Text(Text {
                    text,
                    citations: annotations,
                }),
                Metadata::default(),
            )?,
        ));
    }
    if let Some(refusal) = message.refusal.take() {
        result.push((
            2,
            block(
                Content::Text(Text {
                    text: refusal,
                    citations: Vec::new(),
                }),
                Metadata {
                    refusal: true,
                    ..Default::default()
                },
            )?,
        ));
    }
    if let Some(Some(mut audio)) = message.audio.take() {
        let source = Asset::Inline {
            bytes: base64::engine::general_purpose::STANDARD
                .decode(&audio.data)
                .map_err(Error::invalid)?,
        };
        let transcript = Some(std::mem::take(&mut audio.transcript));
        audio.data.clear();
        let mut native = r::ResponseAudioOptions::from(audio);
        native.data = None;
        native.transcript = None;
        result.push((
            3,
            block(
                Content::Audio(Audio {
                    source,
                    format: format.ok_or_else(|| {
                        Error::invalid("Chat audio decoding requires the requested audio format")
                    })?,
                    transcript,
                }),
                Metadata {
                    audio: Some(native),
                    ..Default::default()
                },
            )?,
        ));
    }
    if let Some(audio) = supplied_audio {
        result.push((3, audio));
    }
    for (index, call) in message
        .tool_calls
        .take()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        match call {
            c::MessageToolCall::Function(value) => {
                let content = Content::ToolCall(ToolCall {
                    id: value.id,
                    name: value.function.name,
                    arguments: value.function.arguments,
                    provider_executed: false,
                });
                result.push((
                    4 + index as u32,
                    block(
                        content,
                        Metadata {
                            call_type: Some(value.type_),
                            rest: value.rest,
                            function_rest: value.function.rest,
                            ..Default::default()
                        },
                    )?,
                ));
            }
            c::MessageToolCall::Custom(value) => {
                #[derive(Serialize)]
                struct Custom {
                    custom: c::CustomToolCall,
                }
                result.push((
                    4 + index as u32,
                    with_metadata(Content::Opaque, Vendor::Openai, Custom { custom: value })?,
                ));
            }
        }
    }
    if let Some(value) = message.function_call.take() {
        result.push((
            4,
            block(
                Content::ToolCall(ToolCall {
                    id: "legacy_function".into(),
                    name: value.name,
                    arguments: value.arguments,
                    provider_executed: false,
                }),
                Metadata {
                    legacy: true,
                    function_rest: value.rest,
                    ..Default::default()
                },
            )?,
        ));
    }
    let mut native = r::ResponseMessageOptions::from(message);
    native.content = None;
    native.refusal = None;
    native.audio = None;
    native.tool_calls = None;
    native.function_call = None;
    native.annotations = None;
    if let Some((_, first)) = result.first_mut() {
        let mut meta = metadata(first)?.unwrap_or_default();
        meta.message = Some(native);
        first
            .provider_metadata
            .entry(Vendor::Openai)
            .or_default()
            .extend(encode_rest(Envelope { chat: Some(meta) })?);
    }
    Ok(result)
}
pub(crate) fn finish_reason(reason: r::FinishReason) -> FinishReason {
    match reason {
        r::FinishReason::Stop => FinishReason::Stop,
        r::FinishReason::Length => FinishReason::Length,
        r::FinishReason::ToolCalls | r::FinishReason::FunctionCall => FinishReason::ToolCalls,
        r::FinishReason::ContentFilter => FinishReason::ContentBlocked,
    }
}
pub(crate) fn parse_usage(usage: Option<&r::Usage>) -> Result<Usage> {
    let Some(v) = usage else {
        return Ok(Usage::default());
    };
    Ok(Usage {
        input_tokens: Some(count(v.prompt_tokens)?),
        output_tokens: Some(count(v.completion_tokens)?),
        reasoning_tokens: optional_count_chat(
            v.completion_tokens_details
                .as_ref()
                .and_then(|v| v.reasoning_tokens),
        )?,
        cache_read_tokens: optional_count_chat(
            v.prompt_tokens_details
                .as_ref()
                .and_then(|v| v.cached_tokens),
        )?,
        cache_write_tokens: optional_count_chat(
            v.prompt_tokens_details
                .as_ref()
                .and_then(|v| v.cache_write_tokens),
        )?,
    })
}
fn optional_count_chat(value: Option<i64>) -> Result<Option<u64>> {
    value.map(count).transpose()
}

/// Native Chat chunks do not contain a complete audio object. Accumulate their
/// observed fields without inventing an audio ID or an expiration timestamp.
pub(crate) struct StreamState {
    message: r::ResponseMessage,
    calls: BTreeMap<u32, c::ChatToolCall>,
    audio_seen: bool,
    audio: Vec<u8>,
    transcript: String,
    audio_id: Option<String>,
    expires: Option<i64>,
    audio_rest: Rest,
    format: Option<AudioFormat>,
    reason: Option<r::FinishReason>,
    usage: Usage,
}
impl StreamState {
    pub(crate) fn new(format: Option<AudioFormat>) -> Self {
        Self {
            message: r::ResponseMessage::builder(None, None, r::ResponseRole::Assistant).build(),
            calls: BTreeMap::new(),
            audio_seen: false,
            audio: Vec::new(),
            transcript: String::new(),
            audio_id: None,
            expires: None,
            audio_rest: Rest::new(),
            format,
            reason: None,
            usage: Usage::default(),
        }
    }
    pub(crate) fn push(&mut self, data: &str) -> Result<Vec<crate::stream::Event>> {
        use crate::stream::{BlockId, Event};
        use gproxy_protocol::openai::chat::stream as s;
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Record {
            Error { error: s::StreamError },
            Chunk(s::ChatCompletionChunk),
        }
        let chunk = match serde_json::from_str::<Record>(data).map_err(Error::invalid)? {
            Record::Error { error } => {
                let _ = error;
                return Err(Error::from_response(
                    200,
                    &http::HeaderMap::new(),
                    data.into(),
                ));
            }
            Record::Chunk(chunk) => chunk,
        };
        let mut out = Vec::new();
        if let Some(choice) = chunk.choices.into_iter().next() {
            let delta = choice.delta;
            if let Some(Some(text)) = delta.content {
                self.message
                    .content
                    .get_or_insert_with(String::new)
                    .push_str(&text);
                out.push(Event::TextDelta {
                    id: BlockId { item: 1, part: 0 },
                    delta: text,
                });
            }
            if let Some(Some(text)) = delta.refusal {
                self.message
                    .refusal
                    .get_or_insert_with(String::new)
                    .push_str(&text);
                out.push(Event::TextDelta {
                    id: BlockId { item: 2, part: 0 },
                    delta: text,
                });
            }
            if let Some(Some(text)) = delta.reasoning_content {
                self.message
                    .reasoning_content
                    .get_or_insert_with(|| Some(String::new()))
                    .get_or_insert_with(String::new)
                    .push_str(&text);
                out.push(Event::ReasoningDelta {
                    id: BlockId { item: 0, part: 0 },
                    delta: text,
                });
            }
            if let Some(Some(text)) = delta.reasoning {
                self.message
                    .reasoning
                    .get_or_insert_with(|| Some(String::new()))
                    .get_or_insert_with(String::new)
                    .push_str(&text);
                out.push(Event::ReasoningDelta {
                    id: BlockId { item: 0, part: 0 },
                    delta: text,
                });
            }
            if let Some(Some(details)) = delta.reasoning_details {
                self.message
                    .reasoning_details
                    .get_or_insert_with(|| Some(Vec::new()))
                    .get_or_insert_with(Vec::new)
                    .extend(details);
            }
            if let Some(annotations) = delta.annotations {
                for annotation in &annotations {
                    out.push(Event::Citation {
                        id: BlockId { item: 1, part: 0 },
                        citation: citations::parse_chat(annotation.clone())?,
                    });
                }
                self.message
                    .annotations
                    .get_or_insert_with(Vec::new)
                    .extend(annotations);
            }
            if let Some(audio) = delta.audio {
                self.audio_seen = true;
                if let Some(id) = audio.id {
                    self.audio_id = Some(id);
                }
                if let Some(expiry) = audio.expires_at {
                    self.expires = Some(expiry);
                }
                if let Some(data) = audio.data {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(Error::invalid)?;
                    self.audio.extend_from_slice(&bytes);
                    out.push(Event::AudioDelta {
                        id: BlockId { item: 3, part: 0 },
                        bytes,
                        format: self.format.clone().ok_or_else(|| {
                            Error::invalid("audio stream requires its requested format")
                        })?,
                    });
                }
                if let Some(text) = audio.transcript {
                    self.transcript.push_str(&text);
                    out.push(Event::TranscriptDelta {
                        id: BlockId { item: 3, part: 0 },
                        delta: text,
                    });
                }
                self.audio_rest.extend(audio.rest);
            }
            if let Some(Some(function)) = delta.function_call {
                let first = self.message.function_call.is_none();
                let value = self.message.function_call.get_or_insert_with(|| {
                    c::FunctionCall::builder(String::new(), String::new()).build()
                });
                if let Some(Some(name)) = function.name {
                    value.name.push_str(&name);
                }
                let id = BlockId { item: 4, part: 0 };
                if first {
                    out.push(Event::ToolCallStart {
                        id,
                        call_id: "legacy_function".into(),
                        name: value.name.clone(),
                        provider_executed: false,
                    });
                }
                if let Some(Some(arguments)) = function.arguments {
                    value.arguments.push_str(&arguments);
                    out.push(Event::ToolCallArgsDelta {
                        id,
                        delta: arguments,
                    });
                }
                value.rest.extend(function.rest);
            }
            if let Some(Some(calls)) = delta.tool_calls {
                for delta in calls {
                    let index: u32 = delta.index.try_into().map_err(Error::invalid)?;
                    let id = BlockId {
                        item: 4 + index,
                        part: 0,
                    };
                    let new = !self.calls.contains_key(&index);
                    let value = self.calls.entry(index).or_insert_with(|| {
                        c::ChatToolCall::builder(
                            String::new(),
                            c::FunctionCall::builder(String::new(), String::new()).build(),
                            c::ChatToolCallType::Function,
                        )
                        .build()
                    });
                    if let Some(Some(call_id)) = delta.id {
                        value.id.push_str(&call_id);
                    }
                    if let Some(Some(function)) = delta.function {
                        if let Some(Some(name)) = function.name {
                            value.function.name.push_str(&name);
                        }
                        if new {
                            out.push(Event::ToolCallStart {
                                id,
                                call_id: value.id.clone(),
                                name: value.function.name.clone(),
                                provider_executed: false,
                            });
                        }
                        if let Some(Some(arguments)) = function.arguments {
                            value.function.arguments.push_str(&arguments);
                            out.push(Event::ToolCallArgsDelta {
                                id,
                                delta: arguments,
                            });
                        }
                        value.function.rest.extend(function.rest);
                    }
                    value.type_ = c::ChatToolCallType::Function;
                    value.rest.extend(delta.rest);
                }
            }
            if let Some(Some(reason)) = choice.finish_reason {
                self.reason = Some(reason);
            }
            self.message.rest.extend(delta.rest);
        }
        if let Some(Some(value)) = chunk.usage {
            self.usage = Usage {
                input_tokens: Some(count(value.prompt_tokens)?),
                output_tokens: Some(count(value.completion_tokens)?),
                reasoning_tokens: optional_count_chat(
                    value
                        .completion_tokens_details
                        .as_ref()
                        .and_then(Option::as_ref)
                        .and_then(|v| v.reasoning_tokens.flatten()),
                )?,
                cache_read_tokens: optional_count_chat(
                    value
                        .prompt_tokens_details
                        .as_ref()
                        .and_then(Option::as_ref)
                        .and_then(|v| v.cached_tokens.flatten()),
                )?,
                cache_write_tokens: optional_count_chat(
                    value
                        .prompt_tokens_details
                        .as_ref()
                        .and_then(Option::as_ref)
                        .and_then(|v| v.cache_write_tokens.flatten()),
                )?,
            };
        }
        Ok(out)
    }
    pub(crate) fn finish(mut self) -> Result<Vec<crate::stream::Event>> {
        use crate::stream::{BlockId, Event};
        let reason = finish_reason(
            self.reason
                .ok_or_else(|| Error::invalid("Chat stream ended before finish reason"))?,
        );
        self.message.tool_calls = (!self.calls.is_empty()).then(|| {
            self.calls
                .into_values()
                .map(c::MessageToolCall::Function)
                .collect()
        });
        let audio = if self.audio_seen {
            let value = Audio {
                source: Asset::Inline { bytes: self.audio },
                format: self
                    .format
                    .clone()
                    .ok_or_else(|| Error::invalid("audio stream requires its requested format"))?,
                transcript: Some(self.transcript),
            };
            let native = r::ResponseAudioOptions {
                id: self.audio_id,
                data: None,
                expires_at: self.expires,
                transcript: None,
                rest: self.audio_rest,
            };
            Some(block(
                Content::Audio(value),
                Metadata {
                    audio: Some(native),
                    ..Default::default()
                },
            )?)
        } else {
            None
        };
        let blocks = parse_message_inner(self.message, self.format, audio)?;
        let mut events = Vec::new();
        for (item, block) in blocks {
            let id = BlockId { item, part: 0 };
            if matches!(block.content, Content::ToolCall(_)) {
                events.push(Event::ToolCallEnd { id });
            }
            if let Content::Text(text) = &block.content {
                for citation in &text.citations {
                    events.push(Event::Citation {
                        id,
                        citation: citation.clone(),
                    });
                }
            }
            events.push(Event::BlockEnd { id, block });
        }
        events.push(Event::Finish {
            reason,
            usage: self.usage,
        });
        Ok(events)
    }
}
