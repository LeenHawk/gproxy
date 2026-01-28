use gproxy_protocol::openai::create_chat_completions::request::CreateChatCompletionRequest;
use gproxy_protocol::openai::create_chat_completions::types::{
    AllowedToolsMode, ChatCompletionAllowedTool, ChatCompletionAllowedToolsChoice,
    ChatCompletionFunctionCallChoice, ChatCompletionFunctionCallMode,
    ChatCompletionFunctionCallOption, ChatCompletionMessageToolCall,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestDeveloperMessage,
    ChatCompletionRequestFunctionMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestToolMessage,
    ChatCompletionRequestUserMessage, ChatCompletionResponseFormat, ChatCompletionTextContent,
    ChatCompletionTextContentPart, ChatCompletionToolChoiceMode, ChatCompletionToolChoiceOption,
    ChatCompletionToolDefinition, ChatCompletionUserContent, ChatCompletionUserContentPart,
};
use gproxy_protocol::openai::create_response::request::CreateResponseRequest;
use gproxy_protocol::openai::create_response::types::{
    AllowedTool, CustomToolCall, CustomToolCallType, EasyInputMessage, EasyInputMessageContent,
    EasyInputMessageRole, EasyInputMessageType, FunctionCallItemStatus,
    FunctionCallOutputItemParam, FunctionCallOutputItemType, FunctionToolCall,
    FunctionToolCallType, InputContent, InputFileContent, InputImageContent, InputItem,
    InputMessage, InputMessageRole, InputMessageType, InputParam, Instructions, MessageStatus,
    OutputMessage, OutputMessageContent, OutputMessageRole, OutputMessageType,
    ResponseStreamOptions, ResponseTextParam, TextResponseFormatConfiguration, Tool,
    ToolCallOutput, ToolChoiceAllowed, ToolChoiceAllowedMode, ToolChoiceAllowedType,
    ToolChoiceCustom, ToolChoiceCustomType, ToolChoiceFunction, ToolChoiceFunctionType,
    ToolChoiceOptions, ToolChoiceParam,
};

