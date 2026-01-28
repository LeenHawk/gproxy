use gproxy_protocol::openai::create_chat_completions::request::{
    CreateChatCompletionRequest, CreateChatCompletionRequestBody, StopConfiguration,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionAllowedTool, ChatCompletionAllowedToolCustom, ChatCompletionAllowedToolFunction,
    ChatCompletionAllowedTools, ChatCompletionAllowedToolsChoice,
    ChatCompletionAllowedToolsChoiceType, ChatCompletionAssistantContent,
    ChatCompletionAssistantContentPart, ChatCompletionFunctionCallChoice,
    ChatCompletionFunctionCallMode, ChatCompletionImageDetail, ChatCompletionImageUrl,
    ChatCompletionInputFile, ChatCompletionNamedToolChoice, ChatCompletionNamedToolChoiceCustom,
    ChatCompletionNamedToolChoiceCustomName, ChatCompletionNamedToolChoiceCustomType,
    ChatCompletionNamedToolChoiceFunction, ChatCompletionNamedToolChoiceType,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestDeveloperMessage,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestUserMessage, ChatCompletionResponseFormat, ChatCompletionStreamOptions,
    ChatCompletionTextContent, ChatCompletionTextContentPart, ChatCompletionToolChoiceMode,
    ChatCompletionToolChoiceOption, ChatCompletionToolDefinition, ChatCompletionUserContent,
    ChatCompletionUserContentPart, FunctionObject, ResponseFormatJsonSchema,
};
use gproxy_protocol::openai::create_response::request::CreateResponseRequest;
use gproxy_protocol::openai::create_response::types::{
    AllowedTool, EasyInputMessage, EasyInputMessageContent, EasyInputMessageRole, InputContent,
    InputFileContent, InputImageContent, InputItem, InputMessage, InputMessageRole, InputParam,
    Instructions, ResponseTextParam, TextResponseFormatConfiguration, Tool, ToolChoiceAllowed,
    ToolChoiceAllowedMode, ToolChoiceOptions, ToolChoiceParam,
};

/// Convert an OpenAI responses request into an OpenAI chat-completions request.
pub fn transform_request(request: CreateResponseRequest) -> CreateChatCompletionRequest {
    let mut messages = Vec::new();

    if let Some(instructions) = request.body.instructions {
        append_instructions(instructions, &mut messages);
    }

    if let Some(input) = request.body.input {
        append_input_param(input, &mut messages);
    }

    let response_format = request.body.text.as_ref().and_then(map_response_format);
    let verbosity = request.body.text.as_ref().and_then(|text| text.verbosity);

    let tools = request
        .body
        .tools
        .map(map_tools)
        .and_then(|tools| if tools.is_empty() { None } else { Some(tools) });

    let tool_choice_param = request.body.tool_choice.clone();
    let tool_choice = tool_choice_param.clone().and_then(map_tool_choice);

    let stream_options = request
        .body
        .stream_options
        .map(|options| ChatCompletionStreamOptions {
            include_usage: None,
            include_obfuscation: options.include_obfuscation,
        });

    CreateChatCompletionRequest {
        body: CreateChatCompletionRequestBody {
            messages,
            model: request.body.model,
            modalities: None,
            verbosity,
            reasoning_effort: request
                .body
                .reasoning
                .and_then(|reasoning| reasoning.effort),
            max_completion_tokens: request.body.max_output_tokens,
            frequency_penalty: None,
            presence_penalty: None,
            web_search_options: None,
            top_logprobs: request.body.top_logprobs,
            response_format,
            audio: None,
            store: request.body.store,
            stream: request.body.stream,
            stop: map_stop_sequences(
                request
                    .body
                    .text
                    .as_ref()
                    .and_then(|text| text.format.clone()),
            ),
            logit_bias: None,
            logprobs: request.body.top_logprobs.map(|_| true),
            max_tokens: None,
            n: None,
            prediction: None,
            seed: None,
            stream_options,
            tools,
            tool_choice,
            parallel_tool_calls: request.body.parallel_tool_calls,
            function_call: map_function_call(tool_choice_param),
            functions: None,
            metadata: request.body.metadata,
            extra_body: None,
            temperature: request.body.temperature,
            top_p: request.body.top_p,
            user: request.body.user,
            safety_identifier: request.body.safety_identifier,
            prompt_cache_key: request.body.prompt_cache_key,
            service_tier: request.body.service_tier,
            prompt_cache_retention: request.body.prompt_cache_retention,
        },
    }
}

