use super::*;
use gproxy_protocol::openai::responses::{generate as q, input as i, response as r, tools as t};

#[derive(Serialize, Deserialize)]
struct TextMetadata {
    #[serde(default)]
    logprobs: Vec<i::OutputLogprob>,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
struct PartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native: Option<TextMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    item: Option<MessageMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<u32>,
}

enum Pending {
    Input {
        role: i::MessageRole,
        parts: Vec<i::InputContent>,
    },
    Output {
        group: Option<u32>,
        message: i::ResponseOutputMessage,
    },
}
fn flush(pending: Option<Pending>, input: &mut Vec<i::InputItem>) {
    if let Some(pending) = pending {
        input.push(match pending {
            Pending::Input { role, parts } => i::InputItem::Easy(
                i::EasyInputMessage::builder(i::MessageContent::Parts(parts), role)
                    .type_(i::MessageType::Message)
                    .build(),
            ),
            Pending::Output { message, .. } => i::InputItem::OutputMessage(message),
        });
    }
}
fn push_input(
    pending: &mut Option<Pending>,
    input: &mut Vec<i::InputItem>,
    role: i::MessageRole,
    part: i::InputContent,
) {
    match pending.take() {
        Some(Pending::Input { role, mut parts }) => {
            parts.push(part);
            *pending = Some(Pending::Input { role, parts });
        }
        other => {
            flush(other, input);
            *pending = Some(Pending::Input {
                role,
                parts: vec![part],
            });
        }
    }
}
fn push_output(
    pending: &mut Option<Pending>,
    input: &mut Vec<i::InputItem>,
    meta: PartMetadata,
    part: i::OutputContent,
) -> Result<()> {
    match pending.take() {
        Some(Pending::Output { group, mut message }) if group == meta.group => {
            message.content.push(part);
            *pending = Some(Pending::Output { group, message });
        }
        other => {
            flush(other, input);
            let item = meta.item.ok_or_else(|| {
                Error::invalid("native output-text metadata requires its message envelope")
            })?;
            let mut message = i::ResponseOutputMessage::builder(
                item.id,
                Vec::new(),
                i::OutputMessageRole::Assistant,
                item.status,
                i::MessageType::Message,
            )
            .rest(item.rest)
            .build();
            message.agent = item.agent;
            message.phase = item.phase;
            message.content = vec![part];
            *pending = Some(Pending::Output {
                group: meta.group,
                message,
            });
        }
    }
    Ok(())
}
fn message_role(role: &Role) -> Result<i::MessageRole> {
    Ok(match role {
        Role::System => i::MessageRole::System,
        Role::User => i::MessageRole::User,
        Role::Assistant => i::MessageRole::Assistant,
        Role::Unknown(_) => return Err(Error::invalid("unknown persisted message role")),
        Role::Tool => {
            return Err(Error::invalid(
                "Responses tool messages require tool-result blocks",
            ));
        }
    })
}

