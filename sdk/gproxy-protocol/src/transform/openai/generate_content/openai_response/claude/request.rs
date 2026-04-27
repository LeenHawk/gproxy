use crate::claude::count_tokens::types as ct;
use crate::claude::create_message::request::{
    ClaudeCreateMessageRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::claude::create_message::types::{
    BetaMetadata, BetaServiceTierParam, BetaSpeed, HttpMethod as ClaudeHttpMethod, Model,
};
use crate::openai::count_tokens::types as ot;
use crate::openai::create_response::request::OpenAiCreateResponseRequest;
use crate::openai::create_response::types::{ResponseContextManagementType, ResponseServiceTier};
use crate::transform::openai::count_tokens::claude::utils::{
    ClaudeToolUseIdMapper, mcp_allowed_tools_to_configs, openai_mcp_tool_to_server,
    openai_message_content_to_claude, openai_reasoning_to_claude, openai_role_to_claude,
    openai_tool_choice_to_claude, parallel_disable, push_message_block,
    response_input_contents_to_tool_result_content, tool_from_function,
};
use crate::transform::openai::count_tokens::utils::{
    openai_input_to_items, openai_reasoning_summary_to_text,
};
use crate::transform::utils::TransformError;

fn parse_tool_use_input(input: String) -> ct::JsonObject {
    serde_json::from_str::<ct::JsonObject>(&input).unwrap_or_else(|_| {
        let mut object = ct::JsonObject::new();
        object.insert("input".to_string(), serde_json::Value::String(input));
        object
    })
}

fn push_block_message(
    messages: &mut Vec<ct::BetaMessageParam>,
    role: ct::BetaMessageRole,
    block: ct::BetaContentBlockParam,
) {
    push_message_block(messages, role, block);
}

fn web_search_tool_use_id(
    id: Option<String>,
    action: &ot::ResponseFunctionWebSearchAction,
    sequence: usize,
) -> String {
    id.unwrap_or_else(|| match action {
        ot::ResponseFunctionWebSearchAction::Search { .. } => format!("web_search_{sequence}"),
        ot::ResponseFunctionWebSearchAction::OpenPage { .. } => {
            format!("web_search_open_page_{sequence}")
        }
        ot::ResponseFunctionWebSearchAction::FindInPage { .. } => {
            format!("web_search_find_in_page_{sequence}")
        }
    })
}

impl TryFrom<OpenAiCreateResponseRequest> for ClaudeCreateMessageRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiCreateResponseRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let mut messages = Vec::new();
        let mut tool_use_ids = ClaudeToolUseIdMapper::default();

        for item in openai_input_to_items(body.input.clone()) {
            match item {
                ot::ResponseInputItem::Message(message) => {
                    messages.push(ct::BetaMessageParam {
                        content: openai_message_content_to_claude(message.content),
                        role: openai_role_to_claude(message.role),
                    });
                }
                ot::ResponseInputItem::OutputMessage(message) => {
                    let text = message
                        .content
                        .into_iter()
                        .map(|part| match part {
                            ot::ResponseOutputContent::Text(text) => text.text,
                            ot::ResponseOutputContent::Refusal(refusal) => refusal.refusal,
                        })
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Text(text),
                            role: ct::BetaMessageRole::Assistant,
                        });
                    }
                }
                ot::ResponseInputItem::FunctionToolCall(tool_call) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                            id: tool_use_id,
                            input: parse_tool_use_input(tool_call.arguments),
                            name: tool_call.name,
                            type_: ct::BetaToolUseBlockType::ToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::CustomToolCall(tool_call) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                            id: tool_use_id,
                            input: parse_tool_use_input(tool_call.input),
                            name: tool_call.name,
                            type_: ct::BetaToolUseBlockType::ToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::FunctionCallOutput(tool_result) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_result.call_id);
                    let content = match tool_result.output {
                        ot::ResponseFunctionCallOutputContent::Text(text) => (!text.is_empty())
                            .then_some(ct::BetaToolResultBlockParamContent::Text(text)),
                        ot::ResponseFunctionCallOutputContent::Content(parts) => {
                            response_input_contents_to_tool_result_content(parts)
                        }
                    };
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content,
                            is_error: None,
                        }),
                    );
                }
                ot::ResponseInputItem::CustomToolCallOutput(tool_result) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_result.call_id);
                    let content = match tool_result.output {
                        ot::ResponseCustomToolCallOutputContent::Text(text) => (!text.is_empty())
                            .then_some(ct::BetaToolResultBlockParamContent::Text(text)),
                        ot::ResponseCustomToolCallOutputContent::Content(parts) => {
                            response_input_contents_to_tool_result_content(parts)
                        }
                    };
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content,
                            is_error: None,
                        }),
                    );
                }
                ot::ResponseInputItem::McpCall(call) => {
                    let tool_use_id = call.id.clone();
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::McpToolUse(ct::BetaMcpToolUseBlockParam {
                            id: tool_use_id.clone(),
                            input: parse_tool_use_input(call.arguments),
                            name: call.name,
                            server_name: call.server_label,
                            type_: ct::BetaMcpToolUseBlockType::McpToolUse,
                            cache_control: None,
                        }),
                    );
                    if call.output.is_some() || call.error.is_some() {
                        let text = call.output.or(call.error).unwrap_or_default();
                        push_block_message(
                            &mut messages,
                            ct::BetaMessageRole::User,
                            ct::BetaContentBlockParam::McpToolResult(
                                ct::BetaRequestMcpToolResultBlockParam {
                                    tool_use_id,
                                    type_: ct::BetaRequestMcpToolResultBlockType::McpToolResult,
                                    cache_control: None,
                                    content: (!text.is_empty()).then_some(
                                        ct::BetaMcpToolResultBlockParamContent::Text(text),
                                    ),
                                    is_error: None,
                                },
                            ),
                        );
                    }
                }
                ot::ResponseInputItem::CodeInterpreterToolCall(call) => {
                    let mut input = ct::JsonObject::new();
                    input.insert("code".to_string(), serde_json::Value::String(call.code));
                    if !call.container_id.is_empty() {
                        input.insert(
                            "container_id".to_string(),
                            serde_json::Value::String(call.container_id),
                        );
                    }
                    let tool_use_id = tool_use_ids.server_tool_use_id(call.id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ServerToolUse(ct::BetaServerToolUseBlockParam {
                            id: tool_use_id.clone(),
                            input,
                            name: ct::BetaServerToolUseName::CodeExecution,
                            type_: ct::BetaServerToolUseBlockType::ServerToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                    if let Some(outputs) = call.outputs {
                        let mut stdout = Vec::new();
                        for output in outputs {
                            match output {
                                ot::ResponseCodeInterpreterOutputItem::Logs { logs } => {
                                    if !logs.is_empty() {
                                        stdout.push(logs);
                                    }
                                }
                                ot::ResponseCodeInterpreterOutputItem::Image { url } => {
                                    if !url.is_empty() {
                                        stdout.push(url);
                                    }
                                }
                            }
                        }
                        push_block_message(
                            &mut messages,
                            ct::BetaMessageRole::User,
                            ct::BetaContentBlockParam::CodeExecutionToolResult(
                                ct::BetaCodeExecutionToolResultBlockParam {
                                    content: ct::BetaCodeExecutionToolResultBlockParamContent::Result(
                                        ct::BetaCodeExecutionResultBlockParam {
                                            content: Vec::new(),
                                            return_code: 0,
                                            stderr: String::new(),
                                            stdout: stdout.join("\n"),
                                            type_: ct::BetaCodeExecutionResultBlockType::CodeExecutionResult,
                                        },
                                    ),
                                    tool_use_id,
                                    type_: ct::BetaCodeExecutionToolResultBlockType::CodeExecutionToolResult,
                                    cache_control: None,
                                },
                            ),
                        );
                    }
                }
                ot::ResponseInputItem::FunctionWebSearch(call) => {
                    let raw_tool_use_id =
                        web_search_tool_use_id(call.id.clone(), &call.action, messages.len());
                    match call.action {
                        ot::ResponseFunctionWebSearchAction::Search {
                            query,
                            queries,
                            sources,
                        } => {
                            let tool_use_id = tool_use_ids.server_tool_use_id(raw_tool_use_id);
                            let mut input = ct::JsonObject::new();
                            if let Some(query) = query.clone() {
                                input.insert("query".to_string(), serde_json::Value::String(query));
                            }
                            if let Some(queries) = queries.clone() {
                                input.insert(
                                    "queries".to_string(),
                                    serde_json::Value::Array(
                                        queries
                                            .into_iter()
                                            .map(serde_json::Value::String)
                                            .collect(),
                                    ),
                                );
                            }
                            push_block_message(
                                &mut messages,
                                ct::BetaMessageRole::Assistant,
                                ct::BetaContentBlockParam::ServerToolUse(
                                    ct::BetaServerToolUseBlockParam {
                                        id: tool_use_id.clone(),
                                        input,
                                        name: ct::BetaServerToolUseName::WebSearch,
                                        type_: ct::BetaServerToolUseBlockType::ServerToolUse,
                                        cache_control: None,
                                        caller: None,
                                    },
                                ),
                            );
                            if let Some(sources) = sources {
                                let text = sources
                                    .into_iter()
                                    .map(|source| source.url)
                                    .filter(|url| !url.is_empty())
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                push_block_message(
                                    &mut messages,
                                    ct::BetaMessageRole::User,
                                    ct::BetaContentBlockParam::ToolResult(
                                        ct::BetaToolResultBlockParam {
                                            tool_use_id,
                                            type_: ct::BetaToolResultBlockType::ToolResult,
                                            cache_control: None,
                                            content: (!text.is_empty()).then_some(
                                                ct::BetaToolResultBlockParamContent::Text(text),
                                            ),
                                            is_error: None,
                                        },
                                    ),
                                );
                            }
                        }
                        ot::ResponseFunctionWebSearchAction::OpenPage { url } => {
                            let tool_use_id = tool_use_ids.server_tool_use_id(raw_tool_use_id);
                            let mut input = ct::JsonObject::new();
                            if let Some(url) = url.clone() {
                                input.insert("url".to_string(), serde_json::Value::String(url));
                            }
                            push_block_message(
                                &mut messages,
                                ct::BetaMessageRole::Assistant,
                                ct::BetaContentBlockParam::ServerToolUse(
                                    ct::BetaServerToolUseBlockParam {
                                        id: tool_use_id.clone(),
                                        input,
                                        name: ct::BetaServerToolUseName::WebFetch,
                                        type_: ct::BetaServerToolUseBlockType::ServerToolUse,
                                        cache_control: None,
                                        caller: None,
                                    },
                                ),
                            );
                        }
                        ot::ResponseFunctionWebSearchAction::FindInPage { pattern, url } => {
                            let tool_use_id = tool_use_ids.tool_use_id(raw_tool_use_id);
                            let mut input = ct::JsonObject::new();
                            input.insert("pattern".to_string(), serde_json::Value::String(pattern));
                            input.insert("url".to_string(), serde_json::Value::String(url));
                            push_block_message(
                                &mut messages,
                                ct::BetaMessageRole::Assistant,
                                ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                                    id: tool_use_id,
                                    input,
                                    name: "web_fetch".to_string(),
                                    type_: ct::BetaToolUseBlockType::ToolUse,
                                    cache_control: None,
                                    caller: None,
                                }),
                            );
                        }
                    }
                }
                ot::ResponseInputItem::ShellCall(call) => {
                    let mut input = ct::JsonObject::new();
                    input.insert(
                        "commands".to_string(),
                        serde_json::Value::Array(
                            call.action
                                .commands
                                .into_iter()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                    if let Some(timeout_ms) = call.action.timeout_ms {
                        input.insert(
                            "timeout_ms".to_string(),
                            serde_json::Value::Number(timeout_ms.into()),
                        );
                    }
                    let tool_use_id = tool_use_ids.server_tool_use_id(call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ServerToolUse(ct::BetaServerToolUseBlockParam {
                            id: tool_use_id,
                            input,
                            name: ct::BetaServerToolUseName::BashCodeExecution,
                            type_: ct::BetaServerToolUseBlockType::ServerToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::ShellCallOutput(call) => {
                    if let Some(first) = call.output.into_iter().next() {
                        let tool_use_id = tool_use_ids.server_tool_use_id(call.call_id);
                        let content = match first.outcome {
                            ot::ResponseShellCallOutcome::Timeout => {
                                ct::BetaBashCodeExecutionToolResultBlockParamContent::Error(
                                    ct::BetaBashCodeExecutionToolResultErrorParam {
                                        error_code: ct::BetaBashCodeExecutionToolResultErrorCode::ExecutionTimeExceeded,
                                        type_: ct::BetaBashCodeExecutionToolResultErrorType::BashCodeExecutionToolResultError,
                                    },
                                )
                            }
                            ot::ResponseShellCallOutcome::Exit { exit_code } => {
                                ct::BetaBashCodeExecutionToolResultBlockParamContent::Result(
                                    ct::BetaBashCodeExecutionResultBlockParam {
                                        content: Vec::new(),
                                        return_code: i64::from(exit_code),
                                        stderr: first.stderr,
                                        stdout: first.stdout,
                                        type_: ct::BetaBashCodeExecutionResultBlockType::BashCodeExecutionResult,
                                    },
                                )
                            }
                        };
                        push_block_message(
                            &mut messages,
                            ct::BetaMessageRole::User,
                            ct::BetaContentBlockParam::BashCodeExecutionToolResult(
                                ct::BetaBashCodeExecutionToolResultBlockParam {
                                    content,
                                    tool_use_id,
                                    type_: ct::BetaBashCodeExecutionToolResultBlockType::BashCodeExecutionToolResult,
                                    cache_control: None,
                                },
                            ),
                        );
                    }
                }
                ot::ResponseInputItem::LocalShellCall(call) => {
                    let input = serde_json::to_value(call.action)
                        .ok()
                        .and_then(|value| serde_json::from_value::<ct::JsonObject>(value).ok())
                        .unwrap_or_default();
                    let tool_use_id = tool_use_ids.tool_use_id(call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                            id: tool_use_id,
                            input,
                            name: "bash".to_string(),
                            type_: ct::BetaToolUseBlockType::ToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::LocalShellCallOutput(call) => {
                    let tool_use_id = tool_use_ids.tool_use_id(call.id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content: (!call.output.is_empty())
                                .then_some(ct::BetaToolResultBlockParamContent::Text(call.output)),
                            is_error: None,
                        }),
                    );
                }
                ot::ResponseInputItem::ApplyPatchCall(call) => {
                    let input = serde_json::to_value(call.operation)
                        .ok()
                        .and_then(|value| serde_json::from_value::<ct::JsonObject>(value).ok())
                        .unwrap_or_default();
                    let tool_use_id = tool_use_ids.tool_use_id(call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                            id: tool_use_id,
                            input,
                            name: "str_replace_based_edit_tool".to_string(),
                            type_: ct::BetaToolUseBlockType::ToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::ApplyPatchCallOutput(call) => {
                    let text = call
                        .output
                        .unwrap_or_else(|| format!("status:{:?}", call.status));
                    let tool_use_id = tool_use_ids.tool_use_id(call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content: (!text.is_empty())
                                .then_some(ct::BetaToolResultBlockParamContent::Text(text)),
                            is_error: None,
                        }),
                    );
                }
                ot::ResponseInputItem::ComputerToolCall(call) => {
                    let input = serde_json::to_value(call.action)
                        .ok()
                        .and_then(|value| serde_json::from_value::<ct::JsonObject>(value).ok())
                        .unwrap_or_default();
                    let tool_use_id = tool_use_ids.tool_use_id(call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                            id: tool_use_id,
                            input,
                            name: "computer".to_string(),
                            type_: ct::BetaToolUseBlockType::ToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::ComputerCallOutput(call) => {
                    let mut parts = Vec::new();
                    if let Some(file_id) = call.output.file_id {
                        parts.push(ot::ResponseInputContent::Image(ot::ResponseInputImage {
                            detail: None,
                            type_: ot::ResponseInputImageType::InputImage,
                            file_id: Some(file_id),
                            image_url: None,
                        }));
                    } else if let Some(image_url) = call.output.image_url {
                        parts.push(ot::ResponseInputContent::Image(ot::ResponseInputImage {
                            detail: None,
                            type_: ot::ResponseInputImageType::InputImage,
                            file_id: None,
                            image_url: Some(image_url),
                        }));
                    }
                    let tool_use_id = tool_use_ids.tool_use_id(call.call_id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content: response_input_contents_to_tool_result_content(parts),
                            is_error: None,
                        }),
                    );
                }
                ot::ResponseInputItem::FileSearchToolCall(call) => {
                    let mut input = ct::JsonObject::new();
                    if let Some(query) = call.queries.first().cloned() {
                        input.insert("query".to_string(), serde_json::Value::String(query));
                    }
                    if call.queries.len() > 1 {
                        input.insert(
                            "queries".to_string(),
                            serde_json::Value::Array(
                                call.queries
                                    .iter()
                                    .cloned()
                                    .map(serde_json::Value::String)
                                    .collect(),
                            ),
                        );
                    }
                    let tool_use_id = tool_use_ids.server_tool_use_id(call.id);
                    push_block_message(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ServerToolUse(ct::BetaServerToolUseBlockParam {
                            id: tool_use_id.clone(),
                            input,
                            name: ct::BetaServerToolUseName::ToolSearchToolBm25,
                            type_: ct::BetaServerToolUseBlockType::ServerToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                    if let Some(results) = call.results {
                        let tool_references = results
                            .into_iter()
                            .filter_map(|result| result.filename.or(result.text))
                            .filter(|name| !name.is_empty())
                            .map(|tool_name| ct::BetaToolReferenceBlockParam {
                                tool_name,
                                type_: ct::BetaToolReferenceBlockType::ToolReference,
                                cache_control: None,
                            })
                            .collect::<Vec<_>>();
                        push_block_message(
                            &mut messages,
                            ct::BetaMessageRole::User,
                            ct::BetaContentBlockParam::ToolSearchToolResult(
                                ct::BetaToolSearchToolResultBlockParam {
                                    content: ct::BetaToolSearchToolResultBlockParamContent::Result(
                                        ct::BetaToolSearchToolSearchResultBlockParam {
                                            tool_references,
                                            type_: ct::BetaToolSearchToolSearchResultBlockType::ToolSearchToolSearchResult,
                                        },
                                    ),
                                    tool_use_id,
                                    type_: ct::BetaToolSearchToolResultBlockType::ToolSearchToolResult,
                                    cache_control: None,
                                },
                            ),
                        );
                    }
                }
                ot::ResponseInputItem::ReasoningItem(reasoning) => {
                    let mut thinking = openai_reasoning_summary_to_text(&reasoning.summary);
                    if thinking.is_empty()
                        && let Some(encrypted) = reasoning.encrypted_content
                    {
                        thinking = encrypted;
                    }

                    if !thinking.is_empty()
                        && let Some(signature) = reasoning.id.filter(|id| !id.is_empty())
                    {
                        push_block_message(
                            &mut messages,
                            ct::BetaMessageRole::Assistant,
                            ct::BetaContentBlockParam::Thinking(ct::BetaThinkingBlockParam {
                                signature,
                                thinking,
                                type_: ct::BetaThinkingBlockType::Thinking,
                            }),
                        );
                    }
                }
                other => {
                    let text = format!("{other:?}");
                    if !text.is_empty() {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Text(text),
                            role: ct::BetaMessageRole::User,
                        });
                    }
                }
            }
        }

        let claude_model = Model::Custom(body.model.clone().unwrap_or_default());
        let disable_parallel_tool_use = parallel_disable(body.parallel_tool_calls);
        let tool_choice = openai_tool_choice_to_claude(body.tool_choice, disable_parallel_tool_use);
        let thinking = openai_reasoning_to_claude(
            body.reasoning.clone(),
            body.max_output_tokens,
            Some(&claude_model),
        );
        let claude_max_tokens = body.max_output_tokens.unwrap_or(8_192);

        let output_effort = body
            .text
            .as_ref()
            .and_then(|text| text.verbosity.as_ref())
            .map(|verbosity| match verbosity {
                ot::ResponseTextVerbosity::Low => ct::BetaOutputEffort::Low,
                ot::ResponseTextVerbosity::Medium => ct::BetaOutputEffort::Medium,
                ot::ResponseTextVerbosity::High => ct::BetaOutputEffort::High,
            });

        let output_format = body
            .text
            .as_ref()
            .and_then(|text| text.format.as_ref())
            .and_then(|format| match format {
                ot::ResponseTextFormatConfig::JsonSchema(schema) => {
                    Some(ct::BetaJsonOutputFormat {
                        schema: schema.schema.clone(),
                        type_: ct::BetaJsonOutputFormatType::JsonSchema,
                    })
                }
                ot::ResponseTextFormatConfig::JsonObject(_) => Some(ct::BetaJsonOutputFormat {
                    schema: serde_json::from_str::<ct::JsonObject>(r#"{"type":"object"}"#)
                        .unwrap_or_default(),
                    type_: ct::BetaJsonOutputFormatType::JsonSchema,
                }),
                _ => None,
            });

        let output_config = if output_effort.is_some() || output_format.is_some() {
            Some(ct::BetaOutputConfig {
                effort: output_effort,
                format: output_format.clone(),
                task_budget: None,
            })
        } else {
            None
        };

        let context_management = {
            let mut edits = Vec::new();

            if let Some(entries) = body.context_management {
                for entry in entries {
                    if entry.type_ == ResponseContextManagementType::Compaction {
                        edits.push(ct::BetaContextManagementEdit::Compact(
                            ct::BetaCompact20260112Edit {
                                type_: ct::BetaCompactType::Compact20260112,
                                instructions: None,
                                pause_after_compaction: None,
                                trigger: entry.compact_threshold.map(|value| {
                                    ct::BetaInputTokensTrigger {
                                        type_: ct::BetaInputTokensCounterType::InputTokens,
                                        value,
                                    }
                                }),
                            },
                        ));
                    }
                }
            }

            if matches!(body.truncation, Some(ot::ResponseTruncation::Auto)) && edits.is_empty() {
                edits.push(ct::BetaContextManagementEdit::Compact(
                    ct::BetaCompact20260112Edit {
                        type_: ct::BetaCompactType::Compact20260112,
                        instructions: None,
                        pause_after_compaction: None,
                        trigger: None,
                    },
                ));
            }

            if edits.is_empty() {
                None
            } else {
                Some(ct::BetaContextManagementConfig { edits: Some(edits) })
            }
        };

        let mut converted_tools = Vec::new();
        let mut mcp_servers = Vec::new();
        if let Some(tools) = body.tools {
            for tool in tools {
                match tool {
                    ot::ResponseTool::Function(tool) => {
                        converted_tools.push(tool_from_function(tool))
                    }
                    ot::ResponseTool::Custom(tool) => {
                        converted_tools.push(ct::BetaToolUnion::Custom(ct::BetaTool {
                            input_schema: ct::BetaToolInputSchema {
                                type_: ct::BetaToolInputSchemaType::Object,
                                properties: None,
                                required: None,
                                extra_fields: Default::default(),
                            },
                            name: tool.name,
                            common: ct::BetaToolCommonFields::default(),
                            description: tool.description,
                            eager_input_streaming: None,
                            type_: Some(ct::BetaCustomToolType::Custom),
                        }));
                    }
                    ot::ResponseTool::CodeInterpreter(_) => {
                        converted_tools.push(ct::BetaToolUnion::CodeExecution20250825(
                            ct::BetaCodeExecutionTool20250825 {
                                name: ct::BetaCodeExecutionToolName::CodeExecution,
                                type_: ct::BetaCodeExecutionTool20250825Type::CodeExecution20250825,
                                common: ct::BetaToolCommonFields::default(),
                            },
                        ));
                    }
                    ot::ResponseTool::LocalShell(_) | ot::ResponseTool::Shell(_) => {
                        converted_tools.push(ct::BetaToolUnion::Bash20250124(
                            ct::BetaToolBash20250124 {
                                name: ct::BetaBashToolName::Bash,
                                type_: ct::BetaToolBash20250124Type::Bash20250124,
                                common: ct::BetaToolCommonFields::default(),
                            },
                        ));
                    }
                    ot::ResponseTool::ApplyPatch(_) => {
                        converted_tools.push(ct::BetaToolUnion::TextEditor20250728(
                            ct::BetaToolTextEditor20250728 {
                                name: ct::BetaTextEditorToolNameV2::StrReplaceBasedEditTool,
                                type_: ct::BetaToolTextEditor20250728Type::TextEditor20250728,
                                common: ct::BetaToolCommonFields::default(),
                                max_characters: None,
                            },
                        ));
                    }
                    ot::ResponseTool::Computer(tool) => {
                        converted_tools.push(ct::BetaToolUnion::ComputerUse20251124(
                            ct::BetaToolComputerUse20251124 {
                                display_height_px: tool.display_height_or_default(),
                                display_width_px: tool.display_width_or_default(),
                                name: ct::BetaComputerToolName::Computer,
                                type_: ct::BetaToolComputerUse20251124Type::Computer20251124,
                                common: ct::BetaToolCommonFields::default(),
                                display_number: None,
                                enable_zoom: None,
                            },
                        ));
                    }
                    ot::ResponseTool::WebSearch(tool) => {
                        converted_tools.push(ct::BetaToolUnion::WebSearch20250305(
                            ct::BetaWebSearchTool20250305 {
                                name: ct::BetaWebSearchToolName::WebSearch,
                                type_: ct::BetaWebSearchTool20250305Type::WebSearch20250305,
                                common: ct::BetaToolCommonFields::default(),
                                allowed_domains: tool.filters.and_then(|f| f.allowed_domains),
                                blocked_domains: None,
                                max_uses: None,
                                user_location: tool.user_location.map(|location| {
                                    ct::BetaWebSearchUserLocation {
                                        type_: ct::BetaWebSearchUserLocationType::Approximate,
                                        city: location.city,
                                        country: location.country,
                                        region: location.region,
                                        timezone: location.timezone,
                                    }
                                }),
                            },
                        ));
                    }
                    ot::ResponseTool::WebSearchPreview(tool) => {
                        converted_tools.push(ct::BetaToolUnion::WebSearch20250305(
                            ct::BetaWebSearchTool20250305 {
                                name: ct::BetaWebSearchToolName::WebSearch,
                                type_: ct::BetaWebSearchTool20250305Type::WebSearch20250305,
                                common: ct::BetaToolCommonFields::default(),
                                allowed_domains: None,
                                blocked_domains: None,
                                max_uses: None,
                                user_location: tool.user_location.map(|location| {
                                    ct::BetaWebSearchUserLocation {
                                        type_: ct::BetaWebSearchUserLocationType::Approximate,
                                        city: location.city,
                                        country: location.country,
                                        region: location.region,
                                        timezone: location.timezone,
                                    }
                                }),
                            },
                        ));
                    }
                    ot::ResponseTool::FileSearch(_) => {
                        converted_tools.push(ct::BetaToolUnion::ToolSearchBm25_20251119(
                            ct::BetaToolSearchToolBm25_20251119 {
                                name: ct::BetaToolSearchToolBm25Name::ToolSearchToolBm25,
                                type_: ct::BetaToolSearchToolBm25Type::ToolSearchToolBm2520251119,
                                common: ct::BetaToolCommonFields::default(),
                            },
                        ));
                    }
                    ot::ResponseTool::Mcp(tool) => {
                        if let Some(server) = openai_mcp_tool_to_server(&tool) {
                            mcp_servers.push(server);
                        }
                        converted_tools.push(ct::BetaToolUnion::McpToolset(ct::BetaMcpToolset {
                            mcp_server_name: tool.server_label,
                            type_: ct::BetaMcpToolsetType::McpToolset,
                            cache_control: None,
                            configs: mcp_allowed_tools_to_configs(tool.allowed_tools.as_ref()),
                            default_config: None,
                        }));
                    }
                    ot::ResponseTool::Namespace(_) | ot::ResponseTool::ToolSearch(_) => {}
                    ot::ResponseTool::ImageGeneration(_) => {}
                }
            }
        }

        let service_tier = match body.service_tier.as_ref() {
            Some(ResponseServiceTier::Auto) => Some(BetaServiceTierParam::Auto),
            Some(
                ResponseServiceTier::Default
                | ResponseServiceTier::Flex
                | ResponseServiceTier::Scale
                | ResponseServiceTier::Priority,
            ) => Some(BetaServiceTierParam::StandardOnly),
            None => None,
        };
        let speed = match body.service_tier.as_ref() {
            Some(ResponseServiceTier::Priority) => Some(BetaSpeed::Fast),
            _ => None,
        };

        let metadata_user_id = body.user.or_else(|| {
            body.metadata
                .as_ref()
                .and_then(|map| map.get("user_id").cloned())
        });
        let metadata = metadata_user_id.map(|user_id| BetaMetadata {
            user_id: Some(user_id),
        });

        let system = body.instructions.and_then(|text| {
            if text.is_empty() {
                None
            } else {
                Some(ct::BetaSystemPrompt::Text(text))
            }
        });

        Ok(ClaudeCreateMessageRequest {
            method: ClaudeHttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                max_tokens: claude_max_tokens,
                messages,
                model: claude_model,
                container: None,
                context_management,
                inference_geo: None,
                mcp_servers: if mcp_servers.is_empty() {
                    None
                } else {
                    Some(mcp_servers)
                },
                metadata,
                cache_control: None,
                output_config,
                service_tier,
                speed,
                stop_sequences: None,
                stream: body.stream,
                system,
                temperature: body.temperature,
                thinking,
                tool_choice,
                tools: if converted_tools.is_empty() {
                    None
                } else {
                    Some(converted_tools)
                },
                top_k: None,
                top_p: body.top_p,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_assistant_output_text_item_without_message_type() {
        let body = serde_json::from_value(serde_json::json!({
            "model": "claude-jupiter-v1-p",
            "input": [
                {
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "hello" }]
                },
                {
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "hello there" }]
                },
                {
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "who are you" }]
                }
            ],
            "store": false,
            "stream": true,
            "temperature": 1.0
        }))
        .expect("Cherry Studio Responses history should deserialize");
        let request = OpenAiCreateResponseRequest {
            body,
            ..Default::default()
        };

        let converted = ClaudeCreateMessageRequest::try_from(request).expect("request converts");

        assert_eq!(converted.body.messages.len(), 3);
        assert_eq!(
            converted.body.messages[1].role,
            ct::BetaMessageRole::Assistant
        );
        assert_eq!(
            converted.body.messages[1].content,
            ct::BetaMessageContent::Text("hello there".to_string())
        );
    }
}