fn append_instructions(
    instructions: Instructions,
    messages: &mut Vec<ChatCompletionRequestMessage>,
) {
    match instructions {
        Instructions::Text(text) => {
            if !text.is_empty() {
                messages.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: ChatCompletionTextContent::Text(text),
                        name: None,
                    },
                ));
            }
        }
        Instructions::Items(items) => {
            for item in items {
                append_input_item(item, messages);
            }
        }
    }
}

fn append_input_param(input: InputParam, messages: &mut Vec<ChatCompletionRequestMessage>) {
    match input {
        InputParam::Text(text) => {
            messages.push(ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessage {
                    content: ChatCompletionUserContent::Text(text),
                    name: None,
                },
            ));
        }
        InputParam::Items(items) => {
            for item in items {
                append_input_item(item, messages);
            }
        }
    }
}

fn append_input_item(item: InputItem, messages: &mut Vec<ChatCompletionRequestMessage>) {
    match item {
        InputItem::EasyMessage(message) => append_easy_message(message, messages),
        InputItem::Item(item) => match item {
            gproxy_protocol::openai::create_response::types::Item::InputMessage(message) => {
                append_input_message(message, messages);
            }
            gproxy_protocol::openai::create_response::types::Item::OutputMessage(message) => {
                if let Some(msg) = map_output_message(message) {
                    messages.push(msg);
                }
            }
            _ => {}
        },
        InputItem::Reference(_) => {}
    }
}

fn append_easy_message(
    message: EasyInputMessage,
    messages: &mut Vec<ChatCompletionRequestMessage>,
) {
    match message.role {
        EasyInputMessageRole::User => {
            if let Some(content) = map_easy_content_to_user_content(message.content) {
                messages.push(ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessage {
                        content,
                        name: None,
                    },
                ));
            }
        }
        EasyInputMessageRole::Assistant => {
            if let Some(content) = map_easy_content_to_assistant_content(message.content) {
                messages.push(ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessage {
                        content: Some(content),
                        refusal: None,
                        name: None,
                        audio: None,
                        tool_calls: None,
                        function_call: None,
                    },
                ));
            }
        }
        EasyInputMessageRole::System => {
            if let Some(content) = map_easy_content_to_text(message.content) {
                messages.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content,
                        name: None,
                    },
                ));
            }
        }
        EasyInputMessageRole::Developer => {
            if let Some(content) = map_easy_content_to_text(message.content) {
                messages.push(ChatCompletionRequestMessage::Developer(
                    ChatCompletionRequestDeveloperMessage {
                        content,
                        name: None,
                    },
                ));
            }
        }
    }
}

fn append_input_message(message: InputMessage, messages: &mut Vec<ChatCompletionRequestMessage>) {
    match message.role {
        InputMessageRole::User => {
            if let Some(content) = map_input_contents_to_user_content(&message.content) {
                messages.push(ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessage {
                        content,
                        name: None,
                    },
                ));
            }
        }
        InputMessageRole::System => {
            if let Some(content) = map_input_contents_to_text(&message.content) {
                messages.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content,
                        name: None,
                    },
                ));
            }
        }
        InputMessageRole::Developer => {
            if let Some(content) = map_input_contents_to_text(&message.content) {
                messages.push(ChatCompletionRequestMessage::Developer(
                    ChatCompletionRequestDeveloperMessage {
                        content,
                        name: None,
                    },
                ));
            }
        }
    }
}

fn map_output_message(
    message: gproxy_protocol::openai::create_response::types::OutputMessage,
) -> Option<ChatCompletionRequestMessage> {
    let (text, refusal) = extract_output_message_text(&message.content);

    if let Some(refusal) = refusal {
        return Some(ChatCompletionRequestMessage::Assistant(
            ChatCompletionRequestAssistantMessage {
                content: Some(ChatCompletionAssistantContent::Parts(vec![
                    ChatCompletionAssistantContentPart::Refusal { refusal },
                ])),
                refusal: None,
                name: None,
                audio: None,
                tool_calls: None,
                function_call: None,
            },
        ));
    }

    text.map(|text| {
        ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
            content: Some(ChatCompletionAssistantContent::Text(text)),
            refusal: None,
            name: None,
            audio: None,
            tool_calls: None,
            function_call: None,
        })
    })
}

