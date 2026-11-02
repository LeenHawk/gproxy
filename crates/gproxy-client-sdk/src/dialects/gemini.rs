use super::*;
use gproxy_protocol::gemini as g;

// The flattened projection preserves native fields that do not have an IR
// counterpart. Payload-bearing children use their own partial projections.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_data: Option<g::BlobOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_data: Option<g::FileDataOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<g::FunctionCallOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_response: Option<g::FunctionResponseOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audio_transcription: Option<g::audio::AudioTranscriptionOptions>,
    #[serde(default)]
    response_json_text: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call: Option<g::ToolCallOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_response: Option<g::ToolResponseOptions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    trailing_transcripts: Vec<TranscriptMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    primary_transcript_chars: Option<usize>,
    #[serde(flatten)]
    fields: g::PartOptions,
}

#[derive(Serialize, Deserialize)]
struct TranscriptMetadata {
    fields: g::PartOptions,
    transcription: g::audio::AudioTranscriptionOptions,
    chars: usize,
}
pub(crate) fn merge_transcription(block: &mut Block, part: &g::Part) -> Result<Option<String>> {
    let Content::Audio(audio) = &mut block.content else {
        return Ok(None);
    };
    let Some(transcript) = &part.audio_transcription else {
        return Ok(None);
    };
    if part.inline_data.is_some() || part.file_data.is_some() || part.text.is_some() {
        return Ok(None);
    }
    // The metadata records boundaries and extensions, not a second transcript.
    let mut meta = match block.provider_metadata.get(&Vendor::Gemini) {
        Some(rest) => serde_json::from_value::<Native<PartMetadata>>(Value::Object(rest.clone()))
            .map_err(Error::invalid)?
            .native
            .unwrap_or_default(),
        None => PartMetadata::default(),
    };
    let text = audio.transcript.get_or_insert_with(String::new);
    if meta.primary_transcript_chars.is_none() {
        meta.primary_transcript_chars = Some(text.chars().count());
    }
    text.push_str(&transcript.text);
    let mut fields = g::PartOptions::from(part.clone());
    fields.audio_transcription = None;
    let mut native = g::audio::AudioTranscriptionOptions::from(transcript.clone());
    native.text = None;
    meta.trailing_transcripts.push(TranscriptMetadata {
        fields,
        transcription: native,
        chars: transcript.text.chars().count(),
    });
    block
        .provider_metadata
        .entry(Vendor::Gemini)
        .or_default()
        .extend(encode_rest(Native { native: Some(meta) })?);
    Ok(Some(transcript.text.clone()))
}
fn trailing_transcripts(block: &Block) -> Result<Vec<g::Part>> {
    let Content::Audio(audio) = &block.content else {
        return Ok(Vec::new());
    };
    let Some(meta) = read_native::<PartMetadata>(block, Vendor::Gemini)? else {
        return Ok(Vec::new());
    };
    let mut chars = audio
        .transcript
        .as_deref()
        .unwrap_or_default()
        .chars()
        .skip(meta.primary_transcript_chars.unwrap_or(0));
    let count = meta.trailing_transcripts.len();
    let mut parts = Vec::with_capacity(count);
    for (index, meta) in meta.trailing_transcripts.into_iter().enumerate() {
        let text = if index + 1 == count {
            chars.by_ref().collect()
        } else {
            chars.by_ref().take(meta.chars).collect()
        };
        let mut part = g::Part::builder().build();
        meta.fields.apply_to(&mut part);
        let mut transcription = g::audio::AudioTranscription::builder(text).build();
        meta.transcription.apply_to(&mut transcription);
        part.audio_transcription = Some(transcription);
        parts.push(part);
    }
    Ok(parts)
}
pub fn render(request: &GenerateRequest) -> Result<g::GenerateContentRequestBody> {
    let opts: g::GenerateContentRequestBodyOptions = options(request)?;
    let mut body = g::GenerateContentRequestBody::builder(Vec::new()).build();
    opts.apply_to(&mut body);
    let mut contents = Vec::new();
    let mut system = Vec::new();
    let mut calls = BTreeMap::new();
    for message in &request.messages {
        let mut parts = Vec::new();
        for block in &message.content {
            if let Some(part) = render_block(block, &calls)? {
                parts.push(part);
                parts.extend(trailing_transcripts(block)?);
            }
            if let Content::ToolCall(ToolCall { id, name, .. }) = &block.content {
                calls.insert(id.as_str(), name.as_str());
            }
        }
        if message.role == Role::System {
            system.extend(parts);
        } else {
            contents.push(
                g::Content::builder()
                    .role(if message.role == Role::Assistant {
                        "model".to_owned()
                    } else {
                        "user".to_owned()
                    })
                    .parts(parts)
                    .build(),
            );
        }
    }
    body.contents = contents;
    if !system.is_empty() {
        body.system_instruction = Some(g::Content::builder().parts(system).build());
    }
    if request.settings != Settings::default() {
        let config = body
            .generation_config
            .get_or_insert_with(|| g::GenerationConfig::builder().build());
        if let Some(value) = request.settings.temperature {
            config.temperature = Some(value);
        }
        if let Some(value) = request.settings.top_p {
            config.top_p = Some(value);
        }
        if let Some(value) = request.settings.max_output_tokens {
            config.max_output_tokens = Some(value.into());
        }
        if !request.settings.stop_sequences.is_empty() {
            config.stop_sequences = Some(request.settings.stop_sequences.clone());
        }
        if let Some(schema) = &request.settings.output_schema {
            config.response_mime_type = Some("application/json".into());
            config.response_json_schema = Some(schema.clone());
        }
    }
    if !request.settings.output_modalities.is_empty() {
        let config = body
            .generation_config
            .get_or_insert_with(|| g::GenerationConfig::builder().build());
        config.response_modalities = Some(
            request
                .settings
                .output_modalities
                .iter()
                .map(|m| match m {
                    Modality::Text => g::Modality::Text,
                    Modality::Audio => g::Modality::Audio,
                    Modality::Image => g::Modality::Image,
                })
                .collect(),
        );
    }
    if let Some(speech) = &request.settings.speech {
        match &speech.format {
            AudioFormat::Pcm {
                sample_rate: 24000,
                channels: 1,
                ..
            } => {}
            _ => {
                return Err(Error::invalid(
                    "Gemini speech output is 24 kHz mono PCM; no transcoding is performed",
                ));
            }
        }
        let config = body
            .generation_config
            .get_or_insert_with(|| g::GenerationConfig::builder().build());
        config.speech_config = Some(
            g::SpeechConfig::builder()
                .voice_config(
                    g::VoiceConfig::builder()
                        .prebuilt_voice_config(
                            g::PrebuiltVoiceConfig::builder()
                                .voice_name(speech.voice.clone())
                                .build(),
                        )
                        .build(),
                )
                .build(),
        );
    }
    if !request.tools.is_empty() {
        let declarations: Vec<_> = request
            .tools
            .iter()
            .map(|tool| {
                g::FunctionDeclaration::builder(tool.name.clone(), tool.description.clone())
                    .parameters_json_schema(tool.input_schema.clone())
                    .build()
            })
            .collect();
        body.tools = Some(vec![
            g::Tool::builder()
                .function_declarations(declarations)
                .build(),
        ]);
    }
    if let Some(choice) = &request.settings.tool_choice {
        let mut config = g::FunctionCallingConfig::builder().build();
        config.mode = Some(match choice {
            ToolChoice::Auto => g::FunctionCallingMode::Auto,
            ToolChoice::None => g::FunctionCallingMode::None,
            ToolChoice::Required | ToolChoice::Tool { .. } => g::FunctionCallingMode::Any,
        });
        if let ToolChoice::Tool { name } = choice {
            config.allowed_function_names = Some(vec![name.clone()]);
        }
        let tool_config = body
            .tool_config
            .get_or_insert_with(|| g::ToolConfig::builder().build());
        tool_config.function_calling_config = Some(config);
    }
    Ok(body)
}

