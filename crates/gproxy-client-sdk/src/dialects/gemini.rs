use super::*;
use gproxy_protocol::gemini as g;

// Only native fields without a portable payload are persisted.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct MediaMetadata {
    mime_type: Option<String>,
    display_name: Option<String>,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct FunctionMetadata {
    id_present: bool,
    args_present: bool,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct ResultMetadata {
    id_present: bool,
    parts_present: bool,
    will_continue: Option<bool>,
    scheduling: Option<g::Scheduling>,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
struct ServerCallMetadata {
    tool_type: g::ToolType,
    id_present: bool,
    name_present: bool,
    args_present: bool,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
struct ServerResponseMetadata {
    tool_type: g::ToolType,
    id_present: bool,
    response_present: bool,
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AudioMetadata {
    speaker_label: Option<String>,
    words: Option<Vec<g::audio::WordInfo>>,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PartFields {
    thought: Option<bool>,
    thought_signature: Option<String>,
    part_metadata: Option<Rest>,
    media_resolution: Option<g::MediaResolution>,
    media_processing: Option<g::MediaProcessing>,
    speech_metadata: Option<g::audio::SpeechMetadata>,
    video_metadata: Option<g::VideoMetadata>,
    #[serde(flatten)]
    rest: Rest,
}
fn part_fields(part: g::Part) -> PartFields {
    PartFields {
        thought: part.thought,
        thought_signature: part.thought_signature,
        part_metadata: part.part_metadata,
        media_resolution: part.media_resolution,
        media_processing: part.media_processing,
        speech_metadata: part.speech_metadata,
        video_metadata: part.video_metadata,
        rest: part.rest,
    }
}
fn native_part(meta: PartFields) -> g::Part {
    let mut part = g::Part::builder().build();
    part.thought = meta.thought;
    part.thought_signature = meta.thought_signature;
    part.part_metadata = meta.part_metadata;
    part.media_resolution = meta.media_resolution;
    part.media_processing = meta.media_processing;
    part.speech_metadata = meta.speech_metadata;
    part.video_metadata = meta.video_metadata;
    part.rest = meta.rest;
    part
}
fn audio_metadata(value: g::audio::AudioTranscription) -> AudioMetadata {
    AudioMetadata {
        speaker_label: value.speaker_label,
        words: value.words,
        rest: value.rest,
    }
}
fn transcription(text: String, meta: AudioMetadata) -> g::audio::AudioTranscription {
    let mut value = g::audio::AudioTranscription::builder(text).build();
    value.speaker_label = meta.speaker_label;
    value.words = meta.words;
    value.rest = meta.rest;
    value
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PartMetadata {
    inline_data: Option<MediaMetadata>,
    file_data: Option<MediaMetadata>,
    function_call: Option<FunctionMetadata>,
    function_response: Option<ResultMetadata>,
    audio_transcription: Option<AudioMetadata>,
    response_json_text: bool,
    tool_call: Option<ServerCallMetadata>,
    tool_response: Option<ServerResponseMetadata>,
    trailing_transcripts: Vec<TranscriptMetadata>,
    primary_transcript_chars: Option<usize>,
    #[serde(flatten)]
    fields: PartFields,
}
#[derive(Serialize, Deserialize)]
struct TranscriptMetadata {
    fields: PartFields,
    transcription: AudioMetadata,
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
    let fields = part_fields(part.clone());
    let native = audio_metadata(transcript.clone());
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
        let mut part = native_part(meta.fields);
        part.audio_transcription = Some(transcription(text, meta.transcription));
        parts.push(part);
    }
    Ok(parts)
}
pub fn render(request: &GenerateRequest) -> Result<Rendered<g::GenerateContentRequestBody>> {
    crate::resolution::validate_request(request)?;
    let opts = &request.provider_options.gemini;
    let mut body = g::GenerateContentRequestBody::builder(Vec::new()).build();
    body.generation_config = opts.generation_config.clone();
    body.tools = opts.tools.clone();
    body.tool_config = opts.tool_config.clone();
    body.safety_settings = opts.safety_settings.clone();
    body.cached_content = opts.cached_content.clone();
    body.continuation_token = opts.continuation_token.clone();
    body.service_tier = opts.service_tier.clone();
    body.rest = opts.rest.clone();
    let mut contents = Vec::new();
    let mut system = Vec::new();
    let mut calls = BTreeMap::new();
    for message in &request.messages {
        if matches!(&message.role, Role::Unknown(_)) {
            continue;
        }
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
        if let Some(value) = request.settings.temperature
            && config.temperature.is_none()
        {
            config.temperature = Some(value);
        }
        if let Some(value) = request.settings.top_p
            && config.top_p.is_none()
        {
            config.top_p = Some(value);
        }
        if let Some(value) = request.settings.max_output_tokens
            && config.max_output_tokens.is_none()
        {
            config.max_output_tokens = Some(value.into());
        }
        if !request.settings.stop_sequences.is_empty() && config.stop_sequences.is_none() {
            config.stop_sequences = Some(request.settings.stop_sequences.clone());
        }
        if let Some(schema) = &request.settings.output_schema
            && config.response_json_schema.is_none()
            && config.response_schema.is_none()
            && config.response_mime_type.is_none()
        {
            config.response_mime_type = Some("application/json".into());
            config.response_json_schema = Some(schema.clone());
        }
    }
    if !request.settings.output_modalities.is_empty()
        && opts
            .generation_config
            .as_ref()
            .and_then(|c| c.response_modalities.as_ref())
            .is_none()
    {
        let modalities = request
            .settings
            .output_modalities
            .iter()
            .filter_map(|m| match m {
                Modality::Text => Some(g::Modality::Text),
                Modality::Audio => Some(g::Modality::Audio),
                Modality::Image => Some(g::Modality::Image),
                Modality::Unknown(_) => None,
            })
            .collect::<Vec<_>>();
        if !modalities.is_empty() {
            body.generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .response_modalities = Some(modalities);
        }
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
        let speech_config = config
            .speech_config
            .get_or_insert_with(|| g::SpeechConfig::builder().build());
        if speech_config.multi_speaker_voice_config.is_none() {
            let voice = speech_config
                .voice_config
                .get_or_insert_with(|| g::VoiceConfig::builder().build());
            let prebuilt = voice
                .prebuilt_voice_config
                .get_or_insert_with(|| g::PrebuiltVoiceConfig::builder().build());
            if prebuilt.voice_name.is_none() {
                prebuilt.voice_name = Some(speech.voice.clone());
            }
        }
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
        body.tools.get_or_insert_with(Vec::new).push(
            g::Tool::builder()
                .function_declarations(declarations)
                .build(),
        );
    }
    if let Some(choice) = &request.settings.tool_choice {
        let mode = match choice {
            ToolChoice::Auto => Some(g::FunctionCallingMode::Auto),
            ToolChoice::None => Some(g::FunctionCallingMode::None),
            ToolChoice::Required | ToolChoice::Tool { .. } => Some(g::FunctionCallingMode::Any),
            ToolChoice::Unknown(_) => None,
        };
        if let Some(mode) = mode {
            let config = body
                .tool_config
                .get_or_insert_with(|| g::ToolConfig::builder().build())
                .function_calling_config
                .get_or_insert_with(|| g::FunctionCallingConfig::builder().build());
            config.mode = Some(mode);
            if let ToolChoice::Tool { name } = choice {
                config.allowed_function_names = Some(vec![name.clone()]);
            }
        }
    }
    let mut warnings = crate::warnings::for_request(request, None)?;
    warnings.extend(crate::reasoning::gemini(request, &mut body));
    Ok(Rendered {
        request: body,
        warnings,
    })
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
    let mut part = native_part(meta.fields);
    match &block.content {
        Content::Text(text) => part.text = Some(text.text.clone()),
        Content::Reasoning(Reasoning { text }) => {
            part.text = Some(text.clone());
            part.thought = Some(true);
        }
        Content::Image(image) => {
            media::check_file_vendor(&image.source, Vendor::Gemini)?;
            let source = &image.source;
            render_asset(
                &mut part,
                source,
                image.media_type.as_deref(),
                None,
                meta.inline_data,
                meta.file_data,
            )?;
            set_detail(&mut part, &image.detail);
        }
        Content::Document(document) => {
            let (asset, mime) = media::document_bytes(document)?;
            media::check_file_vendor(&asset, Vendor::Gemini)?;
            let source = asset.as_ref();
            render_asset(
                &mut part,
                source,
                mime,
                document.title.as_deref(),
                meta.inline_data,
                meta.file_data,
            )?;
            set_detail(&mut part, &document.detail);
        }
        Content::File(file) => {
            media::check_file_vendor(&file.source, Vendor::Gemini)?;
            let source = &file.source;
            render_asset(
                &mut part,
                source,
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
                && media::parse_audio_format(original)? == audio.format
            {
                mime = original.into();
            }
            media::check_file_vendor(&audio.source, Vendor::Gemini)?;
            let source = &audio.source;
            render_asset(
                &mut part,
                source,
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
                    part.audio_transcription = Some(transcription(
                        text,
                        meta.audio_transcription.unwrap_or_default(),
                    ));
                }
            }
        }
        Content::Video(video) => {
            media::check_file_vendor(&video.source, Vendor::Gemini)?;
            let source = &video.source;
            render_asset(
                &mut part,
                source,
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
                let Some(native) = meta.tool_call else {
                    return Ok(None);
                };
                let mut value = g::ToolCall::builder(native.tool_type)
                    .rest(native.rest)
                    .build();
                if native.id_present {
                    value.id = Some(call.id.clone());
                }
                if native.name_present {
                    value.tool_name = Some(call.name.clone());
                }
                if native.args_present || call.arguments != "{}" {
                    value.args =
                        Some(serde_json::from_str(&call.arguments).map_err(Error::invalid)?);
                }
                part.tool_call = Some(value);
                return Ok(Some(part));
            }
            let include_args = meta.function_call.as_ref().is_some_and(|m| m.args_present)
                || !had_metadata
                || call.arguments != "{}";
            let include_id =
                meta.function_call.as_ref().is_some_and(|m| m.id_present) || !had_metadata;
            let mut value = g::FunctionCall::builder(call.name.clone()).build();
            if let Some(native) = meta.function_call {
                value.rest = native.rest;
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
                let Some(native) = meta.tool_response else {
                    return Ok(None);
                };
                let mut value = g::ToolResponse::builder(native.tool_type)
                    .rest(native.rest)
                    .build();
                if native.id_present {
                    value.id = Some(result.id.clone());
                }
                let ToolOutput::Json(output) = &result.output else {
                    return Err(Error::invalid("Gemini server tool output requires JSON"));
                };
                if native.response_present || !output.is_null() {
                    value.response =
                        Some(serde_json::from_value(output.clone()).map_err(Error::invalid)?);
                }
                part.tool_response = Some(value);
                return Ok(Some(part));
            }
            let name = tool_name(&result.name, &result.id, calls)?;
            let mut parts = Vec::new();
            let response = match &result.output {
                ToolOutput::Unknown(_) => {
                    return Err(Error::invalid("unknown persisted tool output"));
                }
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
                            ToolResultContent::Unknown(_) => {
                                return Err(Error::invalid("unknown persisted tool content"));
                            }
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
            let include_id = meta
                .function_response
                .as_ref()
                .is_some_and(|m| m.id_present)
                || !had_metadata;
            let parts_present = meta
                .function_response
                .as_ref()
                .is_some_and(|m| m.parts_present);
            if let Some(native) = meta.function_response {
                value.will_continue = native.will_continue;
                value.scheduling = native.scheduling;
                value.rest = native.rest;
            }
            if parts_present || !parts.is_empty() {
                value.parts = Some(parts);
            }
            if include_id && !result.id.is_empty() {
                value.id = Some(result.id.clone());
            }
            part.function_response = Some(value);
        }
        Content::Unknown(_) => return Ok(None),
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
        Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
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
        part.rest = meta.fields.rest;
        if let (Some(native), Some(blob)) = (meta.inline_data, part.inline_data.as_mut()) {
            blob.rest = native.rest;
            if native.display_name.is_some() {
                blob.display_name = native.display_name;
            }
        }
    }
    Ok(part)
}
fn set_detail(part: &mut g::Part, detail: &Detail) {
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
    inline: Option<MediaMetadata>,
    file: Option<MediaMetadata>,
) -> Result<()> {
    match source {
        Asset::Inline { bytes } => {
            let mut value = g::Blob::builder(
                media::mime(mime, source)?.into(),
                base64::engine::general_purpose::STANDARD.encode(bytes),
            )
            .build();
            if let Some(native) = inline {
                value.rest = native.rest;
                value.display_name = native.display_name;
            }
            if let Some(name) = filename {
                value.display_name = Some(name.into());
            }
            part.inline_data = Some(value);
            part.file_data = None;
        }
        Asset::Url { url } | Asset::ProviderFile { id: url, .. } => {
            let mut value = g::FileData::builder(url.clone()).build();
            if let Some(native) = file {
                value.rest = native.rest;
                value.display_name = native.display_name;
                value.mime_type = native.mime_type;
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
        Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
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
    let metadata = response_metadata(&response);
    let warnings = if response.candidates.as_ref().is_some_and(|v| v.len() > 1) {
        vec![Warning::new(
            WarningKind::Compatibility,
            "response.candidates",
            "Only the first candidate is returned",
        )]
    } else {
        Vec::new()
    };
    check_response(&response)?;
    let usage = parse_usage(response.usage_metadata.as_ref())?;
    let candidate = response.candidates.into_iter().flatten().next();
    let reason = candidate.as_ref().and_then(|c| c.finish_reason);
    let sources = candidate
        .as_ref()
        .map(crate::sources::gemini_candidate)
        .transpose()?
        .unwrap_or_default();
    let sources = crate::sources::unique(sources);
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
        response: metadata,
        warnings,
        sources,
        content,
        finish_reason: finish_reason(reason, has_tools),
        usage,
    })
}

pub(crate) fn check_response(response: &g::GenerateContentResponseBody) -> Result<()> {
    if response.error.is_some() {
        return Err(Error::from_event(
            serde_json::to_string(response).map_err(Error::invalid)?,
        ));
    }
    if response
        .prompt_feedback
        .as_ref()
        .and_then(|p| p.block_reason)
        .is_some_and(|r| r != g::BlockReason::Unspecified)
    {
        return Err(Error::content_blocked(
            serde_json::to_string(response).map_err(Error::invalid)?,
        ));
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
    let Some(reason) = reason else {
        return FinishReason::new(FinishReasonKind::Other, None);
    };
    let (mut kind, raw) = match reason {
        g::FinishReason::Stop => (FinishReasonKind::Stop, "STOP"),
        g::FinishReason::MaxTokens => (FinishReasonKind::Length, "MAX_TOKENS"),
        g::FinishReason::Safety => (FinishReasonKind::ContentBlocked, "SAFETY"),
        g::FinishReason::Blocklist => (FinishReasonKind::ContentBlocked, "BLOCKLIST"),
        g::FinishReason::ProhibitedContent => {
            (FinishReasonKind::ContentBlocked, "PROHIBITED_CONTENT")
        }
        g::FinishReason::Spii => (FinishReasonKind::ContentBlocked, "SPII"),
        g::FinishReason::ImageSafety => (FinishReasonKind::ContentBlocked, "IMAGE_SAFETY"),
        g::FinishReason::Unspecified => (FinishReasonKind::Other, "FINISH_REASON_UNSPECIFIED"),
        g::FinishReason::Continuation => (FinishReasonKind::Other, "CONTINUATION"),
        g::FinishReason::Recitation => (FinishReasonKind::Other, "RECITATION"),
        g::FinishReason::Language => (FinishReasonKind::Other, "LANGUAGE"),
        g::FinishReason::Other => (FinishReasonKind::Other, "OTHER"),
        g::FinishReason::MalformedFunctionCall => {
            (FinishReasonKind::Other, "MALFORMED_FUNCTION_CALL")
        }
        g::FinishReason::ImageProhibitedContent => {
            (FinishReasonKind::ContentBlocked, "IMAGE_PROHIBITED_CONTENT")
        }
        g::FinishReason::ImageOther => (FinishReasonKind::Other, "IMAGE_OTHER"),
        g::FinishReason::NoImage => (FinishReasonKind::Other, "NO_IMAGE"),
        g::FinishReason::ImageRecitation => (FinishReasonKind::Other, "IMAGE_RECITATION"),
        g::FinishReason::UnexpectedToolCall => (FinishReasonKind::Other, "UNEXPECTED_TOOL_CALL"),
        g::FinishReason::TooManyToolCalls => (FinishReasonKind::Other, "TOO_MANY_TOOL_CALLS"),
        g::FinishReason::MissingThoughtSignature => {
            (FinishReasonKind::Other, "MISSING_THOUGHT_SIGNATURE")
        }
        g::FinishReason::MalformedResponse => (FinishReasonKind::Other, "MALFORMED_RESPONSE"),
        g::FinishReason::Escalation => (FinishReasonKind::Other, "ESCALATION"),
        g::FinishReason::PupLimitedDisabled => (FinishReasonKind::Other, "PUP_LIMITED_DISABLED"),
    };
    if kind == FinishReasonKind::Stop && has_tools {
        kind = FinishReasonKind::ToolCalls;
    }
    FinishReason::new(kind, Some(raw.into()))
}
pub(crate) fn response_metadata(response: &g::GenerateContentResponseBody) -> ResponseMetadata {
    ResponseMetadata {
        id: response.response_id.clone(),
        model: response.model_version.clone(),
        timestamp: None,
    }
}

fn parse_blob(
    blob: g::Blob,
    detail: Detail,
    transcription: Option<g::audio::AudioTranscription>,
) -> Result<(Content, MediaMetadata, Option<AudioMetadata>)> {
    let g::Blob {
        mime_type,
        data,
        display_name,
        rest,
    } = blob;
    let source = decode_inline(&data)?;
    let base = mime_type.split(';').next().unwrap_or(&mime_type);
    let mut native = MediaMetadata {
        mime_type: None,
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
            audio_meta = Some(audio_metadata(value));
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
fn parse_file(file: g::FileData, detail: Detail) -> Result<(Content, MediaMetadata)> {
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
    let mut native = MediaMetadata {
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
    let mut fields = part;
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
                refusal: false,
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
        meta.function_call = Some(FunctionMetadata {
            id_present: id.is_some(),
            args_present: args.is_some(),
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
        let parts_present = parts.is_some();
        let output = if let Some(parts) = parts.filter(|p| !p.is_empty()) {
            let mut blocks = vec![ContentBlock::new(ToolResultContent::Text(Text {
                refusal: false,
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
        meta.function_response = Some(ResultMetadata {
            id_present: id.is_some(),
            parts_present,
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
        let (content, mut file) = parse_file(file, detail)?;
        let mut content = if fields.video_metadata.is_some() {
            match content {
                Content::File(value) => {
                    file.display_name = value.filename;
                    Content::Video(Video {
                        source: value.source,
                        media_type: value.media_type,
                        clip: None,
                    })
                }
                other => other,
            }
        } else {
            content
        };
        meta.file_data = Some(file);
        if let Content::Audio(audio) = &mut content
            && let Some(mut value) = fields.audio_transcription.take()
        {
            audio.transcript = Some(std::mem::take(&mut value.text));
            meta.audio_transcription = Some(audio_metadata(value));
        }
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
        meta.tool_call = Some(ServerCallMetadata {
            tool_type,
            id_present: id.is_some(),
            name_present: tool_name.is_some(),
            args_present: args.is_some(),
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
        meta.tool_response = Some(ServerResponseMetadata {
            tool_type,
            id_present: id.is_some(),
            response_present: response.is_some(),
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
        return with_native(Content::Opaque, Vendor::Gemini, fields);
    };
    if let Content::Video(_) = &content
        && let Some(video) = fields.video_metadata.as_mut()
    {
        video.fps = None;
    }
    meta.fields = part_fields(fields);
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
#[derive(Default, Serialize, Deserialize)]
struct ResponseFields {
    #[serde(default, flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsePartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inline_data: Option<MediaMetadata>,
    #[serde(flatten)]
    fields: ResponseFields,
}
fn parse_response_part(part: g::FunctionResponsePart) -> Result<ToolResultBlock> {
    let mut fields = part;
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
                fields: ResponseFields { rest: fields.rest },
            },
        },
    )
}
