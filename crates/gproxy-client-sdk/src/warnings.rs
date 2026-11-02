//! Diagnostics describe concrete information omitted by native compilation.
use crate::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct NativePresence {
    native: Option<serde::de::IgnoredAny>,
    custom: Option<serde::de::IgnoredAny>,
    item: Option<serde::de::IgnoredAny>,
    chat: Option<serde::de::IgnoredAny>,
}
#[derive(Default)]
struct Presence {
    native: bool,
    custom: bool,
    item: bool,
    chat: bool,
}
fn presence(metadata: &ProviderMetadata, vendor: &Vendor) -> Result<Presence> {
    let Some(rest) = metadata.get(vendor) else {
        return Ok(Presence::default());
    };
    let decoder = serde::de::value::MapDeserializer::<_, serde_json::Error>::new(
        rest.iter().map(|(key, value)| (key.as_str(), value)),
    );
    let value = NativePresence::deserialize(decoder).map_err(Error::invalid)?;
    Ok(Presence {
        native: value.native.is_some(),
        custom: value.custom.is_some(),
        item: value.item.is_some(),
        chat: value.chat.is_some(),
    })
}
pub(crate) fn for_request(request: &GenerateRequest, now: Option<u64>) -> Result<Vec<Warning>> {
    #[cfg(not(feature = "chat"))]
    let _ = now;
    let mut walk = Walk {
        #[cfg(any(feature = "openai", feature = "chat"))]
        dialect: request.model.dialect,
        vendor: request.model.dialect.vendor(),
        #[cfg(feature = "chat")]
        now,
        warnings: Vec::new(),
    };
    let options = &request.provider_options;
    if walk.vendor != Vendor::Claude && options.claude != options::ClaudeOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.claude",
            "Claude options are not sent to this vendor",
        );
    }
    if walk.vendor != Vendor::Openai && options.openai != options::OpenaiOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.openai",
            "OpenAI options are not sent to this vendor",
        );
    }
    if walk.vendor != Vendor::Gemini && options.gemini != options::GeminiOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.gemini",
            "Gemini options are not sent to this vendor",
        );
    }
    if walk.is_chat() && options.openai.responses != options::ResponsesOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.openai.responses",
            "Responses options are not sent to Chat Completions",
        );
    }
    if walk.is_responses() && options.openai.chat != options::ChatOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.openai.chat",
            "Chat options are not sent to Responses",
        );
    }
    if walk.vendor == Vendor::Claude {
        if request.settings.temperature.is_some() || options.claude.rest.contains_key("temperature")
        {
            walk.warn(
                WarningKind::Deprecated,
                "settings.temperature",
                "Claude temperature is sent as a legacy compatibility extension",
            );
        }
        if request.settings.top_p.is_some() || options.claude.rest.contains_key("top_p") {
            walk.warn(
                WarningKind::Deprecated,
                "settings.top_p",
                "Claude top_p is sent as a legacy compatibility extension",
            );
        }
        if request.settings.speech.is_some() {
            walk.warn(
                WarningKind::Unsupported,
                "settings.speech",
                "Claude Messages has no speech-output settings",
            );
        }
    }
    if walk.vendor == Vendor::Claude {
        if request.settings.max_output_tokens.is_some() && options.claude.max_tokens.is_some() {
            walk.warn(
                WarningKind::Compatibility,
                "settings.max_output_tokens",
                "Claude max_tokens overrides the portable setting",
            );
        }
        if request.settings.output_schema.is_some()
            && options
                .claude
                .output_config
                .as_ref()
                .and_then(|c| c.format.as_ref())
                .is_some()
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.output_schema",
                "Claude output_config.format overrides the portable schema",
            );
        }
    }
    if walk.is_responses()
        && request.settings.output_schema.is_some()
        && options
            .openai
            .responses
            .text
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_some()
    {
        walk.warn(
            WarningKind::Compatibility,
            "settings.output_schema",
            "Responses text.format overrides the portable schema",
        );
    }
    if walk.is_chat() {
        if request.settings.speech.is_some()
            && options
                .openai
                .chat
                .audio
                .as_ref()
                .and_then(Option::as_ref)
                .is_some()
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.speech",
                "Chat audio options override portable speech settings",
            );
        }
        if request.settings.output_schema.is_some() && options.openai.chat.response_format.is_some()
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.output_schema",
                "Chat response_format overrides the portable schema",
            );
        }
    }
    if walk.vendor == Vendor::Gemini
        && let Some(config) = &options.gemini.generation_config
    {
        for (ignored, field) in [
            (
                request.settings.temperature.is_some() && config.temperature.is_some(),
                "settings.temperature",
            ),
            (
                request.settings.top_p.is_some() && config.top_p.is_some(),
                "settings.top_p",
            ),
            (
                request.settings.max_output_tokens.is_some() && config.max_output_tokens.is_some(),
                "settings.max_output_tokens",
            ),
            (
                !request.settings.stop_sequences.is_empty() && config.stop_sequences.is_some(),
                "settings.stop_sequences",
            ),
            (
                !request.settings.output_modalities.is_empty()
                    && config.response_modalities.is_some(),
                "settings.output_modalities",
            ),
        ] {
            if ignored {
                walk.warn(
                    WarningKind::Compatibility,
                    field,
                    "Gemini generationConfig overrides the portable setting",
                );
            }
        }
        if request.settings.output_schema.is_some()
            && (config.response_schema.is_some()
                || config.response_json_schema.is_some()
                || config
                    .response_mime_type
                    .as_deref()
                    .is_some_and(|mime| mime != "application/json"))
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.output_schema",
                "Gemini response format options override the portable schema",
            );
        }
        if request.settings.speech.is_some()
            && config.speech_config.as_ref().is_some_and(|speech| {
                speech.multi_speaker_voice_config.is_some()
                    || speech
                        .voice_config
                        .as_ref()
                        .and_then(|voice| voice.prebuilt_voice_config.as_ref())
                        .and_then(|voice| voice.voice_name.as_ref())
                        .is_some()
            })
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.speech.voice",
                "Gemini speechConfig overrides the portable voice",
            );
        }
    }
    if matches!(&request.settings.tool_choice, Some(ToolChoice::Unknown(_))) {
        walk.warn(
            WarningKind::Unsupported,
            "settings.tool_choice",
            "Unknown tool choice is omitted",
        );
    }
    for (index, modality) in request.settings.output_modalities.iter().enumerate() {
        if matches!(modality, Modality::Unknown(_)) {
            walk.warn(
                WarningKind::Unsupported,
                &format!("settings.output_modalities[{index}]"),
                "Unknown output modality is omitted",
            );
        }
    }
    for (index, message) in request.messages.iter().enumerate() {
        let path = format!("messages[{index}]");
        if matches!(message.role, Role::Unknown(_)) {
            walk.warn(
                WarningKind::Unsupported,
                &format!("{path}.role"),
                "Message with an unknown persisted role is omitted",
            );
            continue;
        }
        for (index, block) in message.content.iter().enumerate() {
            walk.block(block, &message.role, &format!("{path}.content[{index}]"))?;
        }
    }
    Ok(walk.warnings)
}
struct Walk {
    #[cfg(any(feature = "openai", feature = "chat"))]
    dialect: Dialect,
    vendor: Vendor,
    #[cfg(feature = "chat")]
    now: Option<u64>,
    warnings: Vec<Warning>,
}
impl Walk {
    fn warn(&mut self, kind: WarningKind, path: &str, message: &str) {
        self.warnings.push(Warning::new(kind, path, message));
    }
    fn is_chat(&self) -> bool {
        #[cfg(feature = "chat")]
        {
            self.dialect == Dialect::OpenaiChat
        }
        #[cfg(not(feature = "chat"))]
        {
            false
        }
    }
    fn is_responses(&self) -> bool {
        #[cfg(feature = "openai")]
        {
            self.dialect == Dialect::Openai
        }
        #[cfg(not(feature = "openai"))]
        {
            false
        }
    }
    fn metadata(&mut self, metadata: &ProviderMetadata, path: &str) -> Result<()> {
        for (vendor, rest) in metadata {
            if vendor != &self.vendor && !rest.is_empty() {
                self.warn(
                    WarningKind::Compatibility,
                    &format!("{path}.provider_metadata"),
                    &format!("Metadata for {vendor:?} is omitted when switching vendor"),
                );
            }
        }
        if self.vendor == Vendor::Openai {
            let flags = presence(metadata, &self.vendor)?;
            if self.is_chat() && (flags.native || flags.item) {
                self.warn(
                    WarningKind::Compatibility,
                    &format!("{path}.provider_metadata"),
                    "Native OpenAI metadata has no Chat continuation representation",
                );
            }
            if self.is_responses() && flags.chat {
                self.warn(
                    WarningKind::Compatibility,
                    &format!("{path}.provider_metadata"),
                    "Chat continuation metadata is omitted by Responses",
                );
            }
        }
        Ok(())
    }
    fn cache<T>(&mut self, block: &ContentBlock<T>, supported: bool, path: &str) {
        let Some(cache) = &block.cache else { return };
        if !supported {
            self.warn(
                WarningKind::Unsupported,
                &format!("{path}.cache"),
                "This native content kind has no cache breakpoint",
            );
            return;
        }
        let Some(ttl) = &cache.ttl else { return };
        let supported = match self.vendor {
            Vendor::Claude => matches!(ttl, CacheTtl::FiveMinutes | CacheTtl::OneHour),
            Vendor::Openai => matches!(ttl, CacheTtl::ThirtyMinutes | CacheTtl::TwelveHours),
            _ => false,
        };
        if !supported {
            self.warn(
                WarningKind::Unsupported,
                &format!("{path}.cache.ttl"),
                "The requested TTL is omitted; the native cache breakpoint uses its default TTL",
            );
        }
    }
    fn detail(&mut self, detail: &Detail, document: bool, path: &str) {
        let (kind, message) = if matches!(detail, Detail::Unknown(_)) {
            (
                WarningKind::Unsupported,
                "Unknown detail is omitted or uses native Auto where required",
            )
        } else if *detail == Detail::Auto {
            return;
        } else if self.vendor == Vendor::Claude {
            (
                WarningKind::Unsupported,
                "Claude has no per-block detail setting",
            )
        } else if self.is_responses()
            && document
            && matches!(detail, Detail::Medium | Detail::Original)
        {
            (
                WarningKind::Unsupported,
                "This document detail is not supported by Responses and is omitted",
            )
        } else if self.is_responses() && !document && *detail == Detail::Medium {
            (
                WarningKind::Compatibility,
                "Responses maps medium image detail to native Auto",
            )
        } else if self.is_chat()
            && (document || matches!(detail, Detail::Medium | Detail::Original))
        {
            (
                WarningKind::Unsupported,
                "This detail is not supported by Chat and is omitted",
            )
        } else {
            return;
        };
        self.warn(kind, &format!("{path}.detail"), message);
    }
    fn citations(&mut self, text: &Text, path: &str, responses_output: bool) -> Result<()> {
        for (index, citation) in text.citations.iter().enumerate() {
            let path = format!("{path}.citations[{index}]");
            self.metadata(&citation.provider_metadata, &path)?;
            let native = presence(&citation.provider_metadata, &self.vendor)?.native;
            let supported = if self.vendor == Vendor::Claude && native {
                match &citation.source {
                    CitationSource::Url { .. } => true,
                    CitationSource::Document {
                        document: DocumentReference::Index { vendor, .. },
                        range: Some(_),
                    } => *vendor == Vendor::Claude,
                    CitationSource::Document {
                        document: DocumentReference::Block { document },
                        range: Some(DocumentRange::Chunks { .. }),
                    } => matches!(
                        &document.content.source,
                        DocumentSource::Asset {
                            source: Asset::Url { .. }
                        }
                    ),
                    _ => false,
                }
            } else if self.is_responses() && responses_output {
                match &citation.source {
                    CitationSource::Url { .. } => {
                        citation.text_range.is_some() && citation.title.is_some()
                    }
                    CitationSource::Document { .. } => native,
                    CitationSource::Unknown(_) => false,
                }
            } else {
                false
            };
            if !supported {
                self.warn(WarningKind::Unsupported,&path,"Citation has no supported request representation for this dialect and is omitted");
            } else if self.vendor == Vendor::Claude && citation.text_range.is_some() {
                self.warn(
                    WarningKind::Unsupported,
                    &format!("{path}.text_range"),
                    "Claude citation parameters do not carry output-text offsets",
                );
            }
        }
        Ok(())
    }
    fn document(&mut self, document: &Document, path: &str) -> Result<()> {
        self.detail(&document.detail, true, path);
        if self.vendor != Vendor::Claude {
            if document.context.is_some() {
                self.warn(
                    WarningKind::Unsupported,
                    &format!("{path}.context"),
                    "Document context is omitted by this dialect",
                );
            }
            if document.citations.is_some() {
                self.warn(
                    WarningKind::Unsupported,
                    &format!("{path}.citations"),
                    "This dialect has no document citation-enable switch",
                );
            }
        }
        if matches!(&document.source, DocumentSource::Text { .. })
            && document
                .media_type
                .as_deref()
                .is_some_and(|mime| mime != "text/plain")
        {
            self.warn(
                WarningKind::Compatibility,
                &format!("{path}.media_type"),
                "Plain document text uses text/plain",
            );
        }
        if let DocumentSource::Chunks { chunks } = &document.source {
            for (index, chunk) in chunks.iter().enumerate() {
                let path = format!("{path}.source.chunks[{index}]");
                self.metadata(&chunk.provider_metadata, &path)?;
                self.cache(chunk, self.vendor == Vendor::Claude, &path);
                match &chunk.content {
                    ChunkContent::Text(text) => self.citations(text, &path, false)?,
                    ChunkContent::Image(image) => self.detail(&image.detail, false, &path),
                    ChunkContent::Unknown(_) => self.warn(
                        WarningKind::Unsupported,
                        &path,
                        "Unknown document chunk is not sent",
                    ),
                }
            }
        }
        Ok(())
    }
    fn block(&mut self, block: &Block, role: &Role, path: &str) -> Result<()> {
        self.metadata(&block.provider_metadata, path)?;
        let mut cache_supported = match &block.content {
            Content::Text(_) | Content::Image(_) | Content::Document(_) | Content::File(_) => {
                self.vendor != Vendor::Gemini
            }
            Content::Audio(_) => self.is_chat() && *role != Role::Assistant,
            Content::ToolCall(_) | Content::ToolResult(_) => self.vendor == Vendor::Claude,
            _ => false,
        };
        let envelope = presence(&block.provider_metadata, &self.vendor)?;
        let responses_text =
            self.is_responses() && *role == Role::Assistant && envelope.native && envelope.item;
        if let Content::Text(text) = &block.content
            && (responses_text || (self.is_chat() && text.refusal && *role == Role::Assistant))
        {
            cache_supported = false;
        }
        #[cfg(feature = "openai")]
        if self.is_responses()
            && matches!(&block.content, Content::Image(_))
            && crate::dialects::openai::is_output_image(block)?
        {
            cache_supported = false;
            if *role != Role::Assistant {
                self.warn(
                    WarningKind::Compatibility,
                    &format!("{path}.provider_metadata"),
                    "Generated-image item metadata is omitted for a non-assistant message",
                );
            }
        }
        self.cache(block, cache_supported, path);
        match &block.content {
            Content::Text(text) => {
                let refusal_native = (self.is_chat() && *role == Role::Assistant) || responses_text;
                if text.refusal && !refusal_native {
                    self.warn(
                        WarningKind::Compatibility,
                        &format!("{path}.refusal"),
                        "Refusal remains portable IR semantics; this native request sends its text without a refusal marker",
                    );
                }
                if self.is_responses()
                    && *role != Role::Assistant
                    && envelope.native
                    && envelope.item
                {
                    self.warn(
                        WarningKind::Compatibility,
                        &format!("{path}.provider_metadata"),
                        "Assistant message-envelope metadata is omitted for another role",
                    );
                }
                self.citations(text, path, responses_text && !text.refusal)?;
            }
            Content::Image(image) => self.detail(&image.detail, false, path),
            Content::Document(document) => self.document(document, path)?,
            Content::Reasoning(_)
                if !self.is_chat() && !presence(&block.provider_metadata, &self.vendor)?.native =>
            {
                self.warn(
                    WarningKind::Compatibility,
                    path,
                    "Reasoning without this vendor's native continuation metadata is omitted",
                )
            }
            Content::Opaque => {
                let native = presence(&block.provider_metadata, &self.vendor)?;
                if !(if self.is_chat() {
                    native.custom
                } else {
                    native.native
                }) {
                    self.warn(
                        WarningKind::Compatibility,
                        path,
                        "Opaque content for another dialect is omitted",
                    );
                }
            }
            Content::Unknown(_) => self.warn(
                WarningKind::Unsupported,
                path,
                "Unknown persisted content is omitted",
            ),
            Content::Audio(audio) => {
                #[cfg(feature = "chat")]
                if self.is_chat()
                    && *role == Role::Assistant
                    && let Some(now) = self.now
                    && !crate::dialects::chat::audio_reference_valid(block, now)?
                    && audio.transcript.is_some()
                {
                    self.warn(
                        WarningKind::Compatibility,
                        &format!("{path}.source"),
                        "Expired or foreign audio is replayed as transcript text",
                    );
                }
                if self.is_chat() && *role != Role::Assistant && audio.transcript.is_some() {
                    self.warn(
                        WarningKind::Unsupported,
                        &format!("{path}.transcript"),
                        "Chat input_audio does not carry a supplied transcript",
                    );
                }
            }
            Content::ToolCall(call)
                if call.provider_executed
                    && !presence(&block.provider_metadata, &self.vendor)?.native =>
            {
                self.warn(
                    WarningKind::Compatibility,
                    path,
                    "Provider-executed tool history without this vendor's metadata is omitted",
                )
            }
            Content::ToolResult(result) => {
                if result.provider_executed
                    && !presence(&block.provider_metadata, &self.vendor)?.native
                {
                    self.warn(
                        WarningKind::Compatibility,
                        path,
                        "Provider-executed tool result for another vendor is omitted",
                    );
                }
                if result.is_error && self.vendor == Vendor::Openai {
                    self.warn(
                        WarningKind::Unsupported,
                        &format!("{path}.is_error"),
                        "OpenAI tool outputs have no portable is_error field",
                    );
                }
                if result.is_error
                    && self.vendor == Vendor::Gemini
                    && match &result.output {
                        ToolOutput::Json(serde_json::Value::Object(map)) => {
                            !map.contains_key("error")
                        }
                        ToolOutput::Blocks(_) => true,
                        _ => false,
                    }
                {
                    self.warn(
                        WarningKind::Compatibility,
                        &format!("{path}.is_error"),
                        "Gemini uses its response payload to represent tool errors",
                    );
                }
                if let ToolOutput::Blocks(blocks) = &result.output {
                    for (index, block) in blocks.iter().enumerate() {
                        let path = format!("{path}.output[{index}]");
                        self.metadata(&block.provider_metadata, &path)?;
                        self.cache(block, self.vendor != Vendor::Gemini, &path);
                        match &block.content {
                            ToolResultContent::Text(text) => self.citations(text, &path, false)?,
                            ToolResultContent::Image(image) => {
                                self.detail(&image.detail, false, &path)
                            }
                            ToolResultContent::Document(document) => {
                                self.document(document, &path)?
                            }
                            ToolResultContent::Unknown(_) => self.warn(
                                WarningKind::Unsupported,
                                &path,
                                "Unknown tool content is not sent",
                            ),
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