fn render_block(block: &Block, calls: &BTreeMap<&str, &str>) -> Result<Option<g::Part>> {
    if matches!(block.content, Content::Opaque) {
        return read_native(block, Vendor::Gemini);
    }
    let metadata = read_native::<PartMetadata>(block, Vendor::Gemini)?;
    if matches!(block.content, Content::Reasoning(_)) && metadata.is_none() {
        return Ok(None);
    }
    let had_metadata = metadata.is_some();
    let meta = metadata.unwrap_or_default();
    let mut part = g::Part::builder().build();
    meta.fields.apply_to(&mut part);
    match &block.content {
        Content::Text(text) => part.text = Some(text.text.clone()),
        Content::Reasoning(Reasoning { text }) => {
            part.text = Some(text.clone());
            part.thought = Some(true);
        }
        Content::Image(image) => {
            let source = crate::assets::source(block, &image.source, Vendor::Gemini, true)?;
            render_asset(
                &mut part,
                &source,
                image.media_type.as_deref(),
                None,
                meta.inline_data,
                meta.file_data,
            )?;
            set_detail(&mut part, image.detail);
        }
        Content::Document(document) => {
            let (asset, mime) = media::document_bytes(document)?;
            let source = crate::assets::source(block, &asset, Vendor::Gemini, true)?;
            render_asset(
                &mut part,
                &source,
                mime,
                document.title.as_deref(),
                meta.inline_data,
                meta.file_data,
            )?;
            set_detail(&mut part, document.detail);
        }
        Content::File(file) => {
            let source = crate::assets::source(block, &file.source, Vendor::Gemini, true)?;
            render_asset(
                &mut part,
                &source,
                file.media_type.as_deref(),
                file.filename.as_deref(),
                meta.inline_data,
                meta.file_data,
            )?;
        }
        Content::Audio(audio) => {
            let mut mime = media::pcm_mime(&audio.format)?;
            if let Some(original) = meta
                .inline_data
                .as_ref()
                .and_then(|m| m.mime_type.as_deref())
                .or_else(|| meta.file_data.as_ref().and_then(|m| m.mime_type.as_deref()))
            {
                if media::parse_audio_format(original)? == audio.format {
                    mime = original.into();
                }
            }
            let source = crate::assets::source(block, &audio.source, Vendor::Gemini, true)?;
            render_asset(
                &mut part,
                &source,
                Some(&mime),
                None,
                meta.inline_data,
                meta.file_data,
            )?;
            if let Some(transcript) = &audio.transcript {
                let primary = meta.primary_transcript_chars;
                if primary != Some(0) {
                    let text = match primary {
                        Some(count) => transcript.chars().take(count).collect(),
                        None => transcript.clone(),
                    };
                    let mut value = g::audio::AudioTranscription::builder(text).build();
                    if let Some(mut native) = meta.audio_transcription {
                        native.text = None;
                        native.apply_to(&mut value);
                    }
                    part.audio_transcription = Some(value);
                }
            }
        }
        Content::Video(video) => {
            let source = crate::assets::source(block, &video.source, Vendor::Gemini, true)?;
            render_asset(
                &mut part,
                &source,
                video.media_type.as_deref(),
                None,
                meta.inline_data,
                meta.file_data,
            )?;
            if let Some(clip) = &video.clip {
                let value = part
                    .video_metadata
                    .get_or_insert_with(|| g::VideoMetadata::builder().build());
                value.start_offset = duration_field(value.start_offset.take(), clip.start_seconds)?;
                value.end_offset = duration_field(value.end_offset.take(), clip.end_seconds)?;
                value.fps = clip.fps;
            }
        }
        Content::ToolCall(call) => {
            if call.provider_executed {
                let Some(mut native) = meta.tool_call else {
                    return Ok(None);
                };
                let tool_type = native.tool_type.clone().ok_or_else(|| {
                    Error::invalid("Gemini server tool metadata requires tool_type")
                })?;
                let mut value = g::ToolCall::builder(tool_type).build();
                if native.id.is_some() {
                    native.id = Some(call.id.clone());
                }
                if native.tool_name.is_some() {
                    native.tool_name = Some(call.name.clone());
                }
                if native.args.is_some() || call.arguments != "{}" {
                    native.args =
                        Some(serde_json::from_str(&call.arguments).map_err(Error::invalid)?);
                }
                native.apply_to(&mut value);
                part.tool_call = Some(value);
                return Ok(Some(part));
            }
            let include_args = meta
                .function_call
                .as_ref()
                .is_some_and(|m| m.args.is_some())
                || !had_metadata
                || call.arguments != "{}";
            let include_id =
                meta.function_call.as_ref().is_some_and(|m| m.id.is_some()) || !had_metadata;
            let mut value = g::FunctionCall::builder(call.name.clone()).build();
            if let Some(mut native) = meta.function_call {
                native.name = None;
                native.apply_to(&mut value);
            }
            if include_args {
                value.args = Some(serde_json::from_str(&call.arguments).map_err(Error::invalid)?);
            }
            if include_id && !call.id.is_empty() {
                value.id = Some(call.id.clone());
            }
            part.function_call = Some(value);
        }
        Content::ToolResult(result) => {
            if result.provider_executed {
                let Some(mut native) = meta.tool_response else {
                    return Ok(None);
                };
                let tool_type = native.tool_type.clone().ok_or_else(|| {
                    Error::invalid("Gemini server tool metadata requires tool_type")
                })?;
                let mut value = g::ToolResponse::builder(tool_type).build();
                if native.id.is_some() {
                    native.id = Some(result.id.clone());
                }
                let ToolOutput::Json(output) = &result.output else {
                    return Err(Error::invalid("Gemini server tool output requires JSON"));
                };
                if !output.is_null() {
                    native.response =
                        Some(serde_json::from_value(output.clone()).map_err(Error::invalid)?);
                }
                native.apply_to(&mut value);
                part.tool_response = Some(value);
                return Ok(Some(part));
            }
            let name = tool_name(&result.name, &result.id, calls)?;
            let mut parts = Vec::new();
            let response = match &result.output {
                ToolOutput::Json(Value::Object(object)) => object.clone(),
                ToolOutput::Json(value) => encode_rest(if result.is_error {
                    ToolOutputJson::Error { error: value }
                } else {
                    ToolOutputJson::Result { result: value }
                })?,
                ToolOutput::Blocks(blocks) => {
                    let mut text = Vec::new();
                    for (index, block) in blocks.iter().enumerate() {
                        if meta.response_json_text && index == 0 {
                            continue;
                        }
                        match &block.content {
                            ToolResultContent::Text(value) => text.push(value.text.as_str()),
                            ToolResultContent::Image(value) => parts.push(response_part(
                                block,
                                &value.source,
                                value.media_type.as_deref(),
                                None,
                            )?),
                            ToolResultContent::Document(value) => {
                                let (source, mime) = media::document_bytes(value)?;
                                parts.push(response_part(
                                    block,
                                    &source,
                                    mime,
                                    value.title.as_deref(),
                                )?);
                            }
                        }
                    }
                    #[derive(Serialize)]
                    struct Output<'a> {
                        result: Vec<&'a str>,
                    }
                    if meta.response_json_text {
                        let Some(ContentBlock {
                            content: ToolResultContent::Text(value),
                            ..
                        }) = blocks.first()
                        else {
                            return Err(Error::invalid("Gemini response JSON text is missing"));
                        };
                        serde_json::from_str(&value.text).map_err(Error::invalid)?
                    } else {
                        encode_rest(Output { result: text })?
                    }
                }
            };
            let mut value = g::FunctionResponse::builder(name.to_owned(), response).build();
            if let Some(mut native) = meta.function_response {
                native.name = None;
                native.response = None;
                native.parts = None;
                native.apply_to(&mut value);
            }
            if !parts.is_empty() {
                value.parts = Some(parts);
            }
            if !result.id.is_empty() && (!had_metadata || value.id.is_some()) {
                value.id = Some(result.id.clone());
            }
            part.function_response = Some(value);
        }
        Content::Opaque => return Err(Error::invalid("opaque Gemini part has no native value")),
    }
    Ok(Some(part))
}
fn response_part(
    block: &ToolResultBlock,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
) -> Result<g::FunctionResponsePart> {
    #[derive(Deserialize)]
    struct Envelope {
        function_result_part: Option<ResponsePartMetadata>,
    }
    let meta =
        read_metadata::<Envelope>(block, Vendor::Gemini)?.and_then(|v| v.function_result_part);
    media::check_file_vendor(source, Vendor::Gemini)?;
    let mut part = g::FunctionResponsePart::builder().build();
    match source {
        Asset::Inline { bytes } => {
            part.inline_data = Some(
                g::Blob::builder(
                    media::mime(mime, source)?.into(),
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                )
                .build(),
            )
        }
        Asset::Url { .. } | Asset::ProviderFile { .. } => {
            return Err(Error::invalid(
                "Gemini function response media requires inline bytes",
            ));
        }
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "prepare application references before rendering tool output",
            ));
        }
    }
    if let (Some(name), Some(blob)) = (filename, part.inline_data.as_mut()) {
        blob.display_name = Some(name.into());
    }
    if let Some(meta) = meta {
        let mut fields = meta.fields;
        fields.inline_data = None;
        fields.apply_to(&mut part);
        if let (Some(mut native), Some(blob)) = (meta.inline_data, part.inline_data.as_mut()) {
            native.data = None;
            native.mime_type = None;
            native.apply_to(blob);
        }
    }
    Ok(part)
}
fn set_detail(part: &mut g::Part, detail: Detail) {
    if let Some(mapped) = media::gemini_detail(detail) {
        if let Some(native) = &mut part.media_resolution {
            native.level = mapped.level;
        } else {
            part.media_resolution = Some(mapped);
        }
    }
}
fn render_asset(
    part: &mut g::Part,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
    inline: Option<g::BlobOptions>,
    file: Option<g::FileDataOptions>,
) -> Result<()> {
    match source {
        Asset::Inline { bytes } => {
            let mut value = g::Blob::builder(
                media::mime(mime, source)?.into(),
                base64::engine::general_purpose::STANDARD.encode(bytes),
            )
            .build();
            if let Some(mut native) = inline {
                native.data = None;
                native.mime_type = None;
                native.apply_to(&mut value);
            }
            if let Some(name) = filename {
                value.display_name = Some(name.into());
            }
            part.inline_data = Some(value);
            part.file_data = None;
        }
        Asset::Url { url } | Asset::ProviderFile { id: url, .. } => {
            let mut value = g::FileData::builder(url.clone()).build();
            if let Some(mut native) = file {
                native.file_uri = None;
                native.apply_to(&mut value);
            }
            if let Some(mime) = mime {
                value.mime_type = Some(mime.into());
            }
            if let Some(name) = filename {
                value.display_name = Some(name.into());
            }
            part.file_data = Some(value);
            part.inline_data = None;
        }
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "prepare application assets before rendering",
            ));
        }
    }
    Ok(())
}
fn duration_field(original: Option<String>, value: Option<f64>) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    if !value.is_finite() || value < 0.0 {
        return Err(Error::invalid(
            "video offsets must be finite and nonnegative",
        ));
    }
    if let Some(original) = original
        && parse_duration(&original)? == value
    {
        return Ok(Some(original));
    }
    Ok(Some(format!("{value:.9}s")))
}
fn parse_duration(value: &str) -> Result<f64> {
    value
        .strip_suffix('s')
        .ok_or_else(|| Error::invalid("Gemini offsets require seconds"))?
        .parse()
        .map_err(Error::invalid)
}

