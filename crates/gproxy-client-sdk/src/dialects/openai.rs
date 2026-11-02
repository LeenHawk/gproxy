use super::*;
use gproxy_protocol::openai::responses::{generate as q, input as i, response as r, tools as t};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum TextMetadata {
    #[serde(rename = "output_text")]
    Text {
        annotations: Vec<i::OutputAnnotation>,
        logprobs: Vec<i::OutputLogprob>,
        #[serde(flatten)]
        rest: Rest,
    },
    #[serde(rename = "refusal")]
    Refusal {
        #[serde(flatten)]
        rest: Rest,
    },
}
#[derive(Serialize, Deserialize)]
struct PartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native: Option<TextMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    item: Option<i::ResponseOutputMessageOptions>,
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
) {
    match pending.take() {
        Some(Pending::Output { group, mut message }) if group == meta.group => {
            message.content.push(part);
            *pending = Some(Pending::Output { group, message });
        }
        other => {
            flush(other, input);
            let mut message = i::ResponseOutputMessage::builder(
                String::new(),
                Vec::new(),
                i::OutputMessageRole::Assistant,
                i::OutputMessageStatus::Completed,
                i::MessageType::Message,
            )
            .build();
            if let Some(item) = meta.item {
                item.apply_to(&mut message);
            }
            message.content = vec![part];
            *pending = Some(Pending::Output {
                group: meta.group,
                message,
            });
        }
    }
}
fn message_role(role: Role) -> Result<i::MessageRole> {
    Ok(match role {
        Role::System => i::MessageRole::System,
        Role::User => i::MessageRole::User,
        Role::Assistant => i::MessageRole::Assistant,
        Role::Tool => {
            return Err(Error::invalid(
                "Responses tool messages require tool-result blocks",
            ));
        }
    })
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
enum ImageMetadata {
    Input(i::ResponseInputImageOptions),
    Generated(i::ImageGenerationCallOptions),
}
#[derive(Serialize, Deserialize)]
struct FileMetadata {
    #[serde(default)]
    raw_base64: bool,
    #[serde(flatten)]
    fields: i::ResponseInputFileOptions,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
enum ServerMetadata {
    Web(i::WebSearchCallOptions),
    Files(i::FileSearchCallOptions),
    Code(i::CodeInterpreterCallOptions),
    Computer(i::ComputerCallOptions),
}

fn input_image(block: &ContentBlock<impl Sized>, image: &Image) -> Result<i::ResponseInputImage> {
    let source = crate::assets::source(block, &image.source, Vendor::Openai, true)?;
    let mut value = i::ResponseInputImage::builder(
        i::ResponseInputImageType::ResponseInputImage,
        i::ImageDetail::Auto,
    )
    .build();
    if let Some(ImageMetadata::Input(mut meta)) = read_native(block, Vendor::Openai)? {
        meta.file_id = None;
        meta.image_url = None;
        meta.apply_to(&mut value);
    }
    value.detail = media::responses_detail(image.detail).unwrap_or(i::ImageDetail::Auto);
    match source.as_ref() {
        Asset::ProviderFile { id, .. } => value.file_id = Some(Some(id.clone())),
        _ => value.image_url = Some(Some(data_url(&source, image.media_type.as_deref())?)),
    };
    value.prompt_cache_breakpoint = media::responses_cache(block)?;
    Ok(value)
}
fn input_document(
    block: &ContentBlock<impl Sized>,
    document: &Document,
) -> Result<i::ResponseInputFile> {
    let (original, mime) = media::document_bytes(document)?;
    let source = crate::assets::source(block, &original, Vendor::Openai, true)?;
    input_file(
        block,
        &source,
        mime,
        document.title.as_deref(),
        Some(document.detail),
    )
}
fn input_file(
    block: &ContentBlock<impl Sized>,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
    detail: Option<Detail>,
) -> Result<i::ResponseInputFile> {
    let mut value =
        i::ResponseInputFile::builder(i::ResponseInputFileType::ResponseInputFile).build();
    let meta = read_native::<FileMetadata>(block, Vendor::Openai)?;
    let raw = meta.as_ref().is_some_and(|m| m.raw_base64);
    let had_detail = meta.as_ref().is_some_and(|m| m.fields.detail.is_some());
    if let Some(mut m) = meta {
        m.fields.file_data = None;
        m.fields.file_url = None;
        m.fields.filename = None;
        if m.fields.file_id.as_ref().is_some_and(Option::is_some) {
            m.fields.file_id = None;
        }
        m.fields.apply_to(&mut value);
    }
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
    if !had_detail && detail == Some(Detail::Auto) && value.detail == Some(i::FileDetail::Auto) {
        value.detail = None;
    }
    value.prompt_cache_breakpoint = media::responses_cache(block)?;
    Ok(value)
}
fn generated_image(block: &Block, image: &Image) -> Result<Option<i::InputItem>> {
    let Some(ImageMetadata::Generated(mut native)) = read_native(block, Vendor::Openai)? else {
        return Ok(None);
    };
    let id = native
        .id
        .take()
        .ok_or_else(|| Error::invalid("generated image is missing native item ID"))?;
    let status = native
        .status
        .take()
        .ok_or_else(|| Error::invalid("generated image is missing native status"))?;
    let Asset::Inline { bytes } = &image.source else {
        return Err(Error::invalid(
            "generated image history requires its inline bytes",
        ));
    };
    let mut value = i::ImageGenerationCall::builder(
        i::ImageGenerationCallType::ImageGenerationCall,
        id,
        None,
        status,
    )
    .build();
    native.result = None;
    native.apply_to(&mut value);
    value.result = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
    Ok(Some(i::InputItem::ImageGenerationCall(value)))
}
fn function_output(block: &ToolResultBlock) -> Result<i::FunctionOutputContent> {
    Ok(match &block.content {
        ToolResultContent::Text(text) => {
            let mut value = i::FunctionOutputText::builder(
                i::ResponseInputTextType::ResponseInputText,
                text.text.clone(),
            )
            .build();
            if let Some(native) = read_native::<i::ResponseInputTextOptions>(block, Vendor::Openai)?
            {
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
    let Some(metadata) = read_native::<ServerMetadata>(block, Vendor::Openai)? else {
        return Ok(None);
    };
    Ok(Some(match metadata {
        ServerMetadata::Web(mut m) => {
            let status = m
                .status
                .take()
                .ok_or_else(|| Error::invalid("web search history is missing status"))?;
            let mut value = i::WebSearchCall::builder(
                i::WebSearchCallType::WebSearchCall,
                call.id.clone(),
                status,
            )
            .build();
            m.id = None;
            m.action = None;
            m.apply_to(&mut value);
            value.action = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            i::InputItem::WebSearchCall(value)
        }
        ServerMetadata::Files(mut m) => {
            let status = m
                .status
                .take()
                .ok_or_else(|| Error::invalid("file search history is missing status"))?;
            let queries = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            let mut value = i::FileSearchCall::builder(
                i::FileSearchCallType::FileSearchCall,
                call.id.clone(),
                queries,
                status,
            )
            .build();
            m.id = None;
            m.queries = None;
            m.apply_to(&mut value);
            i::InputItem::FileSearchCall(value)
        }
        ServerMetadata::Code(mut m) => {
            let id = call.id.clone();
            let status = m
                .status
                .take()
                .ok_or_else(|| Error::invalid("code interpreter history is missing status"))?;
            let container = m.container_id.take().ok_or_else(|| {
                Error::invalid("code interpreter history is missing container ID")
            })?;
            let code = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            let mut value = i::CodeInterpreterCall::builder(
                i::CodeInterpreterCallType::CodeInterpreterCall,
                id,
                code,
                container,
                m.outputs.take(),
                status,
            )
            .build();
            m.id = None;
            m.code = None;
            m.apply_to(&mut value);
            i::InputItem::CodeInterpreterCall(value)
        }
        ServerMetadata::Computer(mut m) => {
            let id =
                m.id.take()
                    .ok_or_else(|| Error::invalid("computer history is missing item ID"))?;
            let action = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
            let status = m
                .status
                .take()
                .ok_or_else(|| Error::invalid("computer call history is missing status"))?;
            let call_id = call.id.clone();
            m.call_id = None;
            let checks = m
                .pending_safety_checks
                .take()
                .ok_or_else(|| Error::invalid("computer history is missing safety checks"))?;
            let mut value = i::ComputerCall::builder(
                i::ComputerCallType::ComputerCall,
                id,
                call_id,
                checks,
                status,
            )
            .build();
            value.action = action;
            m.id = None;
            m.action = None;
            m.apply_to(&mut value);
            i::InputItem::ComputerCall(value)
        }
    }))
}

pub fn render(request: &GenerateRequest, streaming: bool) -> Result<q::GenerateContentRequestBody> {
    let opts: q::GenerateContentRequestBodyOptions = options(request)?;
    let mut body = q::GenerateContentRequestBody::builder().build();
    opts.apply_to(&mut body);
    body.model = Some(request.model.id.clone());
    body.stream = Some(Some(streaming));
    let mut input = Vec::new();
    for message in &request.messages {
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
                    {
                        let part = match native {
                            TextMetadata::Text {
                                mut annotations,
                                logprobs,
                                rest,
                            } => {
                                if !text.citations.is_empty() {
                                    annotations = text
                                        .citations
                                        .iter()
                                        .map(citations::render_responses)
                                        .collect::<Result<Vec<_>>>()?
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                }
                                i::OutputContent::Text(
                                    i::ResponseOutputText::builder(
                                        i::ResponseOutputTextType::ResponseOutputText,
                                        text.text.clone(),
                                        annotations,
                                        logprobs,
                                    )
                                    .rest(rest)
                                    .build(),
                                )
                            }
                            TextMetadata::Refusal { rest } => i::OutputContent::Refusal(
                                i::ResponseOutputRefusal::builder(
                                    i::ResponseOutputRefusalType::ResponseOutputRefusal,
                                    text.text.clone(),
                                )
                                .rest(rest)
                                .build(),
                            ),
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
                        );
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
                            message_role(message.role)?,
                            i::InputContent::Text(text),
                        );
                    }
                }
                Content::Image(image) => {
                    if let Some(generated) = generated_image(block, image)? {
                        flush(pending.take(), &mut input);
                        input.push(generated);
                    } else {
                        push_input(
                            &mut pending,
                            &mut input,
                            message_role(message.role)?,
                            i::InputContent::Image(input_image(block, image)?),
                        );
                    }
                }
                Content::Document(document) => push_input(
                    &mut pending,
                    &mut input,
                    message_role(message.role)?,
                    i::InputContent::File(input_document(block, document)?),
                ),
                Content::File(file) => {
                    let source = crate::assets::source(block, &file.source, Vendor::Openai, true)?;
                    let value = input_file(
                        block,
                        &source,
                        file.media_type.as_deref(),
                        file.filename.as_deref(),
                        None,
                    )?;
                    push_input(
                        &mut pending,
                        &mut input,
                        message_role(message.role)?,
                        i::InputContent::File(value),
                    );
                }
                Content::Reasoning(Reasoning { text }) => {
                    flush(pending.take(), &mut input);
                    if let Some(mut native) =
                        read_native::<i::ReasoningItemOptions>(block, Vendor::Openai)?
                    {
                        let id = native.id.take().ok_or_else(|| {
                            Error::invalid("reasoning metadata is missing item ID")
                        })?;
                        let summary = native.summary.take().ok_or_else(|| {
                            Error::invalid("reasoning metadata is missing summary layout")
                        })?;
                        let original: String = summary.iter().map(|s| s.text.as_str()).collect();
                        let summary = if original == *text {
                            summary
                        } else {
                            vec![
                                i::SummaryText::builder(
                                    i::SummaryTextType::SummaryText,
                                    text.clone(),
                                )
                                .build(),
                            ]
                        };
                        let mut item = i::ReasoningItem::builder(
                            i::ReasoningItemType::ReasoningItem,
                            id,
                            summary,
                        )
                        .build();
                        native.apply_to(&mut item);
                        input.push(i::InputItem::Reasoning(item));
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
                    if let Some(mut meta) =
                        read_native::<i::FunctionCallOptions>(block, Vendor::Openai)?
                    {
                        meta.call_id = None;
                        meta.name = None;
                        meta.arguments = None;
                        meta.apply_to(&mut value);
                    }
                    input.push(i::InputItem::FunctionCall(value));
                }
                Content::ToolResult(result) => {
                    flush(pending.take(), &mut input);
                    if result.provider_executed {
                        continue;
                    }
                    let output = match &result.output {
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
                    if let Some(mut native) =
                        read_native::<i::FunctionCallOutputOptions>(block, Vendor::Openai)?
                    {
                        native.output = None;
                        native.call_id = None;
                        native.apply_to(&mut value);
                    }
                    if !result.id.is_empty() {
                        value.call_id = Some(Some(result.id.clone()));
                    }
                    if !result.name.is_empty() {
                        value.name = Some(Some(result.name.clone()));
                    }
                    input.push(i::InputItem::FunctionCallOutput(value));
                }
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
        body.tool_choice = Some(match choice {
            ToolChoice::Auto => i::ToolChoice::Mode(i::ToolChoiceMode::Auto),
            ToolChoice::None => i::ToolChoice::Mode(i::ToolChoiceMode::None),
            ToolChoice::Required => i::ToolChoice::Mode(i::ToolChoiceMode::Required),
            ToolChoice::Tool { name } => i::ToolChoice::Function(
                i::ToolChoiceFunction::builder(
                    i::ToolChoiceFunctionType::ToolChoiceFunction,
                    name.clone(),
                )
                .build(),
            ),
        });
    }
    if let Some(schema) = &request.settings.output_schema {
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
    Ok(body)
}

pub fn parse(response: r::GenerateContentResponseBody) -> Result<GenerateResult> {
    let (finish_reason, usage) = finish_metadata(&response)?;
    let mut content = Vec::new();
    for (index, item) in response.output.into_iter().enumerate() {
        content.extend(parse_item(item, index as u32)?);
    }
    Ok(GenerateResult {
        content,
        finish_reason,
        usage,
    })
}
pub(crate) fn finish_metadata(
    response: &r::GenerateContentResponseBody,
) -> Result<(FinishReason, Usage)> {
    if let Some(error) = &response.error {
        #[derive(Serialize)]
        struct Envelope<'a> {
            error: &'a r::ResponseError,
        }
        return Err(Error::from_response(
            200,
            &http::HeaderMap::new(),
            serde_json::to_string(&Envelope { error }).map_err(Error::invalid)?,
        ));
    }
    let mut reason = if let Some(incomplete) = response
        .incomplete_details
        .as_ref()
        .and_then(|details| details.reason)
    {
        match incomplete {
            r::ResponseIncompleteReason::MaxOutputTokens => FinishReason::Length,
            r::ResponseIncompleteReason::ContentFilter => FinishReason::ContentBlocked,
            r::ResponseIncompleteReason::MaxMessages => FinishReason::Other("max_messages".into()),
            r::ResponseIncompleteReason::Steered => FinishReason::Other("steered".into()),
            r::ResponseIncompleteReason::Interrupted => FinishReason::Other("interrupted".into()),
        }
    } else {
        match response.status {
            Some(r::ResponseStatus::Completed) => FinishReason::Stop,
            Some(r::ResponseStatus::Failed) => FinishReason::Other("failed".into()),
            Some(r::ResponseStatus::Incomplete) => FinishReason::Other("incomplete".into()),
            Some(r::ResponseStatus::Cancelled) => FinishReason::Other("cancelled".into()),
            Some(r::ResponseStatus::Queued) => FinishReason::Other("queued".into()),
            Some(r::ResponseStatus::InProgress) => FinishReason::Other("in_progress".into()),
            None => FinishReason::Other("unknown".into()),
        }
    };
    if reason == FinishReason::Stop
        && response
            .output
            .iter()
            .any(|item| matches!(item, r::ResponseOutputItem::FunctionCall(_)))
    {
        reason = FinishReason::ToolCalls;
    }
    let refused = response.output.iter().any(|item| match item {
        r::ResponseOutputItem::Message(message) => message
            .content
            .iter()
            .any(|part| matches!(part, i::OutputContent::Refusal(_))),
        _ => false,
    });
    if refused {
        reason = FinishReason::ContentBlocked;
    }
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
    Ok((reason, usage))
}

pub(crate) fn parse_item(item: r::ResponseOutputItem, index: u32) -> Result<Vec<Block>> {
    Ok(match item {
        r::ResponseOutputItem::Message(mut message) => {
            let parts = std::mem::take(&mut message.content);
            let mut envelope = i::ResponseOutputMessageOptions::from(message);
            envelope.content = None;
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
                                    text: value.text,
                                    citations,
                                }),
                                TextMetadata::Text {
                                    annotations: Vec::new(),
                                    logprobs: value.logprobs,
                                    rest: value.rest,
                                },
                            )
                        }
                        i::OutputContent::Refusal(value) => (
                            Content::Text(Text {
                                text: value.refusal,
                                citations: Vec::new(),
                            }),
                            TextMetadata::Refusal { rest: value.rest },
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
            let mut meta = i::FunctionCallOptions::from(call);
            meta.call_id = None;
            meta.name = None;
            meta.arguments = None;
            vec![with_native(content, Vendor::Openai, meta)?]
        }
        r::ResponseOutputItem::Reasoning(item) => {
            let text = item.summary.iter().map(|p| p.text.as_str()).collect();
            vec![with_native(
                Content::Reasoning(Reasoning { text }),
                Vendor::Openai,
                item,
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
                    ImageMetadata::Generated(i::ImageGenerationCallOptions::from(value)),
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
            let mut meta = i::WebSearchCallOptions::from(value);
            meta.id = None;
            meta.action = None;
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Web(meta),
            )?]
        }
        r::ResponseOutputItem::FileSearchCall(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "file_search".into(),
                arguments: serde_json::to_string(&value.queries).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut meta = i::FileSearchCallOptions::from(value);
            meta.id = None;
            meta.queries = None;
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Files(meta),
            )?]
        }
        r::ResponseOutputItem::CodeInterpreterCall(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: "code_interpreter".into(),
                arguments: serde_json::to_string(&value.code).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut meta = i::CodeInterpreterCallOptions::from(value);
            meta.id = None;
            meta.code = None;
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Code(meta),
            )?]
        }
        r::ResponseOutputItem::ComputerCall(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.call_id),
                name: "computer".into(),
                arguments: serde_json::to_string(&value.action).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut meta = i::ComputerCallOptions::from(value);
            meta.call_id = None;
            meta.action = None;
            vec![with_native(
                content,
                Vendor::Openai,
                ServerMetadata::Computer(meta),
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
    let content = Content::ToolResult(ToolResult {
        id: output.call_id.unwrap_or_default(),
        name: output.name.clone().unwrap_or_default(),
        output: output_content,
        is_error: false,
        provider_executed: false,
    });
    let mut native = i::FunctionCallOutputOptions {
        type_: Some(output.type_),
        id: Some(Some(output.id)),
        agent: output.agent,
        caller: output.caller,
        name: output.name.filter(|n| n.is_empty()).map(Some),
        namespace: output.namespace.map(Some),
        status: Some(Some(output.status)),
        rest: output.rest,
        ..Default::default()
    };
    if let Some(created_by) = output.created_by {
        #[derive(Serialize)]
        struct CreatedBy {
            created_by: String,
        }
        native.rest.extend(encode_rest(CreatedBy { created_by })?);
    }
    with_native(content, Vendor::Openai, native)
}
fn parse_output_part(part: i::InputContent) -> Result<ToolResultBlock> {
    match part {
        i::InputContent::Text(mut value) => {
            let cache = value.prompt_cache_breakpoint.take();
            let text = std::mem::take(&mut value.text);
            let mut native = i::ResponseInputTextOptions::from(value);
            native.text = None;
            let mut block = with_native(
                ToolResultContent::Text(Text {
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
            let mut meta = i::ResponseInputImageOptions::from(value);
            meta.detail = None;
            meta.file_id = None;
            meta.image_url = None;
            meta.prompt_cache_breakpoint = None;
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
            let mut fields = i::ResponseInputFileOptions::from(value);
            fields.file_data = None;
            fields.file_url = None;
            fields.file_id = None;
            fields.filename = None;
            fields.prompt_cache_breakpoint = None;
            let mut block = with_native(
                ToolResultContent::Document(document),
                Vendor::Openai,
                FileMetadata {
                    raw_base64: raw,
                    fields,
                },
            )?;
            media::parse_responses_cache(&mut block, cache)?;
            Ok(block)
        }
    }
}