fn extract_output_message_text(
    contents: &[gproxy_protocol::openai::create_response::types::OutputMessageContent],
) -> (Option<String>, Option<String>) {
    let mut texts = Vec::new();
    let mut refusals = Vec::new();
    for content in contents {
        match content {
            gproxy_protocol::openai::create_response::types::OutputMessageContent::OutputText(
                output,
            ) => {
                if !output.text.is_empty() {
                    texts.push(output.text.clone());
                }
            }
            gproxy_protocol::openai::create_response::types::OutputMessageContent::Refusal(
                refusal,
            ) => {
                if !refusal.refusal.is_empty() {
                    refusals.push(refusal.refusal.clone());
                }
            }
        }
    }

    if !refusals.is_empty() {
        return (None, Some(refusals.join("\n")));
    }

    if texts.is_empty() {
        (None, None)
    } else {
        (Some(texts.join("\n")), None)
    }
}

fn map_easy_content_to_user_content(
    content: EasyInputMessageContent,
) -> Option<ChatCompletionUserContent> {
    match content {
        EasyInputMessageContent::Text(text) => Some(ChatCompletionUserContent::Text(text)),
        EasyInputMessageContent::Parts(parts) => map_input_contents_to_user_content(&parts),
    }
}

fn map_easy_content_to_assistant_content(
    content: EasyInputMessageContent,
) -> Option<ChatCompletionAssistantContent> {
    match content {
        EasyInputMessageContent::Text(text) => Some(ChatCompletionAssistantContent::Text(text)),
        EasyInputMessageContent::Parts(parts) => map_input_contents_to_text(&parts)
            .and_then(chat_text_content_to_string)
            .map(ChatCompletionAssistantContent::Text),
    }
}

fn map_easy_content_to_text(content: EasyInputMessageContent) -> Option<ChatCompletionTextContent> {
    match content {
        EasyInputMessageContent::Text(text) => {
            if text.is_empty() {
                None
            } else {
                Some(ChatCompletionTextContent::Text(text))
            }
        }
        EasyInputMessageContent::Parts(parts) => map_input_contents_to_text(&parts),
    }
}

fn map_input_contents_to_text(contents: &[InputContent]) -> Option<ChatCompletionTextContent> {
    let mut texts = Vec::new();
    for content in contents {
        match content {
            InputContent::InputText(text) => {
                if !text.text.is_empty() {
                    texts.push(text.text.clone());
                }
            }
            InputContent::InputImage(image) => {
                if let Some(value) = map_input_image_to_label(image) {
                    texts.push(value);
                }
            }
            InputContent::InputFile(file) => {
                if let Some(value) = map_input_file_to_label(file) {
                    texts.push(value);
                }
            }
        }
    }

    if texts.is_empty() {
        None
    } else if texts.len() == 1 {
        Some(ChatCompletionTextContent::Text(texts[0].clone()))
    } else {
        Some(ChatCompletionTextContent::Parts(
            texts
                .into_iter()
                .map(|text| ChatCompletionTextContentPart::Text { text })
                .collect(),
        ))
    }
}