/// Convert an OpenAI chat-completions request into an OpenAI responses request.
pub fn transform_request(request: CreateChatCompletionRequest) -> CreateResponseRequest {
    let mut instruction_items = Vec::new();
    let mut input_items = Vec::new();
    let mut tool_call_index = 0usize;

    for message in request.body.messages {
        match message {
            ChatCompletionRequestMessage::System(system) => {
                if let Some(item) = map_system_message(system) {
                    instruction_items.push(item);
                }
            }
            ChatCompletionRequestMessage::Developer(developer) => {
                if let Some(item) = map_developer_message(developer) {
                    instruction_items.push(item);
                }
            }
            ChatCompletionRequestMessage::User(user) => {
                if let Some(item) = map_user_message(user) {
                    input_items.push(item);
                }
            }
            ChatCompletionRequestMessage::Assistant(assistant) => {
                map_assistant_message(assistant, &mut input_items, &mut tool_call_index);
            }
            ChatCompletionRequestMessage::Tool(tool) => {
                input_items.push(map_tool_message(tool));
            }
            ChatCompletionRequestMessage::Function(function) => {
                input_items.push(map_function_message(function));
            }
        }
    }

    let instructions = if instruction_items.is_empty() {
        None
    } else {
        Some(Instructions::Items(instruction_items))
    };

    let input = if input_items.is_empty() {
        None
    } else {
        Some(InputParam::Items(input_items))
    };

    let mut tools = map_tools_from_definitions(request.body.tools);
    tools.extend(map_tools_from_functions(request.body.functions));
    let tools = if tools.is_empty() { None } else { Some(tools) };

    let mut tool_choice = request.body.tool_choice.clone().and_then(map_tool_choice);
    if tool_choice.is_none() {
        tool_choice = map_function_call(request.body.function_call);
    }

    let text = map_response_text(request.body.response_format, request.body.verbosity);

    CreateResponseRequest {
        body: gproxy_protocol::openai::create_response::request::CreateResponseRequestBody {
            model: request.body.model,
            input,
            include: None,
            parallel_tool_calls: request.body.parallel_tool_calls,
            store: request.body.store,
            instructions,
            stream: request.body.stream,
            stream_options: request.body.stream_options.map(map_stream_options),
            conversation: None,
            previous_response_id: None,
            reasoning: request.body.reasoning_effort.map(|effort| {
                gproxy_protocol::openai::create_response::types::Reasoning {
                    effort: Some(effort),
                    summary: None,
                    generate_summary: None,
                }
            }),
            background: None,
            max_output_tokens: map_max_output_tokens(
                request.body.max_completion_tokens,
                request.body.max_tokens,
            ),
            max_tool_calls: None,
            text,
            tools,
            tool_choice,
            prompt: None,
            truncation: None,
            top_logprobs: request.body.top_logprobs,
            metadata: request.body.metadata,
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

fn map_system_message(message: ChatCompletionRequestSystemMessage) -> Option<InputItem> {
    let content = map_text_content_to_easy_content(message.content)?;
    Some(InputItem::EasyMessage(EasyInputMessage {
        r#type: EasyInputMessageType::Message,
        role: EasyInputMessageRole::System,
        content,
    }))
}

fn map_developer_message(message: ChatCompletionRequestDeveloperMessage) -> Option<InputItem> {
    let content = map_text_content_to_easy_content(message.content)?;
    Some(InputItem::EasyMessage(EasyInputMessage {
        r#type: EasyInputMessageType::Message,
        role: EasyInputMessageRole::Developer,
        content,
    }))
}

fn map_user_message(message: ChatCompletionRequestUserMessage) -> Option<InputItem> {
    let contents = map_user_content_to_input_contents(message.content);
    if contents.is_empty() {
        return None;
    }

    Some(InputItem::Item(
        gproxy_protocol::openai::create_response::types::Item::InputMessage(InputMessage {
            r#type: Some(InputMessageType::Message),
            role: InputMessageRole::User,
            status: None,
            content: contents,
        }),
    ))
}

fn map_assistant_message(
    message: ChatCompletionRequestAssistantMessage,
    items: &mut Vec<InputItem>,
    tool_call_index: &mut usize,
) {
    if let Some(output) = map_assistant_output_message(&message) {
        items.push(InputItem::Item(
            gproxy_protocol::openai::create_response::types::Item::OutputMessage(output),
        ));
    }

    if let Some(tool_calls) = message.tool_calls {
        for call in tool_calls {
            if let Some(item) = map_tool_call_item(call) {
                items.push(InputItem::Item(item));
                *tool_call_index += 1;
            }
        }
    }

    if let Some(function_call) = message.function_call {
        let call_id = format!("function_call_{}", tool_call_index);
        *tool_call_index += 1;
        items.push(InputItem::Item(
            gproxy_protocol::openai::create_response::types::Item::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(call_id.clone()),
                call_id,
                name: function_call.name,
                arguments: function_call.arguments,
                status: None,
            }),
        ));
    }
}

fn map_assistant_output_message(
    message: &ChatCompletionRequestAssistantMessage,
) -> Option<OutputMessage> {
    let mut content = Vec::new();

    if let Some(refusal) = &message.refusal
        && !refusal.is_empty()
    {
        content.push(OutputMessageContent::Refusal(
            gproxy_protocol::openai::create_response::types::RefusalContent {
                refusal: refusal.clone(),
            },
        ));
    }

    if let Some(text) = message
        .content
        .as_ref()
        .and_then(map_assistant_content_to_text)
        && !text.is_empty()
    {
        content.push(OutputMessageContent::OutputText(
            gproxy_protocol::openai::create_response::types::OutputTextContent {
                text,
                annotations: Vec::new(),
                logprobs: None,
            },
        ));
    }

    if content.is_empty() {
        return None;
    }

    Some(OutputMessage {
        id: "assistant".to_string(),
        r#type: OutputMessageType::Message,
        role: OutputMessageRole::Assistant,
        content,
        status: MessageStatus::Completed,
    })
}

fn map_assistant_content_to_text(
    content: &gproxy_protocol::openai::create_chat_completions::types::ChatCompletionAssistantContent,
) -> Option<String> {
    use gproxy_protocol::openai::create_chat_completions::types::ChatCompletionAssistantContent;

    match content {
        ChatCompletionAssistantContent::Text(text) => Some(text.clone()),
        ChatCompletionAssistantContent::Parts(parts) => {
            let mut texts = Vec::new();
            for part in parts {
                match part {
                    gproxy_protocol::openai::create_chat_completions::types::ChatCompletionAssistantContentPart::Text { text } => {
                        if !text.is_empty() {
                            texts.push(text.clone());
                        }
                    }
                    gproxy_protocol::openai::create_chat_completions::types::ChatCompletionAssistantContentPart::Refusal { refusal } => {
                        if !refusal.is_empty() {
                            texts.push(refusal.clone());
                        }
                    }
                }
            }
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
    }
}

fn map_tool_message(message: ChatCompletionRequestToolMessage) -> InputItem {
    let output_text = map_text_content_to_string(message.content).unwrap_or_default();
    InputItem::Item(
        gproxy_protocol::openai::create_response::types::Item::FunctionOutput(
            FunctionCallOutputItemParam {
                r#type: FunctionCallOutputItemType::FunctionCallOutput,
                id: None,
                call_id: message.tool_call_id,
                output: ToolCallOutput::Text(output_text),
                status: Some(FunctionCallItemStatus::Completed),
            },
        ),
    )
}

fn map_function_message(message: ChatCompletionRequestFunctionMessage) -> InputItem {
    let output_text = message.content.unwrap_or_default();
    InputItem::Item(
        gproxy_protocol::openai::create_response::types::Item::FunctionOutput(
            FunctionCallOutputItemParam {
                r#type: FunctionCallOutputItemType::FunctionCallOutput,
                id: None,
                call_id: message.name,
                output: ToolCallOutput::Text(output_text),
                status: Some(FunctionCallItemStatus::Completed),
            },
        ),
    )
}

fn map_tool_call_item(
    call: ChatCompletionMessageToolCall,
) -> Option<gproxy_protocol::openai::create_response::types::Item> {
    match call {
        ChatCompletionMessageToolCall::Function { id, function } => Some(
            gproxy_protocol::openai::create_response::types::Item::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(id.clone()),
                call_id: id,
                name: function.name,
                arguments: function.arguments,
                status: None,
            }),
        ),
        ChatCompletionMessageToolCall::Custom { id, custom } => Some(
            gproxy_protocol::openai::create_response::types::Item::CustomToolCall(CustomToolCall {
                r#type: CustomToolCallType::CustomToolCall,
                id: Some(id.clone()),
                call_id: id,
                name: custom.name,
                input: custom.input,
            }),
        ),
    }
}