pub fn parse(response: g::GenerateContentResponseBody) -> Result<GenerateResult> {
    check_response(&response)?;
    let usage = parse_usage(response.usage_metadata.as_ref())?;
    let candidate = response.candidates.into_iter().flatten().next();
    let reason = candidate.as_ref().and_then(|c| c.finish_reason);
    let links = candidate
        .as_ref()
        .map(citations::gemini_citations)
        .transpose()?
        .unwrap_or_default();
    let mut content = Vec::new();
    for (index, part) in candidate
        .into_iter()
        .filter_map(|c| c.content)
        .flat_map(|c| c.parts.into_iter().flatten())
        .enumerate()
    {
        if let Some(block) = content.last_mut()
            && merge_transcription(block, &part)?.is_some()
        {
            continue;
        }
        content.push(parse_block(part, index as u32)?);
    }
    for (index, citation) in links {
        if let Some(Block {
            content: Content::Text(text),
            ..
        }) = content.get_mut(index)
        {
            text.citations.push(citation);
        }
    }
    let has_tools = content
        .iter()
        .any(|b| matches!(b.content, Content::ToolCall(_)));
    Ok(GenerateResult {
        content,
        finish_reason: finish_reason(reason, has_tools),
        usage,
    })
}

pub(crate) fn check_response(response: &g::GenerateContentResponseBody) -> Result<()> {
    if let Some(error) = &response.error {
        #[derive(Serialize)]
        struct Envelope<'a> {
            error: &'a g::GenerateContentError,
        }
        return Err(Error::from_response(
            200,
            &http::HeaderMap::new(),
            serde_json::to_string(&Envelope { error }).map_err(Error::invalid)?,
        ));
    }
    if response
        .prompt_feedback
        .as_ref()
        .and_then(|p| p.block_reason)
        .is_some_and(|r| r != g::BlockReason::Unspecified)
    {
        return Err(Error::ContentBlocked {
            body: serde_json::to_string(response).map_err(Error::invalid)?,
        });
    }
    Ok(())
}