type Agent = gproxy_protocol::openai::responses::multi_agent::Agent;
fn nullable<'de, D, T>(deserializer: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
#[derive(Clone, Serialize, Deserialize)]
struct MessageMetadata {
    id: String,
    status: i::OutputMessageStatus,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    phase: Option<Option<i::MessagePhase>>,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Remainder {
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct InputImageMetadata {
    file_id_null: bool,
    image_url_null: bool,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct FileMetadata {
    raw_base64: bool,
    file_id_null: bool,
    detail_present: bool,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
struct GeneratedImageMetadata {
    id: String,
    status: i::ImageGenerationStatus,
    error: Option<gproxy_protocol::openai::responses::errors::ImageGenerationCallError>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    action: Option<Option<t::ImageAction>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    background: Option<Option<t::ImageBackground>>,
    format_present: bool,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    quality: Option<Option<t::ImageQuality>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    revised_prompt: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    size: Option<Option<String>>,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
// Read only at metadata serialization boundaries; boxing adds no lasting saving.
#[allow(clippy::large_enum_variant)]
enum ImageMetadata {
    Input(InputImageMetadata),
    Generated(GeneratedImageMetadata),
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct FunctionMetadata {
    id: Option<String>,
    namespace: Option<String>,
    status: Option<i::ItemStatus>,
    async_: Option<bool>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    caller: Option<Option<i::Caller>>,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
struct ResultMetadata {
    id: String,
    status: i::ItemStatus,
    name_present: bool,
    call_id_present: bool,
    namespace: Option<String>,
    created_by: Option<String>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    caller: Option<Option<i::Caller>>,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
struct SummaryMetadata {
    chars: usize,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
struct ReasoningMetadata {
    id: String,
    summary: Vec<SummaryMetadata>,
    content: Option<Vec<i::ReasoningContent>>,
    status: Option<i::ReasoningStatus>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    encrypted_content: Option<Option<String>>,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ServerMetadata {
    Web {
        status: i::WebSearchStatus,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
    Files {
        status: i::FileSearchStatus,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
    Code {
        status: i::CodeInterpreterStatus,
        container_id: String,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
    Computer {
        id: String,
        status: i::ItemStatus,
        pending_safety_checks: Vec<i::PendingSafetyCheck>,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ServerOutputKind {
    FileSearch,
    CodeInterpreter,
}
#[derive(Serialize, Deserialize)]
struct ServerOutputMetadata {
    kind: ServerOutputKind,
}
#[derive(Serialize, Deserialize)]
struct ComputerArguments {
    action: Option<i::ComputerAction>,
    actions: Option<Vec<i::ComputerAction>>,
}
fn server_output(id: String, name: &str, output: Value, kind: ServerOutputKind) -> Result<Block> {
    with_native(
        Content::ToolResult(ToolResult {
            id,
            name: name.into(),
            output: ToolOutput::Json(output),
            is_error: false,
            provider_executed: true,
        }),
        Vendor::Openai,
        ServerOutputMetadata { kind },
    )
}
fn restore_server_output(
    block: &Block,
    result: &ToolResult,
    input: &mut [i::InputItem],
) -> Result<()> {
    let Some(meta) = read_native::<ServerOutputMetadata>(block, Vendor::Openai)? else {
        return Ok(());
    };
    let ToolOutput::Json(output) = &result.output else {
        return Err(Error::invalid("native server output requires JSON"));
    };
    match meta.kind {
        ServerOutputKind::FileSearch => {
            let target = input
                .iter_mut()
                .rev()
                .find_map(|item| match item {
                    i::InputItem::FileSearchCall(value) if value.id == result.id => Some(value),
                    _ => None,
                })
                .ok_or_else(|| Error::invalid("file-search result has no earlier native call"))?;
            target.results = Some(serde_json::from_value(output.clone()).map_err(Error::invalid)?);
        }
        ServerOutputKind::CodeInterpreter => {
            let target = input
                .iter_mut()
                .rev()
                .find_map(|item| match item {
                    i::InputItem::CodeInterpreterCall(value) if value.id == result.id => {
                        Some(value)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    Error::invalid("code-interpreter result has no earlier native call")
                })?;
            target.outputs = Some(serde_json::from_value(output.clone()).map_err(Error::invalid)?);
        }
    }
    Ok(())
}

fn input_image(block: &ContentBlock<impl Sized>, image: &Image) -> Result<i::ResponseInputImage> {
    media::check_file_vendor(&image.source, Vendor::Openai)?;
    let source = &image.source;
    let mut value = i::ResponseInputImage::builder(
        i::ResponseInputImageType::ResponseInputImage,
        i::ImageDetail::Auto,
    )
    .build();
    if let Some(ImageMetadata::Input(meta)) = read_native(block, Vendor::Openai)? {
        value.rest = meta.rest;
        if meta.file_id_null {
            value.file_id = Some(None);
        }
        if meta.image_url_null {
            value.image_url = Some(None);
        }
    }
    value.detail = media::responses_detail(&image.detail).unwrap_or(i::ImageDetail::Auto);
    match source {
        Asset::ProviderFile { id, .. } => value.file_id = Some(Some(id.clone())),
        _ => value.image_url = Some(Some(data_url(source, image.media_type.as_deref())?)),
    };
    value.prompt_cache_breakpoint = media::responses_cache(block)?;
    Ok(value)
}
fn input_document(
    block: &ContentBlock<impl Sized>,
    document: &Document,
) -> Result<i::ResponseInputFile> {
    let (original, mime) = media::document_bytes(document)?;
    media::check_file_vendor(&original, Vendor::Openai)?;
    let source = original.as_ref();
    input_file(
        block,
        source,
        mime,
        document.title.as_deref(),
        Some(&document.detail),
    )
}
fn input_file(
    block: &ContentBlock<impl Sized>,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
    detail: Option<&Detail>,
) -> Result<i::ResponseInputFile> {
    let mut value =
        i::ResponseInputFile::builder(i::ResponseInputFileType::ResponseInputFile).build();
    let meta = read_native::<FileMetadata>(block, Vendor::Openai)?;
    let raw = meta.as_ref().is_some_and(|m| m.raw_base64);
    let had_detail = meta.as_ref().is_some_and(|m| m.detail_present);
    if let Some(meta) = meta {
        value.rest = meta.rest;
        if meta.file_id_null {
            value.file_id = Some(None);
        }
    }
    value.filename = filename.map(str::to_owned);
    match source {
        Asset::ProviderFile { vendor, id } => {
            if *vendor != Vendor::Openai {
                return Err(Error::invalid("provider files cannot cross vendors"));
            }
            value.file_id = Some(Some(id.clone()));
        }
        Asset::Url { url } => value.file_url = Some(url.clone()),
        Asset::Inline { bytes } => {
            value.file_data = Some(if raw {
                base64::engine::general_purpose::STANDARD.encode(bytes)
            } else {
                data_url(source, mime)?
            });
            value.filename = filename.map(str::to_owned);
        }
        Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "prepare application assets before rendering",
            ));
        }
    }
    value.detail = match detail {
        Some(Detail::Low) => Some(i::FileDetail::Low),
        Some(Detail::High) => Some(i::FileDetail::High),
        Some(Detail::Auto) => Some(i::FileDetail::Auto),
        Some(_) => None,
        None => value.detail,
    };
    // Auto is the native default; omit an override on newly-authored documents.
    if !had_detail && detail == Some(&Detail::Auto) && value.detail == Some(i::FileDetail::Auto) {
        value.detail = None;
    }
    value.prompt_cache_breakpoint = media::responses_cache(block)?;
    Ok(value)
}
pub(crate) fn is_output_image(block: &Block) -> Result<bool> {
    Ok(matches!(
        read_native::<ImageMetadata>(block, Vendor::Openai)?,
        Some(ImageMetadata::Generated(_))
    ))
}
fn generated_image(block: &Block, image: &Image) -> Result<Option<i::InputItem>> {
    let Some(ImageMetadata::Generated(meta)) = read_native(block, Vendor::Openai)? else {
        return Ok(None);
    };
    let Asset::Inline { bytes } = &image.source else {
        return Err(Error::invalid(
            "generated image history requires inline bytes",
        ));
    };
    let mut value = i::ImageGenerationCall::builder(
        i::ImageGenerationCallType::ImageGenerationCall,
        meta.id,
        Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
        meta.status,
    )
    .rest(meta.rest)
    .build();
    value.error = meta.error;
    value.agent = meta.agent;
    value.action = meta.action;
    value.background = meta.background;
    value.output_format = match image.media_type.as_deref() {
        Some("image/png") => Some(Some(t::ImageOutputFormat::Png)),
        Some("image/jpeg") => Some(Some(t::ImageOutputFormat::Jpeg)),
        Some("image/webp") => Some(Some(t::ImageOutputFormat::Webp)),
        Some(_) => {
            return Err(Error::invalid(
                "native generated-image history requires PNG, JPEG or WebP",
            ));
        }
        None => meta.format_present.then_some(None),
    };
    value.quality = meta.quality;
    value.revised_prompt = meta.revised_prompt;
    value.size = meta.size;
    Ok(Some(i::InputItem::ImageGenerationCall(value)))
}
fn function_output(block: &ToolResultBlock) -> Result<i::FunctionOutputContent> {
    Ok(match &block.content {
        ToolResultContent::Unknown(_) => {
            return Err(Error::invalid("unknown persisted tool content"));
        }
        ToolResultContent::Text(text) => {
            let mut value = i::FunctionOutputText::builder(
                i::ResponseInputTextType::ResponseInputText,
                text.text.clone(),
            )
            .build();
            if let Some(native) = read_native::<Remainder>(block, Vendor::Openai)? {
                value.rest = native.rest;
            }
            value.prompt_cache_breakpoint = media::responses_cache(block)?.map(Some);
            i::FunctionOutputContent::Text(value)
        }
        ToolResultContent::Image(image) => {
            let input = input_image(block, image)?;
            let mut value =
                i::FunctionOutputImage::builder(i::ResponseInputImageType::ResponseInputImage)
                    .build();
            value.file_id = input.file_id;
            value.image_url = input.image_url;
            value.detail = Some(Some(input.detail));
            value.prompt_cache_breakpoint = input.prompt_cache_breakpoint.map(Some);
            value.rest = input.rest;
            i::FunctionOutputContent::Image(value)
        }
        ToolResultContent::Document(document) => {
            let input = input_document(block, document)?;
            let mut value =
                i::FunctionOutputFile::builder(i::ResponseInputFileType::ResponseInputFile).build();
            value.file_id = input.file_id;
            value.file_data = input.file_data.map(Some);
            value.file_url = input.file_url.map(Some);
            value.filename = input.filename.map(Some);
            value.detail = input.detail;
            value.prompt_cache_breakpoint = input.prompt_cache_breakpoint.map(Some);
            value.rest = input.rest;
            i::FunctionOutputContent::File(value)
        }
    })
}
fn restore_server(block: &Block, call: &ToolCall) -> Result<Option<i::InputItem>> {
    let Some(meta) = read_native::<ServerMetadata>(block, Vendor::Openai)? else {
        return Ok(None);
    };
    Ok(Some(match meta {
        ServerMetadata::Web {
            status,
            agent,
            rest,
        } => {
            let mut v = i::WebSearchCall::builder(
                i::WebSearchCallType::WebSearchCall,
                call.id.clone(),
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            v.action = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            i::InputItem::WebSearchCall(v)
        }
        ServerMetadata::Files {
            status,
            agent,
            rest,
        } => {
            let queries = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            let mut v = i::FileSearchCall::builder(
                i::FileSearchCallType::FileSearchCall,
                call.id.clone(),
                queries,
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            i::InputItem::FileSearchCall(v)
        }
        ServerMetadata::Code {
            status,
            container_id,
            agent,
            rest,
        } => {
            let code = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            let mut v = i::CodeInterpreterCall::builder(
                i::CodeInterpreterCallType::CodeInterpreterCall,
                call.id.clone(),
                code,
                container_id,
                None,
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            i::InputItem::CodeInterpreterCall(v)
        }
        ServerMetadata::Computer {
            id,
            status,
            pending_safety_checks,
            agent,
            rest,
        } => {
            let mut v = i::ComputerCall::builder(
                i::ComputerCallType::ComputerCall,
                id,
                call.id.clone(),
                pending_safety_checks,
                status,
            )
            .rest(rest)
            .build();
            v.agent = agent;
            let arguments: ComputerArguments =
                serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            v.action = arguments.action;
            v.actions = arguments.actions;
            i::InputItem::ComputerCall(v)
        }
    }))
}
pub fn render(
    request: &GenerateRequest,
    streaming: bool,
) -> Result<Rendered<q::GenerateContentRequestBody>> {
    crate::resolution::validate_request(request)?;
    let opts = &request.provider_options.openai.responses;
    let mut body = q::GenerateContentRequestBody::builder().build();
    body.reasoning = opts.reasoning.clone();
    body.include = opts.include.clone();
    body.store = opts.store;
    body.background = opts.background;
    body.parallel_tool_calls = opts.parallel_tool_calls;
    body.previous_response_id = opts.previous_response_id.clone();
    body.prompt_cache_key = opts.prompt_cache_key.clone();
    body.prompt_cache_options = opts.prompt_cache_options.clone();
    body.prompt_cache_retention = opts.prompt_cache_retention;
    body.service_tier = opts.service_tier;
    body.text = opts.text.clone();
    body.tools = opts.tools.clone();
    body.truncation = opts.truncation;
    body.rest = request.provider_options.openai.rest.clone();
    body.rest.extend(opts.rest.clone());
    body.model = Some(request.model.id.clone());
    body.stream = Some(Some(streaming));
    let mut input = Vec::new();
    for message in &request.messages {
        if matches!(&message.role, Role::Unknown(_)) {
            continue;
        }
        let mut pending = None;
        for block in &message.content {
            match &block.content {
                Content::Text(text) => {
                    let meta = read_metadata::<PartMetadata>(block, Vendor::Openai)?;
                    if let Some(PartMetadata {
                        native: Some(native),
                        item,
                        group,
                    }) = meta
                        && message.role == Role::Assistant
                    {
                        let part = if text.refusal {
                            i::OutputContent::Refusal(
                                i::ResponseOutputRefusal::builder(
                                    i::ResponseOutputRefusalType::ResponseOutputRefusal,
                                    text.text.clone(),
                                )
                                .rest(native.rest)
                                .build(),
                            )
                        } else {
                            let annotations = text
                                .citations
                                .iter()
                                .map(citations::render_responses)
                                .collect::<Result<Vec<_>>>()?
                                .into_iter()
                                .flatten()
                                .collect();
                            i::OutputContent::Text(
                                i::ResponseOutputText::builder(
                                    i::ResponseOutputTextType::ResponseOutputText,
                                    text.text.clone(),
                                    annotations,
                                    native.logprobs,
                                )
                                .rest(native.rest)
                                .build(),
                            )
                        };
                        push_output(
                            &mut pending,
                            &mut input,
                            PartMetadata {
                                native: None,
                                item,
                                group,
                            },
                            part,
                        )?;
                    } else {
                        let mut text = i::ResponseInputText::builder(
                            i::ResponseInputTextType::ResponseInputText,
                            text.text.clone(),
                        )
                        .build();
                        text.prompt_cache_breakpoint = media::responses_cache(block)?;
                        push_input(
                            &mut pending,
                            &mut input,
                            message_role(&message.role)?,
                            i::InputContent::Text(text),
                        );
                    }
                }
                Content::Image(image) => {
                    if message.role == Role::Assistant
                        && let Some(generated) = generated_image(block, image)?
                    {
                        flush(pending.take(), &mut input);
                        input.push(generated);
                    } else {
                        push_input(
                            &mut pending,
                            &mut input,
                            message_role(&message.role)?,
                            i::InputContent::Image(input_image(block, image)?),
                        );
                    }
                }
                Content::Document(document) => push_input(
                    &mut pending,
                    &mut input,
                    message_role(&message.role)?,
                    i::InputContent::File(input_document(block, document)?),
                ),
                Content::File(file) => {
                    media::check_file_vendor(&file.source, Vendor::Openai)?;
                    let source = &file.source;
                    let value = input_file(
                        block,
                        source,
                        file.media_type.as_deref(),
                        file.filename.as_deref(),
                        None,
                    )?;
                    push_input(
                        &mut pending,
                        &mut input,
                        message_role(&message.role)?,
                        i::InputContent::File(value),
                    );
                }
                Content::Reasoning(Reasoning { text }) => {
                    flush(pending.take(), &mut input);
                    if let Some(meta) = read_native::<ReasoningMetadata>(block, Vendor::Openai)? {
                        let mut chars = text.chars();
                        let count = meta.summary.len();
                        let mut summary = Vec::with_capacity(count);
                        for (index, part) in meta.summary.into_iter().enumerate() {
                            let text = if index + 1 == count {
                                chars.by_ref().collect()
                            } else {
                                chars.by_ref().take(part.chars).collect()
                            };
                            summary.push(
                                i::SummaryText::builder(i::SummaryTextType::SummaryText, text)
                                    .rest(part.rest)
                                    .build(),
                            );
                        }
                        let mut value = i::ReasoningItem::builder(
                            i::ReasoningItemType::ReasoningItem,
                            meta.id,
                            summary,
                        )
                        .rest(meta.rest)
                        .build();
                        value.content = meta.content;
                        value.status = meta.status;
                        value.agent = meta.agent;
                        value.encrypted_content = meta.encrypted_content;
                        input.push(i::InputItem::Reasoning(value));
                    }
                }
                Content::ToolCall(call) => {
                    flush(pending.take(), &mut input);
                    if call.provider_executed {
                        if let Some(value) = restore_server(block, call)? {
                            input.push(value);
                        }
                        continue;
                    }
                    let mut value = i::FunctionCall::builder(
                        i::FunctionCallType::FunctionCall,
                        call.arguments.clone(),
                        call.id.clone(),
                        call.name.clone(),
                    )
                    .build();
                    if let Some(meta) = read_native::<FunctionMetadata>(block, Vendor::Openai)? {
                        value.id = meta.id;
                        value.namespace = meta.namespace;
                        value.status = meta.status;
                        value.async_ = meta.async_;
                        value.agent = meta.agent;
                        value.caller = meta.caller;
                        value.rest = meta.rest;
                    }
                    input.push(i::InputItem::FunctionCall(value));
                }
                Content::ToolResult(result) => {
                    flush(pending.take(), &mut input);
                    if result.provider_executed {
                        restore_server_output(block, result, &mut input)?;
                        continue;
                    }
                    let output = match &result.output {
                        ToolOutput::Unknown(_) => {
                            return Err(Error::invalid("unknown persisted tool output"));
                        }
                        ToolOutput::Json(Value::String(text)) => {
                            i::FunctionOutput::Text(text.clone())
                        }
                        ToolOutput::Json(value) => i::FunctionOutput::Text(value.to_string()),
                        ToolOutput::Blocks(blocks) => i::FunctionOutput::Content(
                            blocks.iter().map(function_output).collect::<Result<_>>()?,
                        ),
                    };
                    let mut value = i::FunctionCallOutput::builder(
                        i::FunctionCallOutputType::FunctionCallOutput,
                        output,
                    )
                    .build();
                    let meta = read_native::<ResultMetadata>(block, Vendor::Openai)?;
                    let include_id =
                        meta.as_ref().is_some_and(|m| m.call_id_present) || !result.id.is_empty();
                    let include_name =
                        meta.as_ref().is_some_and(|m| m.name_present) || !result.name.is_empty();
                    if let Some(meta) = meta {
                        value.id = Some(Some(meta.id));
                        value.status = Some(Some(meta.status));
                        value.namespace = meta.namespace.map(Some);
                        value.agent = meta.agent;
                        value.caller = meta.caller;
                        value.rest = meta.rest;
                        if let Some(created_by) = meta.created_by {
                            #[derive(Serialize)]
                            struct CreatedBy {
                                created_by: String,
                            }
                            value.rest.extend(encode_rest(CreatedBy { created_by })?);
                        }
                    }
                    if include_id {
                        value.call_id = Some(Some(result.id.clone()));
                    }
                    if include_name {
                        value.name = Some(Some(result.name.clone()));
                    }
                    input.push(i::InputItem::FunctionCallOutput(value));
                }
                Content::Unknown(_) => {}
                Content::Opaque => {
                    flush(pending.take(), &mut input);
                    if let Some(value) = read_native(block, Vendor::Openai)? {
                        input.push(value);
                    }
                }
                Content::Audio(_) | Content::Video(_) => {
                    return Err(Error::invalid(
                        "Responses does not accept audio/video blocks; use Chat Completions or Realtime for audio",
                    ));
                }
            }
        }
        flush(pending, &mut input);
    }

    body.input = Some(i::Input::Items(input));
    if request
        .settings
        .output_modalities
        .contains(&Modality::Audio)
        || request.settings.speech.is_some()
    {
        return Err(Error::invalid(
            "Responses audio output requires the Chat or Realtime dialect",
        ));
    }
    if request
        .settings
        .output_modalities
        .contains(&Modality::Image)
    {
        let tools = body.tools.get_or_insert_with(Vec::new);
        if !tools
            .iter()
            .any(|tool| matches!(tool, t::Tool::ImageGeneration(_)))
        {
            tools.push(t::Tool::ImageGeneration(
                t::ImageGenerationTool::builder().build(),
            ));
        }
    }

    if let Some(value) = request.settings.temperature {
        body.temperature = Some(Some(
            serde_json::Number::from_f64(value)
                .ok_or_else(|| Error::invalid("temperature must be finite"))?,
        ));
    }
    if let Some(value) = request.settings.top_p {
        body.top_p = Some(Some(
            serde_json::Number::from_f64(value)
                .ok_or_else(|| Error::invalid("top_p must be finite"))?,
        ));
    }
    if let Some(value) = request.settings.max_output_tokens {
        body.max_output_tokens = Some(Some(value.into()));
    }
    if !request.settings.stop_sequences.is_empty() {
        return Err(Error::invalid("Responses has no stop-sequence parameter"));
    }
    if !request.tools.is_empty() {
        let functions = request
            .tools
            .iter()
            .map(|tool| {
                let parameters =
                    serde_json::from_value(tool.input_schema.clone()).map_err(Error::invalid)?;
                Ok(t::Tool::Function(
                    t::FunctionTool::builder(tool.name.clone(), Some(parameters), None)
                        .description(Some(tool.description.clone()))
                        .build(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        body.tools.get_or_insert_with(Vec::new).extend(functions);
    }
    if let Some(choice) = &request.settings.tool_choice {
        body.tool_choice = match choice {
            ToolChoice::Auto => Some(i::ToolChoice::Mode(i::ToolChoiceMode::Auto)),
            ToolChoice::None => Some(i::ToolChoice::Mode(i::ToolChoiceMode::None)),
            ToolChoice::Required => Some(i::ToolChoice::Mode(i::ToolChoiceMode::Required)),
            ToolChoice::Tool { name } => Some(i::ToolChoice::Function(
                i::ToolChoiceFunction::builder(
                    i::ToolChoiceFunctionType::ToolChoiceFunction,
                    name.clone(),
                )
                .build(),
            )),
            ToolChoice::Unknown(_) => None,
        };
    }
    if let Some(schema) = &request.settings.output_schema
        && opts.text.as_ref().and_then(|c| c.format.as_ref()).is_none()
    {
        let schema = serde_json::from_value(schema.clone()).map_err(Error::invalid)?;
        let text = body
            .text
            .get_or_insert_with(|| i::TextConfig::builder().build());
        let mut format = match text.format.take() {
            Some(i::TextFormat::JsonSchema(format)) => format,
            Some(i::TextFormat::Text(format)) => {
                i::TextFormatJsonSchema::builder("response".into(), Rest::new())
                    .rest(format.rest)
                    .build()
            }
            Some(i::TextFormat::JsonObject(format)) => {
                i::TextFormatJsonSchema::builder("response".into(), Rest::new())
                    .rest(format.rest)
                    .build()
            }
            None => i::TextFormatJsonSchema::builder("response".into(), Rest::new()).build(),
        };
        format.name = "response".into();
        format.schema = schema;
        text.format = Some(i::TextFormat::JsonSchema(format));
    }
    let mut warnings = crate::warnings::for_request(request, None)?;
    warnings.extend(crate::reasoning::openai(request, &mut body));
    Ok(Rendered {
        request: body,
        warnings,
    })
}

pub fn parse(response: r::GenerateContentResponseBody) -> Result<GenerateResult> {
    let metadata = response_metadata(&response);
    let sources = response
        .output
        .iter()
        .map(crate::sources::openai_item)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    let sources = crate::sources::unique(sources);
    let (finish_reason, usage) = finish_metadata(&response)?;
    let mut content = Vec::new();
    for (index, item) in response.output.into_iter().enumerate() {
        content.extend(parse_item(item, index as u32)?);
    }
    Ok(GenerateResult {
        response: metadata,
        warnings: Vec::new(),
        sources,
        content,
        finish_reason,
        usage,
    })
}
pub(crate) fn response_metadata(response: &r::GenerateContentResponseBody) -> ResponseMetadata {
    ResponseMetadata {
        id: Some(response.id.clone()),
        model: Some(response.model.clone()),
        timestamp: Some(response.created_at),
    }
}
pub(crate) fn parse_usage(response: &r::GenerateContentResponseBody) -> Result<Usage> {
    let usage = match response.usage.as_ref().and_then(Option::as_ref) {
        None => Usage::default(),
        Some(value) => Usage {
            input_tokens: Some(count(value.input_tokens)?),
            output_tokens: Some(count(value.output_tokens)?),
            reasoning_tokens: Some(count(value.output_tokens_details.reasoning_tokens)?),
            cache_read_tokens: Some(count(value.input_tokens_details.cached_tokens)?),
            cache_write_tokens: Some(count(value.input_tokens_details.cache_write_tokens)?),
        },
    };
    Ok(usage)
}
pub(crate) fn error_finish_reason(response: &r::GenerateContentResponseBody) -> FinishReason {
    let raw = response
        .incomplete_details
        .as_ref()
        .and_then(|d| d.reason)
        .map(|reason| match reason {
            r::ResponseIncompleteReason::MaxOutputTokens => "max_output_tokens",
            r::ResponseIncompleteReason::ContentFilter => "content_filter",
            r::ResponseIncompleteReason::MaxMessages => "max_messages",
            r::ResponseIncompleteReason::Steered => "steered",
            r::ResponseIncompleteReason::Interrupted => "interrupted",
        })
        .or_else(|| {
            response.status.map(|status| match status {
                r::ResponseStatus::Completed => "completed",
                r::ResponseStatus::Failed => "failed",
                r::ResponseStatus::Incomplete => "incomplete",
                r::ResponseStatus::Cancelled => "cancelled",
                r::ResponseStatus::Queued => "queued",
                r::ResponseStatus::InProgress => "in_progress",
            })
        });
    FinishReason::new(FinishReasonKind::Error, raw.map(str::to_owned))
}
pub(crate) fn finish_metadata(
    response: &r::GenerateContentResponseBody,
) -> Result<(FinishReason, Usage)> {
    if response.error.is_some() {
        return Err(Error::from_event(
            serde_json::to_string(response).map_err(Error::invalid)?,
        ));
    }
    let (mut reason, raw) = if let Some(incomplete) =
        response.incomplete_details.as_ref().and_then(|d| d.reason)
    {
        let (kind, raw) = match incomplete {
            r::ResponseIncompleteReason::MaxOutputTokens => {
                (FinishReasonKind::Length, "max_output_tokens")
            }
            r::ResponseIncompleteReason::ContentFilter => {
                (FinishReasonKind::ContentBlocked, "content_filter")
            }
            r::ResponseIncompleteReason::MaxMessages => (FinishReasonKind::Length, "max_messages"),
            r::ResponseIncompleteReason::Steered => (FinishReasonKind::Interrupted, "steered"),
            r::ResponseIncompleteReason::Interrupted => {
                (FinishReasonKind::Interrupted, "interrupted")
            }
        };
        (kind, Some(raw.into()))
    } else {
        match response.status {
            Some(r::ResponseStatus::Completed) => {
                (FinishReasonKind::Stop, Some("completed".into()))
            }
            Some(r::ResponseStatus::Failed) => (FinishReasonKind::Error, Some("failed".into())),
            Some(r::ResponseStatus::Incomplete) => {
                (FinishReasonKind::Interrupted, Some("incomplete".into()))
            }
            Some(r::ResponseStatus::Cancelled) => {
                (FinishReasonKind::Interrupted, Some("cancelled".into()))
            }
            Some(r::ResponseStatus::Queued) => {
                (FinishReasonKind::Interrupted, Some("queued".into()))
            }
            Some(r::ResponseStatus::InProgress) => {
                (FinishReasonKind::Interrupted, Some("in_progress".into()))
            }
            None => (FinishReasonKind::Other, None),
        }
    };
    if reason == FinishReasonKind::Stop
        && response
            .output
            .iter()
            .any(|item| matches!(item, r::ResponseOutputItem::FunctionCall(_)))
    {
        reason = FinishReasonKind::ToolCalls;
    }
    let refused = response.output.iter().any(|item| match item {
        r::ResponseOutputItem::Message(message) => message
            .content
            .iter()
            .any(|part| matches!(part, i::OutputContent::Refusal(_))),
        _ => false,
    });
    if refused {
        reason = FinishReasonKind::ContentBlocked;
    }
    let usage = parse_usage(response)?;
    Ok((FinishReason::new(reason, raw), usage))
}

pub(crate) fn parse_item(item: r::ResponseOutputItem, index: u32) -> Result<Vec<Block>> {
    Ok(match item {
        r::ResponseOutputItem::Message(mut message) => {
            let parts = std::mem::take(&mut message.content);
            let envelope = MessageMetadata {
                id: message.id,
                status: message.status,
                agent: message.agent,
                phase: message.phase,
                rest: message.rest,
            };
            parts
                .into_iter()
                .map(|part| {
                    let (content, native) = match part {
                        i::OutputContent::Text(value) => {
                            let citations = value
                                .annotations
                                .into_iter()
                                .map(citations::parse_responses)
                                .collect::<Result<_>>()?;
                            (
                                Content::Text(Text {
                                    refusal: false,
                                    text: value.text,
                                    citations,
                                }),
                                TextMetadata {
                                    logprobs: value.logprobs,
                                    rest: value.rest,
                                },
                            )
                        }
                        i::OutputContent::Refusal(value) => (
                            Content::Text(Text {
                                refusal: true,
                                text: value.refusal,
                                citations: Vec::new(),
                            }),
                            TextMetadata {
                                logprobs: Vec::new(),
                                rest: value.rest,
                            },
                        ),
                    };
                    with_metadata(
                        content,
                        Vendor::Openai,
                        PartMetadata {
                            native: Some(native),
                            item: Some(envelope.clone()),
                            group: Some(index),
                        },
                    )
                })
                .collect::<Result<_>>()?
        }
        r::ResponseOutputItem::FunctionCall(mut call) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut call.call_id),
                name: std::mem::take(&mut call.name),
                arguments: std::mem::take(&mut call.arguments),
                provider_executed: false,
            });
            let meta = FunctionMetadata {
                id: call.id,
                namespace: call.namespace,
                status: call.status,
                async_: call.async_,
                agent: call.agent,
                caller: call.caller,
                rest: call.rest,
            };
            vec![with_native(content, Vendor::Openai, meta)?]
        }
        r::ResponseOutputItem::Reasoning(item) => {
            let text = item.summary.iter().map(|v| v.text.as_str()).collect();
            let summary = item
                .summary
                .into_iter()
                .map(|v| SummaryMetadata {
                    chars: v.text.chars().count(),
                    rest: v.rest,
                })
                .collect();
            let meta = ReasoningMetadata {
                id: item.id,
                summary,
                content: item.content,
                status: item.status,
                agent: item.agent,
                encrypted_content: item.encrypted_content,
                rest: item.rest,
            };
            vec![with_native(
                Content::Reasoning(Reasoning { text }),
                Vendor::Openai,
                meta,
            )?]
        }
        r::ResponseOutputItem::FunctionCallOutput(value) => vec![parse_tool_result(value)?],
        r::ResponseOutputItem::ImageGenerationCall(mut value) => {
            if let Some(data) = value.result.take() {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(Error::invalid)?;
                let mime = value
                    .output_format
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(|f| {
                        match f {
                            t::ImageOutputFormat::Png => "image/png",
                            t::ImageOutputFormat::Jpeg => "image/jpeg",
                            t::ImageOutputFormat::Webp => "image/webp",
                        }
                        .to_owned()
                    });
                let content = Content::Image(Image {
                    source: Asset::Inline { bytes },
                    media_type: mime,
                    detail: Detail::Auto,
                });
                vec![with_native(
                    content,
                    Vendor::Openai,
                    ImageMetadata::Generated(GeneratedImageMetadata {
                        id: value.id,
                        status: value.status,
                        error: value.error,
                        agent: value.agent,
                        action: value.action,
                        background: value.background,
                        format_present: value.output_format.is_some(),
                        quality: value.quality,
                        revised_prompt: value.revised_prompt,
                        size: value.size,
                        rest: value.rest,
                    }),
                )?]
            } else {
                vec![with_native(
                    Content::Opaque,
                    Vendor::Openai,
                    r::ResponseOutputItem::ImageGenerationCall(value),
                )?]
            }
        }
        r::ResponseOutputItem::WebSearchCall(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "web_search".into(),
                arguments: serde_json::to_string(&value.action).map_err(Error::invalid)?,
                provider_executed: true,
            });
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Web {
                    status: value.status,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?]
        }
        r::ResponseOutputItem::FileSearchCall(mut value) => {
            let outputs = value.results.take();
            let id = value.id.clone();
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "file_search".into(),
                arguments: serde_json::to_string(&value.queries).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut blocks = vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Files {
                    status: value.status,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?];
            if let Some(outputs) = outputs {
                blocks.push(server_output(
                    id,
                    "file_search",
                    serde_json::to_value(outputs).map_err(Error::invalid)?,
                    ServerOutputKind::FileSearch,
                )?);
            }
            blocks
        }
        r::ResponseOutputItem::CodeInterpreterCall(mut value) => {
            let outputs = value.outputs.take();
            let id = value.id.clone();
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "code_interpreter".into(),
                arguments: serde_json::to_string(&value.code).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut blocks = vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Code {
                    status: value.status,
                    container_id: value.container_id,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?];
            if let Some(outputs) = outputs {
                blocks.push(server_output(
                    id,
                    "code_interpreter",
                    serde_json::to_value(outputs).map_err(Error::invalid)?,
                    ServerOutputKind::CodeInterpreter,
                )?);
            }
            blocks
        }
        r::ResponseOutputItem::ComputerCall(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.call_id),
                name: "computer".into(),
                arguments: serde_json::to_string(&ComputerArguments {
                    action: value.action,
                    actions: value.actions,
                })
                .map_err(Error::invalid)?,
                provider_executed: true,
            });
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Computer {
                    id: value.id,
                    status: value.status,
                    pending_safety_checks: value.pending_safety_checks,
                    agent: value.agent,
                    rest: value.rest,
                },
            )?]
        }
        other => vec![with_native(Content::Opaque, Vendor::Openai, other)?],
    })
}
pub fn parse_tool_result(output: r::ResponseFunctionCallOutput) -> Result<Block> {
    let output_content = match output.output {
        i::CustomOutput::Text(text) => ToolOutput::Json(Value::String(text)),
        i::CustomOutput::Content(parts) => ToolOutput::Blocks(
            parts
                .into_iter()
                .map(parse_output_part)
                .collect::<Result<_>>()?,
        ),
    };
    let meta = ResultMetadata {
        id: output.id,
        status: output.status,
        name_present: output.name.is_some(),
        call_id_present: output.call_id.is_some(),
        namespace: output.namespace,
        created_by: output.created_by,
        agent: output.agent,
        caller: output.caller,
        rest: output.rest,
    };
    let content = Content::ToolResult(ToolResult {
        id: output.call_id.unwrap_or_default(),
        name: output.name.unwrap_or_default(),
        output: output_content,
        is_error: false,
        provider_executed: false,
    });
    with_native(content, Vendor::Openai, meta)
}
fn parse_output_part(part: i::InputContent) -> Result<ToolResultBlock> {
    match part {
        i::InputContent::Text(mut value) => {
            let cache = value.prompt_cache_breakpoint.take();
            let text = std::mem::take(&mut value.text);
            let native = Remainder { rest: value.rest };
            let mut block = with_native(
                ToolResultContent::Text(Text {
                    refusal: false,
                    text,
                    citations: Vec::new(),
                }),
                Vendor::Openai,
                native,
            )?;
            media::parse_responses_cache(&mut block, cache)?;
            Ok(block)
        }
        i::InputContent::Image(mut value) => {
            let file_id_null = value.file_id == Some(None);
            let image_url_null = value.image_url == Some(None);
            let source = if let Some(Some(id)) = value.file_id.take() {
                Asset::ProviderFile {
                    vendor: Vendor::Openai,
                    id,
                }
            } else if let Some(Some(url)) = value.image_url.take() {
                Asset::Url { url }
            } else {
                return Err(Error::invalid("tool image has no source"));
            };
            let detail = match value.detail {
                i::ImageDetail::Low => Detail::Low,
                i::ImageDetail::High => Detail::High,
                i::ImageDetail::Auto => Detail::Auto,
                i::ImageDetail::Original => Detail::Original,
            };
            let cache = value.prompt_cache_breakpoint.take();
            let meta = InputImageMetadata {
                file_id_null,
                image_url_null,
                rest: value.rest,
            };
            let mut block = with_native(
                ToolResultContent::Image(Image {
                    source,
                    media_type: None,
                    detail,
                }),
                Vendor::Openai,
                ImageMetadata::Input(meta),
            )?;
            media::parse_responses_cache(&mut block, cache)?;
            Ok(block)
        }
        i::InputContent::File(mut value) => {
            let file_id_null = value.file_id == Some(None);
            let mut mime = None;
            let mut raw = false;
            let source = if let Some(Some(id)) = value.file_id.take() {
                Asset::ProviderFile {
                    vendor: Vendor::Openai,
                    id,
                }
            } else if let Some(url) = value.file_url.take() {
                Asset::Url { url }
            } else if let Some(data) = value.file_data.take() {
                let payload = if let Some(header) = data.strip_prefix("data:") {
                    let (kind, data) = header
                        .split_once(";base64,")
                        .ok_or_else(|| Error::invalid("file data URI must be base64"))?;
                    mime = Some(kind.to_owned());
                    data
                } else {
                    raw = true;
                    data.as_str()
                };
                Asset::Inline {
                    bytes: base64::engine::general_purpose::STANDARD
                        .decode(payload)
                        .map_err(Error::invalid)?,
                }
            } else {
                return Err(Error::invalid("tool document has no source"));
            };
            let detail = match value.detail {
                Some(i::FileDetail::Low) => Detail::Low,
                Some(i::FileDetail::High) => Detail::High,
                _ => Detail::Auto,
            };
            let document = Document {
                source: DocumentSource::Asset { source },
                media_type: mime,
                title: value.filename.take(),
                context: None,
                citations: None,
                detail,
            };
            let cache = value.prompt_cache_breakpoint.take();
            let detail_present = value.detail.is_some();
            let rest = value.rest;
            let mut block = with_native(
                ToolResultContent::Document(document),
                Vendor::Openai,
                FileMetadata {
                    raw_base64: raw,
                    file_id_null,
                    detail_present,
                    rest,
                },
            )?;
            media::parse_responses_cache(&mut block, cache)?;
            Ok(block)
        }
    }
}