fn chat_text_content_to_string(content: ChatCompletionTextContent) -> Option<String> {
    match content {
        ChatCompletionTextContent::Text(text) => {
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        ChatCompletionTextContent::Parts(parts) => {
            let texts: Vec<String> = parts
                .into_iter()
                .filter_map(|part| match part {
                    ChatCompletionTextContentPart::Text { text } => {
                        if text.is_empty() {
                            None
                        } else {
                            Some(text)
                        }
                    }
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
    }
}

fn map_input_contents_to_user_content(
    contents: &[InputContent],
) -> Option<ChatCompletionUserContent> {
    if contents.len() == 1
        && let InputContent::InputText(text) = &contents[0]
    {
        return Some(ChatCompletionUserContent::Text(text.text.clone()));
    }

    let mut parts = Vec::new();
    for content in contents {
        match content {
            InputContent::InputText(text) => {
                parts.push(ChatCompletionUserContentPart::Text {
                    text: text.text.clone(),
                });
            }
            InputContent::InputImage(image) => {
                if let Some(part) = map_input_image_to_part(image) {
                    parts.push(part);
                }
            }
            InputContent::InputFile(file) => {
                if let Some(part) = map_input_file_to_part(file) {
                    parts.push(part);
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(ChatCompletionUserContent::Parts(parts))
    }
}

fn map_input_image_to_part(image: &InputImageContent) -> Option<ChatCompletionUserContentPart> {
    if let Some(url) = &image.image_url {
        let detail = image.detail.and_then(map_image_detail);
        return Some(ChatCompletionUserContentPart::ImageUrl {
            image_url: ChatCompletionImageUrl {
                url: url.clone(),
                detail,
            },
        });
    }

    if let Some(file_id) = &image.file_id {
        return Some(ChatCompletionUserContentPart::File {
            file: ChatCompletionInputFile {
                filename: None,
                file_data: None,
                file_id: Some(file_id.clone()),
            },
        });
    }

    None
}

fn map_input_file_to_part(file: &InputFileContent) -> Option<ChatCompletionUserContentPart> {
    if file.file_url.is_some() {
        return Some(ChatCompletionUserContentPart::Text {
            text: map_input_file_to_label(file).unwrap_or_else(|| "[file]".to_string()),
        });
    }

    Some(ChatCompletionUserContentPart::File {
        file: ChatCompletionInputFile {
            filename: file.filename.clone(),
            file_data: file.file_data.clone(),
            file_id: file.file_id.clone(),
        },
    })
}

fn map_input_image_to_label(image: &InputImageContent) -> Option<String> {
    if let Some(url) = &image.image_url {
        return Some(format!("[image:{}]", url));
    }
    image
        .file_id
        .as_ref()
        .map(|id| format!("[image_file:{}]", id))
}

fn map_input_file_to_label(file: &InputFileContent) -> Option<String> {
    if let Some(url) = &file.file_url {
        return Some(format!("[file_url:{}]", url));
    }
    if let Some(id) = &file.file_id {
        return Some(format!("[file_id:{}]", id));
    }
    if let Some(name) = &file.filename {
        return Some(format!("[file:{}]", name));
    }
    None
}

fn map_image_detail(
    detail: gproxy_protocol::openai::create_response::types::ImageDetail,
) -> Option<ChatCompletionImageDetail> {
    match detail {
        gproxy_protocol::openai::create_response::types::ImageDetail::Auto => {
            Some(ChatCompletionImageDetail::Auto)
        }
        gproxy_protocol::openai::create_response::types::ImageDetail::Low => {
            Some(ChatCompletionImageDetail::Low)
        }
        gproxy_protocol::openai::create_response::types::ImageDetail::High => {
            Some(ChatCompletionImageDetail::High)
        }
    }
}

fn map_response_format(text: &ResponseTextParam) -> Option<ChatCompletionResponseFormat> {
    let format = text.format.as_ref()?;
    Some(match format {
        TextResponseFormatConfiguration::Text => ChatCompletionResponseFormat::Text,
        TextResponseFormatConfiguration::JsonObject => ChatCompletionResponseFormat::JsonObject,
        TextResponseFormatConfiguration::JsonSchema {
            name,
            description,
            schema,
            strict,
        } => {
            let parsed_schema = serde_json::from_value(schema.clone()).ok();
            ChatCompletionResponseFormat::JsonSchema {
                json_schema: ResponseFormatJsonSchema {
                    name: name.clone(),
                    description: description.clone(),
                    schema: parsed_schema,
                    strict: *strict,
                },
            }
        }
    })
}

fn map_tools(tools: Vec<Tool>) -> Vec<ChatCompletionToolDefinition> {
    let mut output = Vec::new();

    for tool in tools {
        match tool {
            Tool::Function(function) => {
                let parameters = function
                    .parameters
                    .and_then(|schema| serde_json::from_value(schema).ok());
                output.push(ChatCompletionToolDefinition::Function {
                    function: FunctionObject {
                        name: function.name,
                        description: function.description,
                        parameters,
                        strict: function.strict,
                    },
                });
            }
            Tool::Custom(custom) => {
                let format = custom.format.map(|format| match format {
                    gproxy_protocol::openai::create_response::types::CustomToolFormat::Text => {
                        gproxy_protocol::openai::create_chat_completions::types::CustomToolFormat::Text
                    }
                    gproxy_protocol::openai::create_response::types::CustomToolFormat::Grammar {
                        syntax,
                        definition,
                    } => {
                        gproxy_protocol::openai::create_chat_completions::types::CustomToolFormat::Grammar {
                            grammar: gproxy_protocol::openai::create_chat_completions::types::CustomToolGrammar {
                                definition,
                                syntax: match syntax {
                                    gproxy_protocol::openai::create_response::types::GrammarSyntax::Lark => {
                                        gproxy_protocol::openai::create_chat_completions::types::GrammarSyntax::Lark
                                    }
                                    gproxy_protocol::openai::create_response::types::GrammarSyntax::Regex => {
                                        gproxy_protocol::openai::create_chat_completions::types::GrammarSyntax::Regex
                                    }
                                },
                            },
                        }
                    }
                });

                output.push(ChatCompletionToolDefinition::Custom {
                    custom: gproxy_protocol::openai::create_chat_completions::types::CustomToolDefinition {
                        name: custom.name,
                        description: custom.description,
                        format,
                    },
                });
            }
            _ => {}
        }
    }

    output
}

fn map_tool_choice(choice: ToolChoiceParam) -> Option<ChatCompletionToolChoiceOption> {
    match choice {
        ToolChoiceParam::Mode(mode) => Some(ChatCompletionToolChoiceOption::Mode(match mode {
            ToolChoiceOptions::None => ChatCompletionToolChoiceMode::None,
            ToolChoiceOptions::Auto => ChatCompletionToolChoiceMode::Auto,
            ToolChoiceOptions::Required => ChatCompletionToolChoiceMode::Required,
        })),
        ToolChoiceParam::Allowed(allowed) => map_allowed_tools_choice(allowed),
        ToolChoiceParam::Function(function) => Some(ChatCompletionToolChoiceOption::NamedTool(
            ChatCompletionNamedToolChoice {
                r#type: ChatCompletionNamedToolChoiceType::Function,
                function: ChatCompletionNamedToolChoiceFunction {
                    name: function.name,
                },
            },
        )),
        ToolChoiceParam::Custom(custom) => Some(ChatCompletionToolChoiceOption::NamedCustomTool(
            ChatCompletionNamedToolChoiceCustom {
                r#type: ChatCompletionNamedToolChoiceCustomType::Custom,
                custom: ChatCompletionNamedToolChoiceCustomName { name: custom.name },
            },
        )),
        _ => None,
    }
}

fn map_allowed_tools_choice(allowed: ToolChoiceAllowed) -> Option<ChatCompletionToolChoiceOption> {
    let mut tools = Vec::new();
    for tool in allowed.tools {
        match tool {
            AllowedTool::Function { name } => tools.push(ChatCompletionAllowedTool::Function {
                function: ChatCompletionAllowedToolFunction { name },
            }),
            AllowedTool::Custom { name } => tools.push(ChatCompletionAllowedTool::Custom {
                custom: ChatCompletionAllowedToolCustom { name },
            }),
            _ => {}
        }
    }

    if tools.is_empty() {
        return None;
    }

    let mode = match allowed.mode {
        ToolChoiceAllowedMode::Auto => {
            gproxy_protocol::openai::create_chat_completions::types::AllowedToolsMode::Auto
        }
        ToolChoiceAllowedMode::Required => {
            gproxy_protocol::openai::create_chat_completions::types::AllowedToolsMode::Required
        }
    };

    Some(ChatCompletionToolChoiceOption::AllowedTools(
        ChatCompletionAllowedToolsChoice {
            r#type: ChatCompletionAllowedToolsChoiceType::AllowedTools,
            allowed_tools: ChatCompletionAllowedTools { mode, tools },
        },
    ))
}

fn map_function_call(choice: Option<ToolChoiceParam>) -> Option<ChatCompletionFunctionCallChoice> {
    match choice? {
        ToolChoiceParam::Mode(mode) => Some(ChatCompletionFunctionCallChoice::Mode(match mode {
            ToolChoiceOptions::None => ChatCompletionFunctionCallMode::None,
            ToolChoiceOptions::Auto | ToolChoiceOptions::Required => ChatCompletionFunctionCallMode::Auto,
        })),
        ToolChoiceParam::Function(function) => Some(ChatCompletionFunctionCallChoice::Named(
            gproxy_protocol::openai::create_chat_completions::types::ChatCompletionFunctionCallOption {
                name: function.name,
            },
        )),
        _ => None,
    }
}

fn map_stop_sequences(
    format: Option<TextResponseFormatConfiguration>,
) -> Option<StopConfiguration> {
    // Responses API does not carry explicit stop sequences; preserve defaults.
    let _ = format;
    None
}