pub(crate) fn parse_usage(value: Option<&g::UsageMetadata>) -> Result<Usage> {
    let Some(value) = value else {
        return Ok(Usage::default());
    };
    Ok(Usage {
        input_tokens: optional_count(value.prompt_token_count)?,
        output_tokens: optional_count(value.candidates_token_count)?,
        reasoning_tokens: optional_count(value.thoughts_token_count)?,
        cache_read_tokens: optional_count(value.cached_content_token_count)?,
        cache_write_tokens: None,
    })
}

pub(crate) fn finish_reason(reason: Option<g::FinishReason>, has_tools: bool) -> FinishReason {
    match reason {
        Some(g::FinishReason::Stop) if has_tools => FinishReason::ToolCalls,
        Some(g::FinishReason::Stop) => FinishReason::Stop,
        Some(g::FinishReason::MaxTokens) => FinishReason::Length,
        Some(
            g::FinishReason::Safety
            | g::FinishReason::Blocklist
            | g::FinishReason::ProhibitedContent
            | g::FinishReason::Spii
            | g::FinishReason::ImageSafety,
        ) => FinishReason::ContentBlocked,
        None | Some(g::FinishReason::Unspecified) => {
            FinishReason::Other("FINISH_REASON_UNSPECIFIED".into())
        }
        Some(g::FinishReason::Continuation) => FinishReason::Other("CONTINUATION".into()),
        Some(g::FinishReason::Recitation) => FinishReason::Other("RECITATION".into()),
        Some(g::FinishReason::Language) => FinishReason::Other("LANGUAGE".into()),
        Some(g::FinishReason::Other) => FinishReason::Other("OTHER".into()),
        Some(g::FinishReason::MalformedFunctionCall) => {
            FinishReason::Other("MALFORMED_FUNCTION_CALL".into())
        }
        Some(g::FinishReason::ImageProhibitedContent) => {
            FinishReason::Other("IMAGE_PROHIBITED_CONTENT".into())
        }
        Some(g::FinishReason::ImageOther) => FinishReason::Other("IMAGE_OTHER".into()),
        Some(g::FinishReason::NoImage) => FinishReason::Other("NO_IMAGE".into()),
        Some(g::FinishReason::ImageRecitation) => FinishReason::Other("IMAGE_RECITATION".into()),
        Some(g::FinishReason::UnexpectedToolCall) => {
            FinishReason::Other("UNEXPECTED_TOOL_CALL".into())
        }
        Some(g::FinishReason::TooManyToolCalls) => {
            FinishReason::Other("TOO_MANY_TOOL_CALLS".into())
        }
        Some(g::FinishReason::MissingThoughtSignature) => {
            FinishReason::Other("MISSING_THOUGHT_SIGNATURE".into())
        }
        Some(g::FinishReason::MalformedResponse) => {
            FinishReason::Other("MALFORMED_RESPONSE".into())
        }
        Some(g::FinishReason::Escalation) => FinishReason::Other("ESCALATION".into()),
        Some(g::FinishReason::PupLimitedDisabled) => {
            FinishReason::Other("PUP_LIMITED_DISABLED".into())
        }
    }
}

