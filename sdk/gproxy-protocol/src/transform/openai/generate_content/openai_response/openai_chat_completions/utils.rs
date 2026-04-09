use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::types as ct;
use crate::openai::create_response::types::ResponseServiceTier;

pub fn custom_call_output_to_text(content: &ot::ResponseCustomToolCallOutputContent) -> String {
    match content {
        ot::ResponseCustomToolCallOutputContent::Text(text) => text.clone(),
        ot::ResponseCustomToolCallOutputContent::Content(parts) => parts
            .iter()
            .map(|part| match part {
                ot::ResponseInputContent::Text(part) => part.text.clone(),
                ot::ResponseInputContent::Image(part) => part
                    .image_url
                    .clone()
                    .or(part.file_id.clone())
                    .unwrap_or_else(|| "[input_image]".to_string()),
                ot::ResponseInputContent::File(part) => part
                    .file_url
                    .clone()
                    .or(part.file_id.clone())
                    .or(part.filename.clone())
                    .or(part.file_data.clone())
                    .unwrap_or_else(|| "[input_file]".to_string()),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn input_content_to_chat_part(
    content: ot::ResponseInputContent,
) -> Option<ct::ChatCompletionContentPart> {
    match content {
        ot::ResponseInputContent::Text(part) => Some(ct::ChatCompletionContentPart::Text(
            ct::ChatCompletionContentPartText {
                text: part.text,
                type_: ct::ChatCompletionContentPartTextType::Text,
            },
        )),
        ot::ResponseInputContent::Image(part) => {
            let url = part
                .image_url
                .or_else(|| part.file_id.map(|id| format!("file:{id}")))?;
            Some(ct::ChatCompletionContentPart::Image(
                ct::ChatCompletionContentPartImage {
                    image_url: ct::ChatCompletionImageUrl { url, detail: None },
                    type_: ct::ChatCompletionContentPartImageType::ImageUrl,
                },
            ))
        }
        ot::ResponseInputContent::File(part) => Some(ct::ChatCompletionContentPart::File(
            ct::ChatCompletionContentPartFile {
                file: ct::ChatCompletionFileInput {
                    file_data: part.file_data,
                    file_id: part.file_id,
                    file_url: part.file_url,
                    filename: part.filename,
                },
                type_: ct::ChatCompletionContentPartFileType::File,
            },
        )),
    }
}

pub fn message_content_to_user_content(
    content: ot::ResponseInputMessageContent,
) -> ct::ChatCompletionUserContent {
    match content {
        ot::ResponseInputMessageContent::Text(text) => ct::ChatCompletionUserContent::Text(text),
        ot::ResponseInputMessageContent::List(parts) => {
            let mapped = parts
                .into_iter()
                .filter_map(input_content_to_chat_part)
                .collect::<Vec<_>>();
            if mapped.is_empty() {
                ct::ChatCompletionUserContent::Text(String::new())
            } else {
                ct::ChatCompletionUserContent::Parts(mapped)
            }
        }
    }
}

pub fn response_reasoning_to_chat_reasoning(
    reasoning: Option<ot::ResponseReasoning>,
) -> Option<ct::ChatCompletionReasoningEffort> {
    let effort = reasoning.and_then(|value| value.effort)?;
    Some(match effort {
        ot::ResponseReasoningEffort::None => ct::ChatCompletionReasoningEffort::None,
        ot::ResponseReasoningEffort::Minimal => ct::ChatCompletionReasoningEffort::Minimal,
        ot::ResponseReasoningEffort::Low => ct::ChatCompletionReasoningEffort::Low,
        ot::ResponseReasoningEffort::Medium => ct::ChatCompletionReasoningEffort::Medium,
        ot::ResponseReasoningEffort::High => ct::ChatCompletionReasoningEffort::High,
        ot::ResponseReasoningEffort::XHigh => ct::ChatCompletionReasoningEffort::XHigh,
    })
}

pub fn response_text_to_chat_response_format(
    text: Option<&ot::ResponseTextConfig>,
) -> Option<ct::ChatCompletionResponseFormat> {
    let format = text.and_then(|value| value.format.as_ref())?;
    Some(match format {
        ot::ResponseTextFormatConfig::Text(_) => {
            ct::ChatCompletionResponseFormat::Text(ct::ChatCompletionResponseFormatText {
                type_: ct::ChatCompletionResponseFormatTextType::Text,
            })
        }
        ot::ResponseTextFormatConfig::JsonObject(_) => {
            ct::ChatCompletionResponseFormat::JsonObject(
                ct::ChatCompletionResponseFormatJsonObject {
                    type_: ct::ChatCompletionResponseFormatJsonObjectType::JsonObject,
                },
            )
        }
        ot::ResponseTextFormatConfig::JsonSchema(schema) => {
            ct::ChatCompletionResponseFormat::JsonSchema(
                ct::ChatCompletionResponseFormatJsonSchema {
                    json_schema: ct::ChatCompletionResponseFormatJsonSchemaConfig {
                        name: schema.name.clone(),
                        description: schema.description.clone(),
                        schema: Some(schema.schema.clone()),
                        strict: schema.strict,
                    },
                    type_: ct::ChatCompletionResponseFormatJsonSchemaType::JsonSchema,
                },
            )
        }
    })
}

pub fn response_text_to_chat_verbosity(
    text: Option<&ot::ResponseTextConfig>,
) -> Option<ct::ChatCompletionVerbosity> {
    text.and_then(|value| value.verbosity.as_ref())
        .map(|verbosity| match verbosity {
            ot::ResponseTextVerbosity::Low => ct::ChatCompletionVerbosity::Low,
            ot::ResponseTextVerbosity::Medium => ct::ChatCompletionVerbosity::Medium,
            ot::ResponseTextVerbosity::High => ct::ChatCompletionVerbosity::High,
        })
}

pub fn response_tool_choice_to_chat_tool_choice(
    tool_choice: Option<ot::ResponseToolChoice>,
) -> Option<ct::ChatCompletionToolChoiceOption> {
    match tool_choice {
        Some(ot::ResponseToolChoice::Options(option)) => {
            Some(ct::ChatCompletionToolChoiceOption::Mode(match option {
                ot::ResponseToolChoiceOptions::None => ct::ChatCompletionToolChoiceMode::None,
                ot::ResponseToolChoiceOptions::Auto => ct::ChatCompletionToolChoiceMode::Auto,
                ot::ResponseToolChoiceOptions::Required => {
                    ct::ChatCompletionToolChoiceMode::Required
                }
            }))
        }
        Some(ot::ResponseToolChoice::Function(tool)) => Some(
            ct::ChatCompletionToolChoiceOption::NamedFunction(ct::ChatCompletionNamedToolChoice {
                function: ct::ChatCompletionNamedFunction { name: tool.name },
                type_: ct::ChatCompletionNamedToolChoiceType::Function,
            }),
        ),
        Some(ot::ResponseToolChoice::Custom(tool)) => {
            Some(ct::ChatCompletionToolChoiceOption::NamedCustom(
                ct::ChatCompletionNamedToolChoiceCustom {
                    custom: ct::ChatCompletionNamedCustomTool { name: tool.name },
                    type_: ct::ChatCompletionNamedToolChoiceCustomType::Custom,
                },
            ))
        }
        Some(ot::ResponseToolChoice::Mcp(tool)) => tool.name.map(|name| {
            ct::ChatCompletionToolChoiceOption::NamedCustom(
                ct::ChatCompletionNamedToolChoiceCustom {
                    custom: ct::ChatCompletionNamedCustomTool { name },
                    type_: ct::ChatCompletionNamedToolChoiceCustomType::Custom,
                },
            )
        }),
        Some(ot::ResponseToolChoice::Allowed(allowed)) => Some(
            ct::ChatCompletionToolChoiceOption::Allowed(ct::ChatCompletionAllowedToolChoice {
                allowed_tools: ct::ChatCompletionAllowedTools {
                    mode: match allowed.mode {
                        ot::ResponseToolChoiceAllowedMode::Auto => {
                            ct::ChatCompletionAllowedToolsMode::Auto
                        }
                        ot::ResponseToolChoiceAllowedMode::Required => {
                            ct::ChatCompletionAllowedToolsMode::Required
                        }
                    },
                    tools: allowed.tools,
                },
                type_: ct::ChatCompletionAllowedToolChoiceType::AllowedTools,
            }),
        ),
        Some(ot::ResponseToolChoice::Types(choice)) => {
            Some(ct::ChatCompletionToolChoiceOption::NamedCustom(
                ct::ChatCompletionNamedToolChoiceCustom {
                    custom: ct::ChatCompletionNamedCustomTool {
                        name: match choice.type_ {
                            ot::ResponseToolChoiceBuiltinType::FileSearch => "file_search",
                            ot::ResponseToolChoiceBuiltinType::WebSearchPreview
                            | ot::ResponseToolChoiceBuiltinType::WebSearchPreview20250311 => {
                                "web_search_preview"
                            }
                            ot::ResponseToolChoiceBuiltinType::Computer
                            | ot::ResponseToolChoiceBuiltinType::ComputerUsePreview
                            | ot::ResponseToolChoiceBuiltinType::ComputerUse => {
                                "computer_use_preview"
                            }
                            ot::ResponseToolChoiceBuiltinType::ImageGeneration => {
                                "image_generation"
                            }
                            ot::ResponseToolChoiceBuiltinType::CodeInterpreter => {
                                "code_interpreter"
                            }
                        }
                        .to_string(),
                    },
                    type_: ct::ChatCompletionNamedToolChoiceCustomType::Custom,
                },
            ))
        }
        Some(ot::ResponseToolChoice::ApplyPatch(_)) => {
            Some(ct::ChatCompletionToolChoiceOption::NamedCustom(
                ct::ChatCompletionNamedToolChoiceCustom {
                    custom: ct::ChatCompletionNamedCustomTool {
                        name: "apply_patch".to_string(),
                    },
                    type_: ct::ChatCompletionNamedToolChoiceCustomType::Custom,
                },
            ))
        }
        Some(ot::ResponseToolChoice::Shell(_)) => {
            Some(ct::ChatCompletionToolChoiceOption::NamedCustom(
                ct::ChatCompletionNamedToolChoiceCustom {
                    custom: ct::ChatCompletionNamedCustomTool {
                        name: "shell".to_string(),
                    },
                    type_: ct::ChatCompletionNamedToolChoiceCustomType::Custom,
                },
            ))
        }
        None => None,
    }
}

fn response_custom_tool_format_to_chat(
    format: Option<ot::ResponseCustomToolInputFormat>,
) -> Option<ct::ChatCompletionCustomToolFormat> {
    match format {
        Some(ot::ResponseCustomToolInputFormat::Text(_)) => Some(
            ct::ChatCompletionCustomToolFormat::Text(ct::ChatCompletionCustomToolTextFormat {
                type_: ct::ChatCompletionCustomToolTextFormatType::Text,
            }),
        ),
        Some(ot::ResponseCustomToolInputFormat::Grammar(grammar)) => {
            Some(ct::ChatCompletionCustomToolFormat::Grammar(
                ct::ChatCompletionCustomToolGrammarFormat {
                    grammar: ct::ChatCompletionCustomToolGrammar {
                        definition: grammar.definition,
                        syntax: match grammar.syntax {
                            ot::ResponseCustomToolGrammarSyntax::Lark => {
                                ct::ChatCompletionCustomToolGrammarSyntax::Lark
                            }
                            ot::ResponseCustomToolGrammarSyntax::Regex => {
                                ct::ChatCompletionCustomToolGrammarSyntax::Regex
                            }
                        },
                    },
                    type_: ct::ChatCompletionCustomToolGrammarFormatType::Grammar,
                },
            ))
        }
        None => None,
    }
}

pub fn response_service_tier_to_chat(
    service_tier: ResponseServiceTier,
) -> ct::ChatCompletionServiceTier {
    match service_tier {
        ResponseServiceTier::Auto => ct::ChatCompletionServiceTier::Auto,
        ResponseServiceTier::Default => ct::ChatCompletionServiceTier::Default,
        ResponseServiceTier::Flex => ct::ChatCompletionServiceTier::Flex,
        ResponseServiceTier::Scale => ct::ChatCompletionServiceTier::Scale,
        ResponseServiceTier::Priority => ct::ChatCompletionServiceTier::Priority,
    }
}

pub fn response_tools_to_chat_tools(
    tools: Option<Vec<ot::ResponseTool>>,
) -> Option<Vec<ct::ChatCompletionTool>> {
    let mut out = Vec::new();

    for tool in tools.unwrap_or_default() {
        match tool {
            ot::ResponseTool::Function(tool) => out.push(ct::ChatCompletionTool::Function(
                ct::ChatCompletionFunctionTool {
                    function: ct::ChatCompletionFunctionDefinition {
                        name: tool.name,
                        description: tool.description,
                        parameters: Some(tool.parameters),
                        strict: tool.strict,
                    },
                    type_: ct::ChatCompletionFunctionToolType::Function,
                },
            )),
            ot::ResponseTool::Custom(tool) => out.push(ct::ChatCompletionTool::Custom(
                ct::ChatCompletionCustomTool {
                    custom: ct::ChatCompletionCustomToolSpec {
                        name: tool.name,
                        description: tool.description,
                        format: response_custom_tool_format_to_chat(tool.format),
                    },
                    type_: ct::ChatCompletionCustomToolType::Custom,
                },
            )),
            _ => {}
        }
    }

    if out.is_empty() { None } else { Some(out) }
}