fn map_text_content_to_easy_content(
    content: ChatCompletionTextContent,
) -> Option<EasyInputMessageContent> {
    match content {
        ChatCompletionTextContent::Text(text) => {
            if text.is_empty() {
                None
            } else {
                Some(EasyInputMessageContent::Text(text))
            }
        }
        ChatCompletionTextContent::Parts(parts) => {
            let mut items = Vec::new();
            for part in parts {
                let ChatCompletionTextContentPart::Text { text } = part;
                if !text.is_empty() {
                    items.push(InputContent::InputText(
                        gproxy_protocol::openai::create_response::types::InputTextContent { text },
                    ));
                }
            }
            if items.is_empty() {
                None
            } else {
                Some(EasyInputMessageContent::Parts(items))
            }
        }
    }
}

fn map_text_content_to_string(content: ChatCompletionTextContent) -> Option<String> {
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

fn map_user_content_to_input_contents(content: ChatCompletionUserContent) -> Vec<InputContent> {
    let mut contents = Vec::new();
    match content {
        ChatCompletionUserContent::Text(text) => {
            contents.push(InputContent::InputText(
                gproxy_protocol::openai::create_response::types::InputTextContent { text },
            ));
        }
        ChatCompletionUserContent::Parts(parts) => {
            for part in parts {
                match part {
                    ChatCompletionUserContentPart::Text { text } => {
                        contents.push(InputContent::InputText(
                            gproxy_protocol::openai::create_response::types::InputTextContent {
                                text,
                            },
                        ));
                    }
                    ChatCompletionUserContentPart::ImageUrl { image_url } => {
                        contents.push(InputContent::InputImage(InputImageContent {
                            image_url: Some(image_url.url),
                            file_id: None,
                            detail: image_url.detail.and_then(map_image_detail),
                        }));
                    }
                    ChatCompletionUserContentPart::InputAudio { input_audio } => {
                        let filename = match input_audio.format {
                            gproxy_protocol::openai::create_chat_completions::types::ChatCompletionInputAudioFormat::Wav => "audio.wav",
                            gproxy_protocol::openai::create_chat_completions::types::ChatCompletionInputAudioFormat::Mp3 => "audio.mp3",
                        };
                        contents.push(InputContent::InputFile(InputFileContent {
                            file_id: None,
                            filename: Some(filename.to_string()),
                            file_url: None,
                            file_data: Some(input_audio.data),
                        }));
                    }
                    ChatCompletionUserContentPart::File { file } => {
                        contents.push(InputContent::InputFile(InputFileContent {
                            file_id: file.file_id,
                            filename: file.filename,
                            file_url: None,
                            file_data: file.file_data,
                        }));
                    }
                }
            }
        }
    }
    contents
}

fn map_image_detail(
    detail: gproxy_protocol::openai::create_chat_completions::types::ChatCompletionImageDetail,
) -> Option<gproxy_protocol::openai::create_response::types::ImageDetail> {
    match detail {
        gproxy_protocol::openai::create_chat_completions::types::ChatCompletionImageDetail::Auto => {
            Some(gproxy_protocol::openai::create_response::types::ImageDetail::Auto)
        }
        gproxy_protocol::openai::create_chat_completions::types::ChatCompletionImageDetail::Low => {
            Some(gproxy_protocol::openai::create_response::types::ImageDetail::Low)
        }
        gproxy_protocol::openai::create_chat_completions::types::ChatCompletionImageDetail::High => {
            Some(gproxy_protocol::openai::create_response::types::ImageDetail::High)
        }
    }
}

fn map_tools_from_definitions(tools: Option<Vec<ChatCompletionToolDefinition>>) -> Vec<Tool> {
    let mut output = Vec::new();
    let tools = match tools {
        Some(tools) => tools,
        None => return output,
    };

    for tool in tools {
        match tool {
            ChatCompletionToolDefinition::Function { function } => {
                output.push(Tool::Function(
                    gproxy_protocol::openai::create_response::types::FunctionTool {
                        name: function.name,
                        description: function.description,
                        parameters: function
                            .parameters
                            .and_then(|schema| serde_json::to_value(schema).ok()),
                        strict: function.strict,
                    },
                ));
            }
            ChatCompletionToolDefinition::Custom { custom } => {
                output.push(Tool::Custom(gproxy_protocol::openai::create_response::types::CustomTool {
                    name: custom.name,
                    description: custom.description,
                    format: custom.format.map(|format| match format {
                        gproxy_protocol::openai::create_chat_completions::types::CustomToolFormat::Text => {
                            gproxy_protocol::openai::create_response::types::CustomToolFormat::Text
                        }
                        gproxy_protocol::openai::create_chat_completions::types::CustomToolFormat::Grammar { grammar } => {
                            gproxy_protocol::openai::create_response::types::CustomToolFormat::Grammar {
                                syntax: match grammar.syntax {
                                    gproxy_protocol::openai::create_chat_completions::types::GrammarSyntax::Lark => {
                                        gproxy_protocol::openai::create_response::types::GrammarSyntax::Lark
                                    }
                                    gproxy_protocol::openai::create_chat_completions::types::GrammarSyntax::Regex => {
                                        gproxy_protocol::openai::create_response::types::GrammarSyntax::Regex
                                    }
                                },
                                definition: grammar.definition,
                            }
                        }
                    }),
                }));
            }
        }
    }

    output
}

fn map_tools_from_functions(
    functions: Option<
        Vec<gproxy_protocol::openai::create_chat_completions::types::ChatCompletionFunctions>,
    >,
) -> Vec<Tool> {
    let mut output = Vec::new();
    let functions = match functions {
        Some(functions) => functions,
        None => return output,
    };

    for function in functions {
        output.push(Tool::Function(
            gproxy_protocol::openai::create_response::types::FunctionTool {
                name: function.name,
                description: function.description,
                parameters: function
                    .parameters
                    .and_then(|schema| serde_json::to_value(schema).ok()),
                strict: None,
            },
        ));
    }

    output
}

fn map_tool_choice(choice: ChatCompletionToolChoiceOption) -> Option<ToolChoiceParam> {
    match choice {
        ChatCompletionToolChoiceOption::Mode(mode) => Some(ToolChoiceParam::Mode(match mode {
            ChatCompletionToolChoiceMode::None => ToolChoiceOptions::None,
            ChatCompletionToolChoiceMode::Auto => ToolChoiceOptions::Auto,
            ChatCompletionToolChoiceMode::Required => ToolChoiceOptions::Required,
        })),
        ChatCompletionToolChoiceOption::AllowedTools(allowed) => {
            map_allowed_tools_choice(allowed).map(ToolChoiceParam::Allowed)
        }
        ChatCompletionToolChoiceOption::NamedTool(named) => {
            Some(ToolChoiceParam::Function(ToolChoiceFunction {
                r#type: ToolChoiceFunctionType::Function,
                name: named.function.name,
            }))
        }
        ChatCompletionToolChoiceOption::NamedCustomTool(named) => {
            Some(ToolChoiceParam::Custom(ToolChoiceCustom {
                r#type: ToolChoiceCustomType::Custom,
                name: named.custom.name,
            }))
        }
    }
}

fn map_allowed_tools_choice(
    allowed: ChatCompletionAllowedToolsChoice,
) -> Option<ToolChoiceAllowed> {
    let mut tools = Vec::new();
    for tool in allowed.allowed_tools.tools {
        match tool {
            ChatCompletionAllowedTool::Function { function } => {
                tools.push(AllowedTool::Function {
                    name: function.name,
                });
            }
            ChatCompletionAllowedTool::Custom { custom } => {
                tools.push(AllowedTool::Custom { name: custom.name });
            }
        }
    }

    if tools.is_empty() {
        return None;
    }

    let mode = match allowed.allowed_tools.mode {
        AllowedToolsMode::Auto => ToolChoiceAllowedMode::Auto,
        AllowedToolsMode::Required => ToolChoiceAllowedMode::Required,
    };

    Some(ToolChoiceAllowed {
        r#type: ToolChoiceAllowedType::AllowedTools,
        mode,
        tools,
    })
}

fn map_function_call(choice: Option<ChatCompletionFunctionCallChoice>) -> Option<ToolChoiceParam> {
    match choice? {
        ChatCompletionFunctionCallChoice::Mode(mode) => Some(ToolChoiceParam::Mode(match mode {
            ChatCompletionFunctionCallMode::None => ToolChoiceOptions::None,
            ChatCompletionFunctionCallMode::Auto => ToolChoiceOptions::Auto,
        })),
        ChatCompletionFunctionCallChoice::Named(ChatCompletionFunctionCallOption { name }) => {
            Some(ToolChoiceParam::Function(ToolChoiceFunction {
                r#type: ToolChoiceFunctionType::Function,
                name,
            }))
        }
    }
}

fn map_response_text(
    format: Option<ChatCompletionResponseFormat>,
    verbosity: Option<gproxy_protocol::openai::create_chat_completions::types::Verbosity>,
) -> Option<ResponseTextParam> {
    if format.is_none() && verbosity.is_none() {
        return None;
    }

    let format = format.map(|format| match format {
        ChatCompletionResponseFormat::Text => TextResponseFormatConfiguration::Text,
        ChatCompletionResponseFormat::JsonObject => TextResponseFormatConfiguration::JsonObject,
        ChatCompletionResponseFormat::JsonSchema { json_schema } => {
            let schema = json_schema
                .schema
                .and_then(|schema| serde_json::to_value(schema).ok())
                .unwrap_or_else(|| serde_json::json!({"type": "object"}));
            TextResponseFormatConfiguration::JsonSchema {
                name: json_schema.name,
                description: json_schema.description,
                schema,
                strict: json_schema.strict,
            }
        }
    });

    Some(ResponseTextParam { format, verbosity })
}

fn map_stream_options(
    options: gproxy_protocol::openai::create_chat_completions::types::ChatCompletionStreamOptions,
) -> ResponseStreamOptions {
    ResponseStreamOptions {
        include_obfuscation: options.include_obfuscation,
    }
}

fn map_max_output_tokens(
    max_completion_tokens: Option<i64>,
    max_tokens: Option<i64>,
) -> Option<i64> {
    max_completion_tokens.or(max_tokens)
}
