use super::*;
impl Walk {
    pub(super) fn block(&mut self, block: &Block, role: &Role, path: &str) -> Result<()> {
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