fn parse_blob(
    blob: g::Blob,
    detail: Detail,
    transcription: Option<g::audio::AudioTranscription>,
) -> Result<(
    Content,
    g::BlobOptions,
    Option<g::audio::AudioTranscriptionOptions>,
)> {
    let g::Blob {
        mime_type,
        data,
        display_name,
        rest,
    } = blob;
    let source = decode_inline(&data)?;
    let base = mime_type.split(';').next().unwrap_or(&mime_type);
    let mut native = g::BlobOptions {
        mime_type: None,
        data: None,
        display_name: display_name.clone(),
        rest,
    };
    let mut audio_meta = None;
    let content = if base.starts_with("image/") {
        native.display_name = display_name;
        Content::Image(Image {
            source,
            media_type: Some(mime_type),
            detail,
        })
    } else if base.starts_with("audio/") {
        let format = media::parse_audio_format(&mime_type)?;
        if media::pcm_mime(&format)? != mime_type {
            native.mime_type = Some(mime_type);
        }
        let transcript = transcription.map(|mut value| {
            let text = std::mem::take(&mut value.text);
            let mut meta = g::audio::AudioTranscriptionOptions::from(value);
            meta.text = None;
            audio_meta = Some(meta);
            text
        });
        Content::Audio(Audio {
            source,
            format,
            transcript,
        })
    } else if base.starts_with("video/") {
        Content::Video(Video {
            source,
            media_type: Some(mime_type),
            clip: None,
        })
    } else if matches!(base, "application/pdf" | "text/plain") {
        Content::Document(Document {
            source: DocumentSource::Asset { source },
            media_type: Some(mime_type),
            title: display_name,
            context: None,
            citations: None,
            detail,
        })
    } else {
        Content::File(File {
            source,
            media_type: Some(mime_type),
            filename: display_name,
        })
    };
    if matches!(content, Content::Document(_) | Content::File(_)) {
        native.display_name = None;
    }
    Ok((content, native, audio_meta))
}
fn parse_file(file: g::FileData, detail: Detail) -> Result<(Content, g::FileDataOptions)> {
    let g::FileData {
        file_uri,
        mime_type,
        display_name,
        rest,
    } = file;
    let source = if file_uri.starts_with("https://generativelanguage.googleapis.com/v1beta/files/")
        || file_uri.starts_with("files/")
    {
        Asset::ProviderFile {
            vendor: Vendor::Gemini,
            id: file_uri,
        }
    } else {
        Asset::Url { url: file_uri }
    };
    let base = mime_type.as_deref().and_then(|s| s.split(';').next());
    let mut native = g::FileDataOptions {
        file_uri: None,
        mime_type: None,
        display_name: display_name.clone(),
        rest,
    };
    let content = if base.is_some_and(|s| s.starts_with("image/")) {
        Content::Image(Image {
            source,
            media_type: mime_type,
            detail,
        })
    } else if base.is_some_and(|s| s.starts_with("audio/")) {
        let format = media::parse_audio_format(mime_type.as_deref().unwrap_or_default())?;
        if let Some(mime) = mime_type
            && media::pcm_mime(&format)? != mime
        {
            native.mime_type = Some(mime);
        }
        Content::Audio(Audio {
            source,
            format,
            transcript: None,
        })
    } else if base.is_some_and(|s| s.starts_with("video/")) {
        Content::Video(Video {
            source,
            media_type: mime_type,
            clip: None,
        })
    } else if matches!(base, Some("application/pdf" | "text/plain")) {
        Content::Document(Document {
            source: DocumentSource::Asset { source },
            media_type: mime_type,
            title: display_name,
            context: None,
            citations: None,
            detail,
        })
    } else {
        Content::File(File {
            source,
            media_type: mime_type,
            filename: display_name,
        })
    };
    if matches!(content, Content::Document(_) | Content::File(_)) {
        native.display_name = None;
    }
    Ok((content, native))
}
pub(crate) fn parse_block(part: g::Part, index: u32) -> Result<Block> {
    let mut fields = g::PartOptions::from(part);
    let mut meta = PartMetadata::default();
    let detail = media::parse_gemini_detail(fields.media_resolution.as_ref());
    if detail != Detail::Auto
        && let Some(resolution) = fields.media_resolution.as_mut()
    {
        resolution.level = None;
    }

    let content = if let Some(text) = fields.text.take() {
        if fields.thought == Some(true) {
            Content::Reasoning(Reasoning { text })
        } else {
            Content::Text(Text {
                text,
                citations: Vec::new(),
            })
        }
    } else if let Some(call) = fields.function_call.take() {
        let g::FunctionCall {
            name,
            id,
            args,
            rest,
        } = call;
        let portable_id = id.clone().unwrap_or_else(|| format!("call_gemini_{index}"));
        let arguments = match &args {
            Some(args) => serde_json::to_string(args).map_err(Error::invalid)?,
            None => "{}".into(),
        };
        meta.function_call = Some(g::FunctionCallOptions {
            name: None,
            id,
            args: args.map(|_| Rest::new()),
            rest,
        });
        Content::ToolCall(ToolCall {
            id: portable_id,
            name,
            arguments,
            provider_executed: false,
        })
    } else if let Some(output) = fields.function_response.take() {
        let g::FunctionResponse {
            name,
            response,
            id,
            parts,
            will_continue,
            scheduling,
            rest,
        } = output;
        let portable_id = id.clone().unwrap_or_default();
        let is_error = response.contains_key("error");
        let output = if let Some(parts) = parts.filter(|p| !p.is_empty()) {
            let mut blocks = vec![ContentBlock::new(ToolResultContent::Text(Text {
                text: serde_json::to_string(&response).map_err(Error::invalid)?,
                citations: Vec::new(),
            }))];
            for part in parts {
                blocks.push(parse_response_part(part)?);
            }
            meta.response_json_text = true;
            ToolOutput::Blocks(blocks)
        } else {
            ToolOutput::Json(Value::Object(response))
        };
        meta.function_response = Some(g::FunctionResponseOptions {
            name: None,
            response: None,
            id,
            parts: None,
            will_continue,
            scheduling,
            rest,
        });
        Content::ToolResult(ToolResult {
            id: portable_id,
            name,
            output,
            is_error,
            provider_executed: false,
        })
    } else if let Some(blob) = fields.inline_data.take() {
        let (mut content, inline, audio) =
            parse_blob(blob, detail, fields.audio_transcription.take())?;
        meta.inline_data = Some(inline);
        meta.audio_transcription = audio;
        if let Content::Video(video) = &mut content {
            video.clip = parse_video(fields.video_metadata.as_ref())?;
        }
        content
    } else if let Some(file) = fields.file_data.take() {
        let (mut content, file) = parse_file(file, detail)?;
        meta.file_data = Some(file);
        if let Content::Video(video) = &mut content {
            video.clip = parse_video(fields.video_metadata.as_ref())?;
        }
        content
    } else if let Some(call) = fields.tool_call.take() {
        let g::ToolCall {
            tool_type,
            id,
            tool_name,
            args,
            rest,
        } = call;
        let name = match &tool_name {
            Some(name) => name.clone(),
            None => {
                serde_json::from_value(serde_json::to_value(&tool_type).map_err(Error::invalid)?)
                    .map_err(Error::invalid)?
            }
        };
        let arguments = match &args {
            Some(args) => serde_json::to_string(args).map_err(Error::invalid)?,
            None => "{}".into(),
        };
        let portable_id = id
            .clone()
            .unwrap_or_else(|| format!("server_gemini_{index}"));
        meta.tool_call = Some(g::ToolCallOptions {
            tool_type: Some(tool_type),
            id,
            tool_name: tool_name.map(|_| String::new()),
            args: args.map(|_| Rest::new()),
            rest,
        });
        Content::ToolCall(ToolCall {
            id: portable_id,
            name,
            arguments,
            provider_executed: true,
        })
    } else if let Some(result) = fields.tool_response.take() {
        let g::ToolResponse {
            tool_type,
            id,
            response,
            rest,
        } = result;
        let name =
            serde_json::from_value(serde_json::to_value(&tool_type).map_err(Error::invalid)?)
                .map_err(Error::invalid)?;
        let output = ToolOutput::Json(
            response
                .as_ref()
                .map(|r| Value::Object(r.clone()))
                .unwrap_or(Value::Null),
        );
        let portable_id = id.clone().unwrap_or_default();
        meta.tool_response = Some(g::ToolResponseOptions {
            tool_type: Some(tool_type),
            id,
            response: response.map(|_| Rest::new()),
            rest,
        });
        Content::ToolResult(ToolResult {
            id: portable_id,
            name,
            output,
            is_error: false,
            provider_executed: true,
        })
    } else {
        let mut original = g::Part::builder().build();
        fields.apply_to(&mut original);
        return with_native(Content::Opaque, Vendor::Gemini, original);
    };
    if let Content::Video(_) = &content
        && let Some(video) = fields.video_metadata.as_mut()
    {
        video.fps = None;
    }
    meta.fields = fields;
    with_native(content, Vendor::Gemini, meta)
}
fn parse_video(value: Option<&g::VideoMetadata>) -> Result<Option<VideoClip>> {
    value
        .map(|v| {
            Ok(VideoClip {
                start_seconds: v.start_offset.as_deref().map(parse_duration).transpose()?,
                end_seconds: v.end_offset.as_deref().map(parse_duration).transpose()?,
                fps: v.fps,
            })
        })
        .transpose()
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsePartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inline_data: Option<g::BlobOptions>,
    #[serde(flatten)]
    fields: g::FunctionResponsePartOptions,
}
fn parse_response_part(part: g::FunctionResponsePart) -> Result<ToolResultBlock> {
    let mut fields = g::FunctionResponsePartOptions::from(part);
    let (content, inline) = if let Some(blob) = fields.inline_data.take() {
        let (content, inline, _) = parse_blob(blob, Detail::Auto, None)?;
        (content, Some(inline))
    } else {
        return Err(Error::invalid("function-response media part has no source"));
    };
    let content = match content {
        Content::Image(v) => ToolResultContent::Image(v),
        Content::Document(v) => ToolResultContent::Document(v),
        _ => {
            return Err(Error::invalid(
                "tool output supports text, images and documents",
            ));
        }
    };
    #[derive(Serialize)]
    struct Envelope {
        function_result_part: ResponsePartMetadata,
    }
    with_metadata(
        content,
        Vendor::Gemini,
        Envelope {
            function_result_part: ResponsePartMetadata {
                inline_data: inline,
                fields,
            },
        },
    )
}
