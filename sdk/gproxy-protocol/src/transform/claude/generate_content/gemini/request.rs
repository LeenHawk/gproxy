use crate::claude::count_tokens::types::{
    BetaContentBlockParam, BetaMessageContent, BetaMessageRole,
};
use crate::claude::create_message::request::ClaudeCreateMessageRequest;
use crate::gemini::count_tokens::types::{
    GeminiBlob, GeminiContentRole, GeminiFileData, GeminiFunctionCall, GeminiPart,
};
use crate::gemini::generate_content::request::{
    GeminiGenerateContentRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::gemini::generate_content::types::{GeminiContent, GeminiGenerationConfig, HttpMethod};
use crate::transform::claude::generate_content::gemini::utils::{
    gemini_system_instruction_from_claude, gemini_thinking_config_from_claude,
    gemini_tool_config_from_claude, gemini_tools_from_claude,
};
use crate::transform::claude::generate_content::utils::{
    beta_message_content_to_text, claude_model_to_string,
};
use crate::transform::claude::model_list::gemini::utils::ensure_models_prefix;
use crate::transform::utils::TransformError;

impl TryFrom<ClaudeCreateMessageRequest> for GeminiGenerateContentRequest {
    type Error = TransformError;

    fn try_from(value: ClaudeCreateMessageRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let model = ensure_models_prefix(&claude_model_to_string(&body.model));

        let contents = body
            .messages
            .into_iter()
            .map(|message| {
                let fallback_text = beta_message_content_to_text(&message.content);
                let parts = match message.content {
                    BetaMessageContent::Text(text) => vec![GeminiPart {
                        text: Some(text),
                        ..GeminiPart::default()
                    }],
                    BetaMessageContent::Blocks(blocks) => {
                        let mut parts = Vec::new();
                        for block in blocks {
                            match block {
                                BetaContentBlockParam::Text(block) => {
                                    parts.push(GeminiPart {
                                        text: Some(block.text),
                                        ..GeminiPart::default()
                                    });
                                }
                                BetaContentBlockParam::Thinking(block) => {
                                    parts.push(GeminiPart {
                                        thought: Some(true),
                                        thought_signature: Some(block.signature),
                                        text: Some(block.thinking),
                                        ..GeminiPart::default()
                                    });
                                }
                                BetaContentBlockParam::ToolUse(block) => {
                                    parts.push(GeminiPart {
                                        function_call: Some(GeminiFunctionCall {
                                            id: Some(block.id),
                                            name: block.name,
                                            args: Some(block.input),
                                        }),
                                        ..GeminiPart::default()
                                    });
                                }
                                BetaContentBlockParam::Image(block) => match block.source {
                                    crate::claude::count_tokens::types::BetaImageSource::Base64(
                                        source,
                                    ) => {
                                        let mime_type = match source.media_type {
                                            crate::claude::count_tokens::types::BetaImageMediaType::ImageJpeg => "image/jpeg",
                                            crate::claude::count_tokens::types::BetaImageMediaType::ImagePng => "image/png",
                                            crate::claude::count_tokens::types::BetaImageMediaType::ImageGif => "image/gif",
                                            crate::claude::count_tokens::types::BetaImageMediaType::ImageWebp => "image/webp",
                                        };
                                        parts.push(GeminiPart {
                                            inline_data: Some(GeminiBlob {
                                                mime_type: mime_type.to_string(),
                                                data: source.data,
                                            }),
                                            ..GeminiPart::default()
                                        });
                                    }
                                    crate::claude::count_tokens::types::BetaImageSource::Url(
                                        source,
                                    ) => {
                                        parts.push(GeminiPart {
                                            file_data: Some(GeminiFileData {
                                                mime_type: None,
                                                file_uri: source.url,
                                            }),
                                            ..GeminiPart::default()
                                        });
                                    }
                                    crate::claude::count_tokens::types::BetaImageSource::File(
                                        source,
                                    ) => {
                                        parts.push(GeminiPart {
                                            text: Some(format!("file_id:{}", source.file_id)),
                                            ..GeminiPart::default()
                                        });
                                    }
                                },
                                _ => {}
                            }
                        }

                        if parts.is_empty() {
                            vec![GeminiPart {
                                text: Some(fallback_text),
                                ..GeminiPart::default()
                            }]
                        } else {
                            parts
                        }
                    }
                };

                GeminiContent {
                    parts,
                    role: Some(match message.role {
                        BetaMessageRole::User => GeminiContentRole::User,
                        BetaMessageRole::Assistant => GeminiContentRole::Model,
                    }),
                }
            })
            .collect::<Vec<_>>();
        let system_instruction = gemini_system_instruction_from_claude(body.system);
        let tools = gemini_tools_from_claude(body.tools, true);
        let tool_config = gemini_tool_config_from_claude(body.tool_choice);

        let mut generation_config = GeminiGenerationConfig::default();
        let mut has_generation_config = true;
        generation_config.max_output_tokens = Some(body.max_tokens.min(u32::MAX as u64) as u32);
        if let Some(stop_sequences) = body.stop_sequences {
            generation_config.stop_sequences = Some(stop_sequences);
            has_generation_config = true;
        }
        if let Some(temperature) = body.temperature {
            generation_config.temperature = Some(temperature);
            has_generation_config = true;
        }
        if let Some(top_p) = body.top_p {
            generation_config.top_p = Some(top_p);
            has_generation_config = true;
        }
        if let Some(top_k) = body.top_k {
            generation_config.top_k = Some(top_k.min(u32::MAX as u64) as u32);
            has_generation_config = true;
        }
        let thinking_config = gemini_thinking_config_from_claude(
            body.thinking,
            body.output_config
                .as_ref()
                .and_then(|config| config.effort.as_ref()),
        );
        if let Some(thinking_config) = thinking_config {
            generation_config.thinking_config = Some(thinking_config);
            has_generation_config = true;
        }
        let json_output_requested = body
            .output_config
            .as_ref()
            .and_then(|config| config.format.as_ref())
            .is_some();
        let response_json_schema = body
            .output_config
            .as_ref()
            .and_then(|config| config.format.as_ref())
            .and_then(|schema| serde_json::to_value(schema.schema.clone()).ok());
        if json_output_requested {
            generation_config.response_mime_type = Some("application/json".to_string());
            has_generation_config = true;
        }
        if let Some(schema) = response_json_schema {
            generation_config.response_json_schema = Some(schema);
            has_generation_config = true;
        }
        let generation_config = if has_generation_config {
            Some(generation_config)
        } else {
            None
        };

        Ok(Self {
            method: HttpMethod::Post,
            path: PathParameters { model },
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                contents,
                tools,
                tool_config,
                safety_settings: None,
                system_instruction,
                generation_config,
                cached_content: None,
                store: None,
            },
        })
    }
}
