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
    message: Option<MessageMetadata>,
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
    audio: Option<AudioMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_source: Option<ReasoningSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reasoning_layout: Vec<ReasoningSegment>,
}
#[derive(Serialize, Deserialize)]
struct ReasoningSegment {
    index: usize,
    chars: usize,
}
#[derive(Default, Serialize, Deserialize)]
struct MessageMetadata {
    #[serde(
        default,
        deserialize_with = "crate::options::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    reasoning_details:
        Option<Option<Vec<gproxy_protocol::openai::chat::reasoning::ReasoningDetail>>>,
    #[serde(
        default,
        deserialize_with = "crate::options::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    reasoning_content: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::options::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    reasoning: Option<Option<String>>,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
struct AudioMetadata {
    id: Option<String>,
    expires_at: Option<i64>,
    #[serde(default, flatten)]
    rest: Rest,
}
// These are extensions in protocol's current Delta.rest. Decode that remainder
// once at its boundary; the mapping never indexes the native Delta as JSON.
#[derive(Default, Deserialize)]
struct StreamExtensions {
    audio: Option<AudioDelta>,
    annotations: Option<Vec<r::Annotation>>,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Deserialize)]
struct AudioDelta {
    id: Option<String>,
    expires_at: Option<i64>,
    data: Option<String>,
    transcript: Option<String>,
    #[serde(default, flatten)]
    rest: Rest,
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
    render_at(request, streaming, media::clock()?)
}
pub fn render_at(
    request: &GenerateRequest,
    streaming: bool,
    now: u64,
) -> Result<q::GenerateContentRequestBody> {
    let opts = &request.provider_options.openai.chat;
    let mut body =
        q::GenerateContentRequestBody::builder(Vec::new(), request.model.id.clone()).build();
    body.audio = opts.audio.clone();
    body.reasoning_effort = opts.reasoning_effort;
    body.response_format = opts.response_format.clone();
    body.tools = opts.tools.clone();
    body.parallel_tool_calls = opts.parallel_tool_calls;
    body.seed = opts.seed;
    body.store = opts.store;
    body.prompt_cache_key = opts.prompt_cache_key.clone();
    body.prompt_cache_options = opts.prompt_cache_options.clone();
    body.prompt_cache_retention = opts.prompt_cache_retention;
    body.service_tier = opts.service_tier;
    body.web_search_options = opts.web_search_options.clone();
    body.rest = request.provider_options.openai.rest.clone();
    body.rest.extend(opts.rest.clone());
    body.model = request.model.id.clone();
    body.stream = Some(Some(streaming));
    if streaming {
        let mut stream_options = opts
            .stream_options
            .clone()
            .flatten()
            .unwrap_or_else(|| q::StreamOptions::builder().build());
        stream_options.include_usage.get_or_insert(true);
        body.stream_options = Some(Some(stream_options));
    }
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
                            value
                                .reasoning
                                .get_or_insert_with(|| Some(String::new()))
                                .get_or_insert_with(String::new)
                                .push_str(&reasoning.text);
                        }
                        Some(ReasoningSource::Details) => render_reasoning_details(
                            &mut value,
                            &reasoning.text,
                            &meta.reasoning_layout,
                        )?,
                        Some(ReasoningSource::Content) | None => {
                            value
                                .reasoning_content
                                .get_or_insert_with(|| Some(String::new()))
                                .get_or_insert_with(String::new)
                                .push_str(&reasoning.text);
                        }
                    },
                    Content::Audio(audio) => {
                        if let Some(native) = meta.audio
                            && let (Some(expiry), Some(id)) = (native.expires_at, native.id)
                            && expiry > 0
                            && count(expiry)? > now
                        {
                            value.audio = Some(Some(
                                c::AssistantAudio::builder(id).rest(native.rest).build(),
                            ));
                            continue;
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
        let voice = q::AudioVoice::String(speech.voice.clone());
        let format = native_format(&speech.format)?;
        let rest = body
            .audio
            .take()
            .flatten()
            .map(|value| value.rest)
            .unwrap_or_default();
        body.audio = Some(Some(
            q::ChatAudioParam::builder(voice, format).rest(rest).build(),
        ));
    }
    if !request.tools.is_empty() {
        let tools = request
            .tools
            .iter()
            .map(|tool| {
                let parameters: Rest =
                    serde_json::from_value(tool.input_schema.clone()).map_err(Error::invalid)?;
                let function = t::FunctionDefinition::builder(tool.name.clone())
                    .description(tool.description.clone())
                    .parameters(parameters)
                    .build();
                Ok(t::ChatTool::Function(
                    t::FunctionTool::builder(t::FunctionToolType::Function, function).build(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        body.tools.get_or_insert_with(Vec::new).extend(tools);
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
        let mut format = match body.response_format.take() {
            Some(q::ResponseFormat::JsonSchema(format)) => format,
            other => {
                let rest = match other {
                    Some(q::ResponseFormat::Text(value)) => value.rest,
                    Some(q::ResponseFormat::JsonObject(value)) => value.rest,
                    _ => Rest::new(),
                };
                q::JsonSchemaResponseFormat::builder(
                    q::JsonSchemaResponseType::JsonSchema,
                    q::JsonSchemaFormat::builder("response".into()).build(),
                )
                .rest(rest)
                .build()
            }
        };
        format.json_schema.schema = Some(schema);
        body.response_format = Some(q::ResponseFormat::JsonSchema(format));
    }
    Ok(body)
}
fn render_reasoning_details(
    message: &mut c::AssistantMessage,
    text: &str,
    layout: &[ReasoningSegment],
) -> Result<()> {
    use gproxy_protocol::openai::chat::reasoning::ReasoningDetailKind as K;
    let details = message
        .reasoning_details
        .as_mut()
        .and_then(Option::as_mut)
        .ok_or_else(|| Error::invalid("Chat reasoning metadata has no detail records"))?;
    let mut chars = text.chars();
    for (position, segment) in layout.iter().enumerate() {
        let value = if position + 1 == layout.len() {
            chars.by_ref().collect()
        } else {
            chars.by_ref().take(segment.chars).collect()
        };
        let detail = details
            .get_mut(segment.index)
            .ok_or_else(|| Error::invalid("Chat reasoning layout references a missing detail"))?;
        match detail.type_ {
            K::Text => detail.text = Some(Some(value)),
            K::Summary => detail.summary = Some(value),
            K::Encrypted => {
                return Err(Error::invalid(
                    "visible reasoning cannot replace encrypted data",
                ));
            }
        }
    }
    Ok(())
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
            media::check_file_vendor(&image.source, Vendor::Openai)?;
            let source = &image.source;
            let mut url =
                c::ImageUrl::builder(data_url(source, image.media_type.as_deref())?).build();
            url.detail = media::chat_detail(image.detail);
            let mut value = c::ImagePart::builder(c::ImagePartType::ImageUrl, url).build();
            value.prompt_cache_breakpoint = media::chat_cache(block)?;
            c::UserContentPart::Image(value)
        }
        Content::Document(document) => {
            let (original, mime) = media::document_bytes(document)?;
            media::check_file_vendor(&original, Vendor::Openai)?;
            let source = original.as_ref();
            chat_file(block, source, mime, document.title.as_deref())?
        }
        Content::File(file) => {
            media::check_file_vendor(&file.source, Vendor::Openai)?;
            let source = &file.source;
            chat_file(
                block,
                source,
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
    Ok(request
        .provider_options
        .openai
        .chat
        .audio
        .as_ref()
        .and_then(Option::as_ref)
        .map(|audio| ir_format(audio.format)))
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
            .is_some_and(|s| !s.is_empty())
        {
            message.reasoning = Some(Some(String::new()));
            ReasoningSource::Reasoning
        } else {
            ReasoningSource::Details
        };
        let mut layout = Vec::new();
        if matches!(source, ReasoningSource::Details) {
            use gproxy_protocol::openai::chat::reasoning::ReasoningDetailKind as K;
            for (index, detail) in message
                .reasoning_details
                .iter_mut()
                .flatten()
                .flatten()
                .enumerate()
            {
                let value = match detail.type_ {
                    K::Text => detail.text.as_mut().and_then(Option::as_mut),
                    K::Summary => detail.summary.as_mut(),
                    K::Encrypted => None,
                };
                if let Some(value) = value {
                    layout.push(ReasoningSegment {
                        index,
                        chars: value.chars().count(),
                    });
                    value.clear();
                }
            }
        }
        result.push((
            0,
            block(
                Content::Reasoning(Reasoning { text }),
                Metadata {
                    reasoning_source: Some(source),
                    reasoning_layout: layout,
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
        let native = AudioMetadata {
            id: Some(audio.id),
            expires_at: Some(audio.expires_at),
            rest: audio.rest,
        };
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
    let native = MessageMetadata {
        reasoning_details: message.reasoning_details,
        reasoning_content: message.reasoning_content,
        reasoning: message.reasoning,
        rest: message.rest,
    };
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
fn append_nullable(target: &mut Option<Option<String>>, delta: Option<Option<String>>) {
    match delta {
        Some(Some(text)) => target
            .get_or_insert_with(|| Some(String::new()))
            .get_or_insert_with(String::new)
            .push_str(&text),
        Some(None) if target.is_none() => *target = Some(None),
        _ => {}
    }
}
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
        // This transient decode record avoids one extra heap allocation per chunk.
        #[allow(clippy::large_enum_variant)]
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
            let extensions: StreamExtensions =
                serde_json::from_value(Value::Object(delta.rest)).map_err(Error::invalid)?;
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
            if let Some(Some(text)) = &delta.reasoning_content {
                out.push(Event::ReasoningDelta {
                    id: BlockId { item: 0, part: 0 },
                    delta: text.clone(),
                });
            }
            append_nullable(&mut self.message.reasoning_content, delta.reasoning_content);
            if let Some(Some(text)) = &delta.reasoning {
                out.push(Event::ReasoningDelta {
                    id: BlockId { item: 0, part: 0 },
                    delta: text.clone(),
                });
            }
            append_nullable(&mut self.message.reasoning, delta.reasoning);
            if let Some(Some(details)) = delta.reasoning_details {
                let target = self
                    .message
                    .reasoning_details
                    .get_or_insert_with(|| Some(Vec::new()))
                    .get_or_insert_with(Vec::new);
                let plain_reasoning = gproxy_protocol::openai::chat::reasoning_text(
                    &self.message.reasoning_content,
                    &self.message.reasoning,
                )
                .is_some_and(|s| !s.is_empty());
                for detail in details {
                    if !plain_reasoning {
                        use gproxy_protocol::openai::chat::reasoning::ReasoningDetailKind as K;
                        let text = match detail.type_ {
                            K::Text => detail.text.as_ref().and_then(Option::as_ref),
                            K::Summary => detail.summary.as_ref(),
                            K::Encrypted => None,
                        };
                        if let Some(text) = text {
                            out.push(Event::ReasoningDelta {
                                id: BlockId { item: 0, part: 0 },
                                delta: text.clone(),
                            });
                        }
                    }
                    if let Some(index) = detail.index
                        && let Some(current) = target.iter_mut().find(|current| {
                            current.index == Some(index) && current.type_ == detail.type_
                        })
                    {
                        append_nullable(&mut current.text, detail.text);
                        append_nullable(&mut current.signature, detail.signature);
                        if let Some(text) = detail.summary {
                            current
                                .summary
                                .get_or_insert_with(String::new)
                                .push_str(&text);
                        }
                        if let Some(data) = detail.data {
                            current.data.get_or_insert_with(String::new).push_str(&data);
                        }
                        if detail.id.is_some() {
                            current.id = detail.id;
                        }
                        if detail.format.is_some() {
                            current.format = detail.format;
                        }
                        current.rest.extend(detail.rest);
                    } else {
                        target.push(detail);
                    }
                }
            }
            if let Some(annotations) = extensions.annotations {
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
            if let Some(audio) = extensions.audio {
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
            self.message.rest.extend(extensions.rest);
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
            let native = AudioMetadata {
                id: self.audio_id,
                expires_at: self.expires,
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
