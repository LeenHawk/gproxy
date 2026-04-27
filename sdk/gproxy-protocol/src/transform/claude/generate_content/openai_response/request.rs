use crate::claude::count_tokens::types::{
    BetaContextManagementEdit, BetaMessageRole, BetaOutputEffort, BetaThinkingConfigParam,
    BetaToolChoice, BetaToolInputSchema, BetaToolInputSchemaType, BetaToolUnion,
};
use crate::claude::create_message::request::ClaudeCreateMessageRequest;
use crate::claude::create_message::types::{BetaServiceTierParam, BetaSpeed};
use crate::openai::count_tokens::types::{
    HttpMethod, ResponseApplyPatchTool, ResponseApplyPatchToolType, ResponseApproximateLocation,
    ResponseApproximateLocationType, ResponseCodeInterpreterContainer, ResponseCodeInterpreterTool,
    ResponseCodeInterpreterToolAuto, ResponseCodeInterpreterToolAutoType,
    ResponseCodeInterpreterToolType, ResponseComputerEnvironment, ResponseComputerTool,
    ResponseComputerToolType, ResponseFormatTextJsonSchemaConfig,
    ResponseFormatTextJsonSchemaConfigType, ResponseFunctionShellTool,
    ResponseFunctionShellToolType, ResponseFunctionTool, ResponseFunctionToolType, ResponseInput,
    ResponseInputItem, ResponseInputMessage, ResponseInputMessageContent, ResponseInputMessageRole,
    ResponseInputMessageType, ResponseMcpAllowedTools, ResponseMcpTool, ResponseMcpToolType,
    ResponseReasoning, ResponseReasoningEffort, ResponseTextConfig, ResponseTextFormatConfig,
    ResponseTextVerbosity, ResponseTool, ResponseToolChoice, ResponseToolChoiceFunction,
    ResponseToolChoiceFunctionType, ResponseToolChoiceOptions, ResponseTruncation,
    ResponseWebSearchFilters, ResponseWebSearchTool, ResponseWebSearchToolType,
};
use crate::openai::create_response::request::{
    OpenAiCreateResponseRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::openai::create_response::types::{
    Metadata, ResponseContextManagementEntry, ResponseContextManagementType, ResponseServiceTier,
};
use crate::transform::claude::generate_content::utils::{
    beta_system_prompt_to_text, claude_model_to_string,
};
use crate::transform::utils::TransformError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

fn tool_input_schema_to_json_object(
    input_schema: BetaToolInputSchema,
) -> std::collections::BTreeMap<String, Value> {
    let mut parameters = std::collections::BTreeMap::new();
    let schema_type = match input_schema.type_ {
        BetaToolInputSchemaType::Object => "object",
    };
    parameters.insert("type".to_string(), Value::String(schema_type.to_string()));
    if let Some(properties) = input_schema.properties {
        let properties_object = properties.into_iter().collect::<Map<String, Value>>();
        parameters.insert("properties".to_string(), Value::Object(properties_object));
    }
    if let Some(required) = input_schema.required {
        parameters.insert(
            "required".to_string(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }
    parameters
}

use crate::claude::count_tokens::types as ct;
use crate::openai::count_tokens::types as ot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudeToolKind {
    Function,
    Custom,
    Mcp,
    CodeInterpreter,
    Computer,
    WebSearch,
    WebFetch,
    Shell,
    ApplyPatch,
    FileSearch,
}

#[derive(Debug, Clone, Copy)]
struct RecordedToolCall {
    item_index: usize,
    kind: ClaudeToolKind,
}

fn json_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn input_text_content(text: String) -> ot::ResponseInputContent {
    ot::ResponseInputContent::Text(ot::ResponseInputText {
        text,
        type_: ot::ResponseInputTextType::InputText,
    })
}

fn flush_input_parts(
    input_items: &mut Vec<ResponseInputItem>,
    role: ResponseInputMessageRole,
    parts: &mut Vec<ot::ResponseInputContent>,
) {
    if parts.is_empty() {
        return;
    }

    let content = if parts.len() == 1 {
        match parts.pop() {
            Some(ot::ResponseInputContent::Text(text_part)) => {
                ResponseInputMessageContent::Text(text_part.text)
            }
            Some(part) => ResponseInputMessageContent::List(vec![part]),
            None => ResponseInputMessageContent::Text(String::new()),
        }
    } else {
        ResponseInputMessageContent::List(std::mem::take(parts))
    };

    input_items.push(ResponseInputItem::Message(ResponseInputMessage {
        content,
        role,
        phase: None,
        status: None,
        type_: Some(ResponseInputMessageType::Message),
    }));
}

fn output_message_item(id: String, text: String) -> ResponseInputItem {
    ResponseInputItem::OutputMessage(ot::ResponseOutputMessage {
        id,
        content: vec![ot::ResponseOutputContent::Text(ot::ResponseOutputText {
            annotations: Vec::new(),
            logprobs: None,
            text,
            type_: ot::ResponseOutputTextType::OutputText,
        })],
        role: ot::ResponseOutputMessageRole::Assistant,
        phase: None,
        status: Some(ot::ResponseItemStatus::Completed),
        type_: Some(ot::ResponseOutputMessageType::Message),
    })
}

fn image_media_type(media_type: ct::BetaImageMediaType) -> &'static str {
    match media_type {
        ct::BetaImageMediaType::ImageJpeg => "image/jpeg",
        ct::BetaImageMediaType::ImagePng => "image/png",
        ct::BetaImageMediaType::ImageGif => "image/gif",
        ct::BetaImageMediaType::ImageWebp => "image/webp",
    }
}

fn image_block_to_input_content(
    block: ct::BetaImageBlockParam,
) -> Option<ot::ResponseInputContent> {
    match block.source {
        ct::BetaImageSource::Base64(source) => {
            Some(ot::ResponseInputContent::Image(ot::ResponseInputImage {
                detail: None,
                type_: ot::ResponseInputImageType::InputImage,
                file_id: None,
                image_url: Some(format!(
                    "data:{};base64,{}",
                    image_media_type(source.media_type),
                    source.data
                )),
            }))
        }
        ct::BetaImageSource::Url(source) => {
            Some(ot::ResponseInputContent::Image(ot::ResponseInputImage {
                detail: None,
                type_: ot::ResponseInputImageType::InputImage,
                file_id: None,
                image_url: Some(source.url),
            }))
        }
        ct::BetaImageSource::File(source) => {
            Some(ot::ResponseInputContent::Image(ot::ResponseInputImage {
                detail: None,
                type_: ot::ResponseInputImageType::InputImage,
                file_id: Some(source.file_id),
                image_url: None,
            }))
        }
    }
}

fn document_block_to_input_content(
    block: ct::BetaRequestDocumentBlock,
) -> Option<ot::ResponseInputContent> {
    let filename = block.title;
    match block.source {
        ct::BetaDocumentSource::Base64Pdf(source) => {
            Some(ot::ResponseInputContent::File(ot::ResponseInputFile {
                type_: ot::ResponseInputFileType::InputFile,
                detail: None,
                file_data: Some(source.data),
                file_id: None,
                file_url: None,
                filename,
            }))
        }
        ct::BetaDocumentSource::PlainText(source) => {
            Some(ot::ResponseInputContent::File(ot::ResponseInputFile {
                type_: ot::ResponseInputFileType::InputFile,
                detail: None,
                file_data: Some(source.data),
                file_id: None,
                file_url: None,
                filename,
            }))
        }
        ct::BetaDocumentSource::UrlPdf(source) => {
            Some(ot::ResponseInputContent::File(ot::ResponseInputFile {
                type_: ot::ResponseInputFileType::InputFile,
                detail: None,
                file_data: None,
                file_id: None,
                file_url: Some(source.url),
                filename,
            }))
        }
        ct::BetaDocumentSource::File(source) => {
            Some(ot::ResponseInputContent::File(ot::ResponseInputFile {
                type_: ot::ResponseInputFileType::InputFile,
                detail: None,
                file_data: None,
                file_id: Some(source.file_id),
                file_url: None,
                filename,
            }))
        }
        ct::BetaDocumentSource::Content(source) => {
            let text = match source.content {
                ct::BetaContentBlockSourceContentPayload::Text(text) => text,
                ct::BetaContentBlockSourceContentPayload::Blocks(parts) => parts
                    .into_iter()
                    .filter_map(|part| match part {
                        ct::BetaContentBlockSourceContent::Text(text) => Some(text.text),
                        ct::BetaContentBlockSourceContent::Image(_) => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            if text.is_empty() {
                None
            } else {
                Some(ot::ResponseInputContent::File(ot::ResponseInputFile {
                    type_: ot::ResponseInputFileType::InputFile,
                    detail: None,
                    file_data: Some(text),
                    file_id: None,
                    file_url: None,
                    filename,
                }))
            }
        }
    }
}

fn user_message_part_from_block(
    block: ct::BetaContentBlockParam,
) -> Option<ot::ResponseInputContent> {
    match block {
        ct::BetaContentBlockParam::Text(block) => Some(input_text_content(block.text)),
        ct::BetaContentBlockParam::Image(block) => image_block_to_input_content(block),
        ct::BetaContentBlockParam::RequestDocument(block) => document_block_to_input_content(block),
        ct::BetaContentBlockParam::SearchResult(block) => {
            let text = block
                .content
                .into_iter()
                .map(|entry| entry.text)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            let summary = if text.is_empty() {
                format!("{}\n{}", block.title, block.source)
            } else {
                format!("{}\n{}\n{}", block.title, block.source, text)
            };
            Some(input_text_content(summary))
        }
        ct::BetaContentBlockParam::ContainerUpload(block) => {
            Some(ot::ResponseInputContent::File(ot::ResponseInputFile {
                type_: ot::ResponseInputFileType::InputFile,
                detail: None,
                file_data: None,
                file_id: Some(block.file_id),
                file_url: None,
                filename: None,
            }))
        }
        _ => None,
    }
}

fn tool_result_block_to_text(block: ct::BetaToolResultContentBlockParam) -> String {
    match block {
        ct::BetaToolResultContentBlockParam::Text(part) => part.text,
        ct::BetaToolResultContentBlockParam::Image(part) => match part.source {
            ct::BetaImageSource::Base64(source) => {
                format!(
                    "data:{};base64,{}",
                    image_media_type(source.media_type),
                    source.data
                )
            }
            ct::BetaImageSource::Url(source) => source.url,
            ct::BetaImageSource::File(source) => format!("file_id:{}", source.file_id),
        },
        ct::BetaToolResultContentBlockParam::SearchResult(part) => {
            let content = part
                .content
                .into_iter()
                .map(|entry| entry.text)
                .collect::<Vec<_>>()
                .join("\n");
            if content.is_empty() {
                format!("{}\n{}", part.title, part.source)
            } else {
                format!("{}\n{}\n{}", part.title, part.source, content)
            }
        }
        ct::BetaToolResultContentBlockParam::Document(part) => {
            document_block_to_input_content(part)
                .and_then(|content| match content {
                    ot::ResponseInputContent::File(file) => file
                        .file_url
                        .or(file.file_id)
                        .or(file.filename)
                        .or(file.file_data),
                    _ => None,
                })
                .unwrap_or_default()
        }
        ct::BetaToolResultContentBlockParam::ToolReference(part) => part.tool_name,
    }
}

fn tool_result_blocks_to_text(parts: Vec<ct::BetaToolResultContentBlockParam>) -> String {
    parts
        .into_iter()
        .map(tool_result_block_to_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_result_blocks_to_input_contents(
    parts: Vec<ct::BetaToolResultContentBlockParam>,
) -> Option<Vec<ot::ResponseInputContent>> {
    let mut converted = Vec::new();
    for part in parts {
        match part {
            ct::BetaToolResultContentBlockParam::Text(part) => {
                converted.push(input_text_content(part.text));
            }
            ct::BetaToolResultContentBlockParam::Image(part) => {
                converted.push(image_block_to_input_content(part)?);
            }
            ct::BetaToolResultContentBlockParam::Document(part) => {
                converted.push(document_block_to_input_content(part)?);
            }
            ct::BetaToolResultContentBlockParam::SearchResult(_)
            | ct::BetaToolResultContentBlockParam::ToolReference(_) => return None,
        }
    }
    Some(converted)
}

fn tool_result_content_to_text(content: Option<ct::BetaToolResultBlockParamContent>) -> String {
    match content {
        Some(ct::BetaToolResultBlockParamContent::Text(text)) => text,
        Some(ct::BetaToolResultBlockParamContent::Blocks(parts)) => {
            tool_result_blocks_to_text(parts)
        }
        None => String::new(),
    }
}

fn tool_result_content_to_function_output(
    content: Option<ct::BetaToolResultBlockParamContent>,
) -> ot::ResponseFunctionCallOutputContent {
    match content {
        Some(ct::BetaToolResultBlockParamContent::Text(text)) => {
            ot::ResponseFunctionCallOutputContent::Text(text)
        }
        Some(ct::BetaToolResultBlockParamContent::Blocks(parts)) => {
            if let Some(contents) = tool_result_blocks_to_input_contents(parts.clone()) {
                ot::ResponseFunctionCallOutputContent::Content(contents)
            } else {
                ot::ResponseFunctionCallOutputContent::Text(tool_result_blocks_to_text(parts))
            }
        }
        None => ot::ResponseFunctionCallOutputContent::Text(String::new()),
    }
}

fn tool_result_content_to_custom_output(
    content: Option<ct::BetaToolResultBlockParamContent>,
) -> ot::ResponseCustomToolCallOutputContent {
    match content {
        Some(ct::BetaToolResultBlockParamContent::Text(text)) => {
            ot::ResponseCustomToolCallOutputContent::Text(text)
        }
        Some(ct::BetaToolResultBlockParamContent::Blocks(parts)) => {
            if let Some(contents) = tool_result_blocks_to_input_contents(parts.clone()) {
                ot::ResponseCustomToolCallOutputContent::Content(contents)
            } else {
                ot::ResponseCustomToolCallOutputContent::Text(tool_result_blocks_to_text(parts))
            }
        }
        None => ot::ResponseCustomToolCallOutputContent::Text(String::new()),
    }
}

fn mcp_result_text(content: Option<ct::BetaMcpToolResultBlockParamContent>) -> String {
    match content {
        Some(ct::BetaMcpToolResultBlockParamContent::Text(text)) => text,
        Some(ct::BetaMcpToolResultBlockParamContent::Blocks(parts)) => parts
            .into_iter()
            .map(|part| part.text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        None => String::new(),
    }
}

fn shell_output_text(stdout: String, stderr: String) -> String {
    if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("stdout: {stdout}\nstderr: {stderr}")
    }
}

fn string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect(),
        Some(serde_json::Value::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn f64_field(input: &ct::JsonObject, key: &str) -> Option<f64> {
    input.get(key).and_then(|value| value.as_f64())
}

fn str_field<'a>(input: &'a ct::JsonObject, key: &str) -> Option<&'a str> {
    input.get(key).and_then(|value| value.as_str())
}

fn computer_mouse_button(input: &ct::JsonObject) -> ot::ResponseComputerMouseButton {
    match str_field(input, "button").unwrap_or("left") {
        "right" => ot::ResponseComputerMouseButton::Right,
        "wheel" => ot::ResponseComputerMouseButton::Wheel,
        "back" => ot::ResponseComputerMouseButton::Back,
        "forward" => ot::ResponseComputerMouseButton::Forward,
        _ => ot::ResponseComputerMouseButton::Left,
    }
}

fn computer_action_from_input(input: &ct::JsonObject) -> Option<ot::ResponseComputerAction> {
    let action = str_field(input, "action")
        .or_else(|| str_field(input, "type"))?
        .to_string();

    match action.as_str() {
        "click" => Some(ot::ResponseComputerAction::Click {
            button: computer_mouse_button(input),
            x: f64_field(input, "x")?,
            y: f64_field(input, "y")?,
        }),
        "double_click" => Some(ot::ResponseComputerAction::DoubleClick {
            x: f64_field(input, "x")?,
            y: f64_field(input, "y")?,
        }),
        "drag" => {
            let path = input
                .get("path")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| {
                            let x = value.get("x")?.as_f64()?;
                            let y = value.get("y")?.as_f64()?;
                            Some(ot::ResponseComputerPoint { x, y })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if path.is_empty() {
                None
            } else {
                Some(ot::ResponseComputerAction::Drag { path })
            }
        }
        "keypress" => {
            let mut keys = string_list(input.get("keys"));
            if keys.is_empty()
                && let Some(key) = str_field(input, "key")
            {
                keys.push(key.to_string());
            }
            if keys.is_empty() {
                None
            } else {
                Some(ot::ResponseComputerAction::Keypress { keys })
            }
        }
        "move" => Some(ot::ResponseComputerAction::Move {
            x: f64_field(input, "x")?,
            y: f64_field(input, "y")?,
        }),
        "screenshot" => Some(ot::ResponseComputerAction::Screenshot),
        "scroll" => Some(ot::ResponseComputerAction::Scroll {
            scroll_x: f64_field(input, "scroll_x")
                .or_else(|| f64_field(input, "delta_x"))
                .unwrap_or_default(),
            scroll_y: f64_field(input, "scroll_y")
                .or_else(|| f64_field(input, "delta_y"))
                .unwrap_or_default(),
            x: f64_field(input, "x").unwrap_or_default(),
            y: f64_field(input, "y").unwrap_or_default(),
        }),
        "type" => Some(ot::ResponseComputerAction::Type {
            text: str_field(input, "text")?.to_string(),
        }),
        "wait" => Some(ot::ResponseComputerAction::Wait),
        _ => None,
    }
}

fn computer_screenshot_from_tool_result_content(
    content: Option<ct::BetaToolResultBlockParamContent>,
) -> Option<ot::ResponseComputerToolCallOutputScreenshot> {
    let ct::BetaToolResultBlockParamContent::Blocks(parts) = content? else {
        return None;
    };

    for part in parts {
        let ct::BetaToolResultContentBlockParam::Image(image) = part else {
            continue;
        };
        return match image.source {
            ct::BetaImageSource::Base64(source) => {
                Some(ot::ResponseComputerToolCallOutputScreenshot {
                    type_: ot::ResponseComputerToolCallOutputScreenshotType::ComputerScreenshot,
                    file_id: None,
                    image_url: Some(format!(
                        "data:{};base64,{}",
                        image_media_type(source.media_type),
                        source.data
                    )),
                })
            }
            ct::BetaImageSource::Url(source) => {
                Some(ot::ResponseComputerToolCallOutputScreenshot {
                    type_: ot::ResponseComputerToolCallOutputScreenshotType::ComputerScreenshot,
                    file_id: None,
                    image_url: Some(source.url),
                })
            }
            ct::BetaImageSource::File(source) => {
                Some(ot::ResponseComputerToolCallOutputScreenshot {
                    type_: ot::ResponseComputerToolCallOutputScreenshotType::ComputerScreenshot,
                    file_id: Some(source.file_id),
                    image_url: None,
                })
            }
        };
    }

    None
}

fn file_search_queries(input: &ct::JsonObject) -> Vec<String> {
    let mut queries = string_list(input.get("queries"));
    if queries.is_empty() {
        for key in ["query", "pattern", "term"] {
            if let Some(value) = str_field(input, key) {
                queries.push(value.to_string());
                break;
            }
        }
    }
    queries
}

fn shell_commands(input: &ct::JsonObject) -> Vec<String> {
    let mut commands = string_list(input.get("commands"));
    if commands.is_empty() {
        for key in ["command", "cmd"] {
            if let Some(value) = str_field(input, key) {
                commands.push(value.to_string());
                break;
            }
        }
    }
    commands
}

fn apply_claude_tool_choice(
    tool_choice: Option<BetaToolChoice>,
    tool_registry: &BTreeMap<String, ClaudeToolKind>,
) -> Option<ResponseToolChoice> {
    match tool_choice {
        Some(BetaToolChoice::Auto(_)) => {
            Some(ResponseToolChoice::Options(ResponseToolChoiceOptions::Auto))
        }
        Some(BetaToolChoice::Any(_)) => Some(ResponseToolChoice::Options(
            ResponseToolChoiceOptions::Required,
        )),
        Some(BetaToolChoice::None(_)) => {
            Some(ResponseToolChoice::Options(ResponseToolChoiceOptions::None))
        }
        Some(BetaToolChoice::Tool(choice)) => match tool_registry.get(&choice.name) {
            Some(ClaudeToolKind::Custom) => {
                Some(ResponseToolChoice::Custom(ot::ResponseToolChoiceCustom {
                    name: choice.name,
                    type_: ot::ResponseToolChoiceCustomType::Custom,
                }))
            }
            Some(ClaudeToolKind::Mcp) => Some(ResponseToolChoice::Mcp(ot::ResponseToolChoiceMcp {
                server_label: choice.name,
                type_: ot::ResponseToolChoiceMcpType::Mcp,
                name: None,
            })),
            Some(ClaudeToolKind::ApplyPatch) => Some(ResponseToolChoice::ApplyPatch(
                ot::ResponseToolChoiceApplyPatch {
                    type_: ot::ResponseToolChoiceApplyPatchType::ApplyPatch,
                },
            )),
            Some(ClaudeToolKind::Shell) => {
                Some(ResponseToolChoice::Shell(ot::ResponseToolChoiceShell {
                    type_: ot::ResponseToolChoiceShellType::Shell,
                }))
            }
            Some(ClaudeToolKind::FileSearch) => {
                Some(ResponseToolChoice::Types(ot::ResponseToolChoiceTypes {
                    type_: ot::ResponseToolChoiceBuiltinType::FileSearch,
                }))
            }
            Some(ClaudeToolKind::Computer) => {
                Some(ResponseToolChoice::Types(ot::ResponseToolChoiceTypes {
                    type_: ot::ResponseToolChoiceBuiltinType::ComputerUsePreview,
                }))
            }
            Some(ClaudeToolKind::CodeInterpreter) => {
                Some(ResponseToolChoice::Types(ot::ResponseToolChoiceTypes {
                    type_: ot::ResponseToolChoiceBuiltinType::CodeInterpreter,
                }))
            }
            Some(ClaudeToolKind::WebSearch | ClaudeToolKind::WebFetch) => {
                Some(ResponseToolChoice::Types(ot::ResponseToolChoiceTypes {
                    type_: ot::ResponseToolChoiceBuiltinType::WebSearchPreview,
                }))
            }
            _ => Some(ResponseToolChoice::Function(ResponseToolChoiceFunction {
                name: choice.name,
                type_: ResponseToolChoiceFunctionType::Function,
            })),
        },
        None => None,
    }
}

impl TryFrom<ClaudeCreateMessageRequest> for OpenAiCreateResponseRequest {
    type Error = TransformError;

    fn try_from(value: ClaudeCreateMessageRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let model = claude_model_to_string(&body.model);

        let instructions = beta_system_prompt_to_text(body.system.clone());
        let parallel_tool_calls = match body.tool_choice.as_ref() {
            Some(BetaToolChoice::Auto(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
            Some(BetaToolChoice::Any(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
            Some(BetaToolChoice::Tool(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
            Some(BetaToolChoice::None(_)) | None => None,
        };

        let mut tool_registry = BTreeMap::new();
        if let Some(tools) = body.tools.as_ref() {
            for tool in tools {
                match tool {
                    BetaToolUnion::Custom(tool) => {
                        tool_registry.insert(
                            tool.name.clone(),
                            if matches!(tool.type_, Some(ct::BetaCustomToolType::Custom)) {
                                ClaudeToolKind::Custom
                            } else {
                                ClaudeToolKind::Function
                            },
                        );
                    }
                    BetaToolUnion::CodeExecution20250522(_)
                    | BetaToolUnion::CodeExecution20250825(_) => {
                        tool_registry.insert(
                            "code_execution".to_string(),
                            ClaudeToolKind::CodeInterpreter,
                        );
                    }
                    BetaToolUnion::ComputerUse20241022(_)
                    | BetaToolUnion::ComputerUse20250124(_)
                    | BetaToolUnion::ComputerUse20251124(_) => {
                        tool_registry.insert("computer".to_string(), ClaudeToolKind::Computer);
                    }
                    BetaToolUnion::WebSearch20250305(_) => {
                        tool_registry.insert("web_search".to_string(), ClaudeToolKind::WebSearch);
                    }
                    BetaToolUnion::WebFetch20250910(_) => {
                        tool_registry.insert("web_fetch".to_string(), ClaudeToolKind::WebFetch);
                    }
                    BetaToolUnion::Bash20241022(_) | BetaToolUnion::Bash20250124(_) => {
                        tool_registry.insert("bash".to_string(), ClaudeToolKind::Shell);
                    }
                    BetaToolUnion::ToolSearchBm25_20251119(_) => {
                        tool_registry.insert(
                            "tool_search_tool_bm25".to_string(),
                            ClaudeToolKind::FileSearch,
                        );
                    }
                    BetaToolUnion::ToolSearchRegex20251119(_) => {
                        tool_registry.insert(
                            "tool_search_tool_regex".to_string(),
                            ClaudeToolKind::FileSearch,
                        );
                    }
                    BetaToolUnion::TextEditor20241022(_) => {
                        tool_registry
                            .insert("str_replace_editor".to_string(), ClaudeToolKind::ApplyPatch);
                    }
                    BetaToolUnion::TextEditor20250124(_)
                    | BetaToolUnion::TextEditor20250429(_)
                    | BetaToolUnion::TextEditor20250728(_) => {
                        tool_registry.insert(
                            "str_replace_based_edit_tool".to_string(),
                            ClaudeToolKind::ApplyPatch,
                        );
                    }
                    BetaToolUnion::McpToolset(tool) => {
                        tool_registry.insert(tool.mcp_server_name.clone(), ClaudeToolKind::Mcp);
                    }
                    BetaToolUnion::Memory20250818(_) => {}
                }
            }
        }
        if let Some(servers) = body.mcp_servers.as_ref() {
            for server in servers {
                tool_registry.insert(server.name.clone(), ClaudeToolKind::Mcp);
            }
        }
        let tool_choice = apply_claude_tool_choice(body.tool_choice.clone(), &tool_registry);

        let reasoning_effort_from_thinking = match body.thinking.clone() {
            Some(BetaThinkingConfigParam::Enabled(config)) => Some(if config.budget_tokens == 0 {
                ResponseReasoningEffort::None
            } else if config.budget_tokens <= 4096 {
                ResponseReasoningEffort::Minimal
            } else if config.budget_tokens <= 8192 {
                ResponseReasoningEffort::Low
            } else if config.budget_tokens <= 16384 {
                ResponseReasoningEffort::Medium
            } else if config.budget_tokens <= 32768 {
                ResponseReasoningEffort::High
            } else {
                ResponseReasoningEffort::XHigh
            }),
            Some(BetaThinkingConfigParam::Disabled(_)) => Some(ResponseReasoningEffort::None),
            Some(BetaThinkingConfigParam::Adaptive(_)) => Some(ResponseReasoningEffort::Medium),
            None => None,
        };
        let reasoning = reasoning_effort_from_thinking.map(|effort| ResponseReasoning {
            effort: Some(effort),
            generate_summary: None,
            summary: None,
        });
        let output_schema = body
            .output_config
            .as_ref()
            .and_then(|config| config.format.as_ref())
            .or(body.output_format.as_ref());
        let text_format = output_schema.map(|schema| {
            ResponseTextFormatConfig::JsonSchema(ResponseFormatTextJsonSchemaConfig {
                name: "output".to_string(),
                schema: schema.schema.clone(),
                type_: ResponseFormatTextJsonSchemaConfigType::JsonSchema,
                description: None,
                strict: None,
            })
        });
        let text_verbosity = body
            .output_config
            .as_ref()
            .and_then(|config| config.effort.as_ref())
            .map(|effort| match effort {
                BetaOutputEffort::Low => ResponseTextVerbosity::Low,
                BetaOutputEffort::Medium => ResponseTextVerbosity::Medium,
                BetaOutputEffort::High | BetaOutputEffort::XHigh | BetaOutputEffort::Max => {
                    ResponseTextVerbosity::High
                }
            });
        let text = if text_format.is_some() || text_verbosity.is_some() {
            Some(ResponseTextConfig {
                format: text_format,
                verbosity: text_verbosity,
            })
        } else {
            None
        };
        let context_management = body.context_management.as_ref().and_then(|config| {
            let mut entries = Vec::new();
            if let Some(edits) = config.edits.as_ref() {
                for edit in edits {
                    if let BetaContextManagementEdit::Compact(compact) = edit {
                        entries.push(ResponseContextManagementEntry {
                            type_: ResponseContextManagementType::Compaction,
                            compact_threshold: compact
                                .trigger
                                .as_ref()
                                .map(|trigger| trigger.value),
                        });
                    }
                }
            }

            if entries.is_empty() {
                None
            } else {
                Some(entries)
            }
        });
        let truncation = body
            .context_management
            .as_ref()
            .map(|_| ResponseTruncation::Auto);

        let mut input_items = Vec::new();
        let mut recorded_calls = BTreeMap::<String, RecordedToolCall>::new();
        let mut assistant_message_index = 0u64;
        let mut reasoning_index = 0u64;

        for message in body.messages {
            match (message.role, message.content) {
                (BetaMessageRole::User, ct::BetaMessageContent::Text(text)) => {
                    if !text.is_empty() {
                        input_items.push(ResponseInputItem::Message(ResponseInputMessage {
                            content: ResponseInputMessageContent::Text(text),
                            role: ResponseInputMessageRole::User,
                            phase: None,
                            status: None,
                            type_: Some(ResponseInputMessageType::Message),
                        }));
                    }
                }
                (BetaMessageRole::User, ct::BetaMessageContent::Blocks(blocks)) => {
                    let mut message_parts = Vec::new();
                    for block in blocks {
                        if let Some(part) = user_message_part_from_block(block.clone()) {
                            message_parts.push(part);
                            continue;
                        }

                        match block {
                            ct::BetaContentBlockParam::ToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                let kind = recorded_calls
                                    .get(&block.tool_use_id)
                                    .map(|record| record.kind)
                                    .unwrap_or(ClaudeToolKind::Function);
                                let is_error = block.is_error.unwrap_or(false);
                                match kind {
                                    ClaudeToolKind::Function => {
                                        input_items.push(ResponseInputItem::FunctionCallOutput(
                                            ot::ResponseFunctionCallOutput {
                                                call_id: block.tool_use_id,
                                                output: tool_result_content_to_function_output(
                                                    block.content,
                                                ),
                                                type_: ot::ResponseFunctionCallOutputType::FunctionCallOutput,
                                                id: None,
                                                status: Some(if is_error {
                                                    ot::ResponseItemStatus::Incomplete
                                                } else {
                                                    ot::ResponseItemStatus::Completed
                                                }),
                                            },
                                        ));
                                    }
                                    ClaudeToolKind::Custom | ClaudeToolKind::ApplyPatch => {
                                        input_items.push(ResponseInputItem::CustomToolCallOutput(
                                            ot::ResponseCustomToolCallOutput {
                                                call_id: block.tool_use_id,
                                                output: tool_result_content_to_custom_output(
                                                    block.content,
                                                ),
                                                type_: ot::ResponseCustomToolCallOutputType::CustomToolCallOutput,
                                                id: None,
                                            },
                                        ));
                                    }
                                    ClaudeToolKind::Mcp => {
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::McpCall(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            let text = tool_result_content_to_text(block.content);
                                            call.output = (!is_error && !text.is_empty())
                                                .then_some(text.clone());
                                            call.error = if is_error {
                                                Some(if text.is_empty() {
                                                    "mcp_tool_result_error".to_string()
                                                } else {
                                                    text
                                                })
                                            } else {
                                                None
                                            };
                                            call.status = Some(if is_error {
                                                ot::ResponseToolCallStatus::Failed
                                            } else {
                                                ot::ResponseToolCallStatus::Completed
                                            });
                                        }
                                    }
                                    ClaudeToolKind::CodeInterpreter => {
                                        let output_text =
                                            tool_result_content_to_text(block.content);
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::CodeInterpreterToolCall(
                                                call,
                                            )) = input_items.get_mut(record.item_index)
                                        {
                                            call.outputs =
                                                (!output_text.is_empty()).then_some(vec![
                                                    ot::ResponseCodeInterpreterOutputItem::Logs {
                                                        logs: output_text,
                                                    },
                                                ]);
                                            call.status = if is_error {
                                                ot::ResponseCodeInterpreterToolCallStatus::Failed
                                            } else {
                                                ot::ResponseCodeInterpreterToolCallStatus::Completed
                                            };
                                        }
                                    }
                                    ClaudeToolKind::Shell => {
                                        let output_text =
                                            tool_result_content_to_text(block.content);
                                        input_items.push(ResponseInputItem::ShellCallOutput(
                                            ot::ResponseShellCallOutput {
                                                call_id: block.tool_use_id,
                                                output: if output_text.is_empty() {
                                                    Vec::new()
                                                } else {
                                                    vec![ot::ResponseFunctionShellCallOutputContent {
                                                        outcome: ot::ResponseShellCallOutcome::Exit {
                                                            exit_code: if is_error { 1 } else { 0 },
                                                        },
                                                        stderr: if is_error {
                                                            output_text.clone()
                                                        } else {
                                                            String::new()
                                                        },
                                                        stdout: if is_error {
                                                            String::new()
                                                        } else {
                                                            output_text
                                                        },
                                                    }]
                                                },
                                                type_: ot::ResponseShellCallOutputType::ShellCallOutput,
                                                id: None,
                                                max_output_length: None,
                                                status: Some(if is_error {
                                                    ot::ResponseItemStatus::Incomplete
                                                } else {
                                                    ot::ResponseItemStatus::Completed
                                                }),
                                            },
                                        ));
                                    }
                                    ClaudeToolKind::FileSearch => {
                                        let output_text =
                                            tool_result_content_to_text(block.content);
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::FileSearchToolCall(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            call.results =
                                                (!output_text.is_empty()).then_some(vec![
                                                    ot::ResponseFileSearchResult {
                                                        text: Some(output_text),
                                                        ..Default::default()
                                                    },
                                                ]);
                                            call.status = if is_error {
                                                ot::ResponseFileSearchToolCallStatus::Failed
                                            } else {
                                                ot::ResponseFileSearchToolCallStatus::Completed
                                            };
                                        }
                                    }
                                    ClaudeToolKind::Computer => {
                                        if let Some(screenshot) =
                                            computer_screenshot_from_tool_result_content(
                                                block.content,
                                            )
                                        {
                                            input_items.push(ResponseInputItem::ComputerCallOutput(
                                                ot::ResponseComputerCallOutput {
                                                    call_id: block.tool_use_id,
                                                    output: screenshot,
                                                    type_: ot::ResponseComputerCallOutputType::ComputerCallOutput,
                                                    id: None,
                                                    acknowledged_safety_checks: None,
                                                    status: Some(if is_error {
                                                        ot::ResponseItemStatus::Incomplete
                                                    } else {
                                                        ot::ResponseItemStatus::Completed
                                                    }),
                                                },
                                            ));
                                        }
                                    }
                                    ClaudeToolKind::WebSearch | ClaudeToolKind::WebFetch => {}
                                }
                            }
                            ct::BetaContentBlockParam::McpToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                let output_text = mcp_result_text(block.content);
                                if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                    && let Some(ResponseInputItem::McpCall(call)) =
                                        input_items.get_mut(record.item_index)
                                {
                                    call.output = (!block.is_error.unwrap_or(false)
                                        && !output_text.is_empty())
                                    .then_some(output_text.clone());
                                    call.error = if block.is_error.unwrap_or(false) {
                                        Some(if output_text.is_empty() {
                                            "mcp_tool_result_error".to_string()
                                        } else {
                                            output_text
                                        })
                                    } else {
                                        None
                                    };
                                    call.status = Some(if block.is_error.unwrap_or(false) {
                                        ot::ResponseToolCallStatus::Failed
                                    } else {
                                        ot::ResponseToolCallStatus::Completed
                                    });
                                }
                            }
                            ct::BetaContentBlockParam::WebSearchToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                let status = match block.content {
                                    ct::BetaWebSearchToolResultBlockParamContent::Results(
                                        results,
                                    ) => {
                                        let sources = results
                                            .into_iter()
                                            .map(|result| ot::ResponseFunctionWebSearchSource {
                                                type_: ot::ResponseFunctionWebSearchSourceType::Url,
                                                url: result.url,
                                            })
                                            .collect::<Vec<_>>();
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::FunctionWebSearch(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            let (query, queries) = match &call.action {
                                                ot::ResponseFunctionWebSearchAction::Search {
                                                    query,
                                                    queries,
                                                    ..
                                                } => (query.clone(), queries.clone()),
                                                _ => (None, None),
                                            };
                                            call.action =
                                                ot::ResponseFunctionWebSearchAction::Search {
                                                    query,
                                                    queries,
                                                    sources: (!sources.is_empty())
                                                        .then_some(sources),
                                                };
                                            call.status =
                                                ot::ResponseFunctionWebSearchStatus::Completed;
                                        }
                                        ot::ResponseFunctionWebSearchStatus::Completed
                                    }
                                    ct::BetaWebSearchToolResultBlockParamContent::Error(_) => {
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::FunctionWebSearch(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            call.status =
                                                ot::ResponseFunctionWebSearchStatus::Failed;
                                        }
                                        ot::ResponseFunctionWebSearchStatus::Failed
                                    }
                                };
                                if !recorded_calls.contains_key(&block.tool_use_id) {
                                    input_items.push(ResponseInputItem::FunctionWebSearch(
                                        ot::ResponseFunctionWebSearch {
                                            id: Some(block.tool_use_id),
                                            action: ot::ResponseFunctionWebSearchAction::Search {
                                                query: None,
                                                queries: None,
                                                sources: None,
                                            },
                                            status,
                                            type_: ot::ResponseFunctionWebSearchType::WebSearchCall,
                                        },
                                    ));
                                }
                            }
                            ct::BetaContentBlockParam::WebFetchToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                match block.content {
                                    ct::BetaWebFetchToolResultBlockParamContent::Result(result) => {
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::FunctionWebSearch(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            call.action =
                                                ot::ResponseFunctionWebSearchAction::OpenPage {
                                                    url: Some(result.url.clone()),
                                                };
                                            call.status =
                                                ot::ResponseFunctionWebSearchStatus::Completed;
                                        } else {
                                            input_items.push(ResponseInputItem::FunctionWebSearch(
                                                ot::ResponseFunctionWebSearch {
                                                    id: Some(block.tool_use_id),
                                                    action: ot::ResponseFunctionWebSearchAction::OpenPage {
                                                        url: Some(result.url),
                                                    },
                                                    status: ot::ResponseFunctionWebSearchStatus::Completed,
                                                    type_: ot::ResponseFunctionWebSearchType::WebSearchCall,
                                                },
                                            ));
                                        }
                                    }
                                    ct::BetaWebFetchToolResultBlockParamContent::Error(_) => {
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::FunctionWebSearch(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            call.status =
                                                ot::ResponseFunctionWebSearchStatus::Failed;
                                        } else {
                                            input_items.push(ResponseInputItem::FunctionWebSearch(
                                                ot::ResponseFunctionWebSearch {
                                                    id: Some(block.tool_use_id),
                                                    action: ot::ResponseFunctionWebSearchAction::OpenPage {
                                                        url: None,
                                                    },
                                                    status: ot::ResponseFunctionWebSearchStatus::Failed,
                                                    type_: ot::ResponseFunctionWebSearchType::WebSearchCall,
                                                },
                                            ));
                                        }
                                    }
                                }
                            }
                            ct::BetaContentBlockParam::CodeExecutionToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                let (output_text, status) = match block.content {
                                    ct::BetaCodeExecutionToolResultBlockParamContent::Result(
                                        result,
                                    ) => (
                                        shell_output_text(result.stdout, result.stderr),
                                        ot::ResponseCodeInterpreterToolCallStatus::Completed,
                                    ),
                                    ct::BetaCodeExecutionToolResultBlockParamContent::Error(
                                        err,
                                    ) => (
                                        format!("code_execution_error:{:?}", err.error_code),
                                        ot::ResponseCodeInterpreterToolCallStatus::Failed,
                                    ),
                                };
                                if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                    && let Some(ResponseInputItem::CodeInterpreterToolCall(call)) =
                                        input_items.get_mut(record.item_index)
                                {
                                    call.outputs = (!output_text.is_empty()).then_some(vec![
                                        ot::ResponseCodeInterpreterOutputItem::Logs {
                                            logs: output_text,
                                        },
                                    ]);
                                    call.status = status;
                                } else {
                                    input_items.push(ResponseInputItem::CodeInterpreterToolCall(
                                        ot::ResponseCodeInterpreterToolCall {
                                            id: block.tool_use_id,
                                            code: String::new(),
                                            container_id: String::new(),
                                            outputs: (!output_text.is_empty()).then_some(vec![
                                                ot::ResponseCodeInterpreterOutputItem::Logs {
                                                    logs: output_text,
                                                },
                                            ]),
                                            status,
                                            type_: ot::ResponseCodeInterpreterToolCallType::CodeInterpreterCall,
                                        },
                                    ));
                                }
                            }
                            ct::BetaContentBlockParam::BashCodeExecutionToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                let (stdout, stderr, outcome) = match block.content {
                                    ct::BetaBashCodeExecutionToolResultBlockParamContent::Result(result) => (
                                        result.stdout,
                                        result.stderr,
                                        ot::ResponseShellCallOutcome::Exit { exit_code: 0 },
                                    ),
                                    ct::BetaBashCodeExecutionToolResultBlockParamContent::Error(err) => (
                                        String::new(),
                                        format!("bash_code_execution_error:{:?}", err.error_code),
                                        if matches!(
                                            err.error_code,
                                            ct::BetaBashCodeExecutionToolResultErrorCode::ExecutionTimeExceeded
                                        ) {
                                            ot::ResponseShellCallOutcome::Timeout
                                        } else {
                                            ot::ResponseShellCallOutcome::Exit { exit_code: 1 }
                                        },
                                    ),
                                };
                                input_items.push(ResponseInputItem::ShellCallOutput(
                                    ot::ResponseShellCallOutput {
                                        call_id: block.tool_use_id,
                                        output: vec![ot::ResponseFunctionShellCallOutputContent {
                                            outcome,
                                            stderr,
                                            stdout,
                                        }],
                                        type_: ot::ResponseShellCallOutputType::ShellCallOutput,
                                        id: None,
                                        max_output_length: None,
                                        status: Some(ot::ResponseItemStatus::Completed),
                                    },
                                ));
                            }
                            ct::BetaContentBlockParam::TextEditorCodeExecutionToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                let output = match block.content {
                                    ct::BetaTextEditorCodeExecutionToolResultBlockParamContent::View(view) => view.content,
                                    ct::BetaTextEditorCodeExecutionToolResultBlockParamContent::Create(create) => {
                                        format!("file_updated:{}", create.is_file_update)
                                    }
                                    ct::BetaTextEditorCodeExecutionToolResultBlockParamContent::StrReplace(replace) => {
                                        replace.lines.unwrap_or_default().join("\n")
                                    }
                                    ct::BetaTextEditorCodeExecutionToolResultBlockParamContent::Error(err) => err
                                        .error_message
                                        .unwrap_or_else(|| {
                                            format!(
                                                "text_editor_code_execution_error:{:?}",
                                                err.error_code
                                            )
                                        }),
                                };
                                input_items.push(ResponseInputItem::CustomToolCallOutput(
                                    ot::ResponseCustomToolCallOutput {
                                        call_id: block.tool_use_id,
                                        output: ot::ResponseCustomToolCallOutputContent::Text(output),
                                        type_: ot::ResponseCustomToolCallOutputType::CustomToolCallOutput,
                                        id: None,
                                    },
                                ));
                            }
                            ct::BetaContentBlockParam::ToolSearchToolResult(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                match block.content {
                                    ct::BetaToolSearchToolResultBlockParamContent::Result(
                                        result,
                                    ) => {
                                        let results = result
                                            .tool_references
                                            .into_iter()
                                            .map(|reference| ot::ResponseFileSearchResult {
                                                filename: Some(reference.tool_name.clone()),
                                                text: Some(reference.tool_name),
                                                ..Default::default()
                                            })
                                            .collect::<Vec<_>>();
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::FileSearchToolCall(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            call.results = Some(results);
                                            call.status =
                                                ot::ResponseFileSearchToolCallStatus::Completed;
                                        } else {
                                            input_items.push(ResponseInputItem::FileSearchToolCall(
                                                ot::ResponseFileSearchToolCall {
                                                    id: block.tool_use_id,
                                                    queries: Vec::new(),
                                                    status: ot::ResponseFileSearchToolCallStatus::Completed,
                                                    type_: ot::ResponseFileSearchToolCallType::FileSearchCall,
                                                    results: Some(results),
                                                },
                                            ));
                                        }
                                    }
                                    ct::BetaToolSearchToolResultBlockParamContent::Error(err) => {
                                        if let Some(record) = recorded_calls.get(&block.tool_use_id)
                                            && let Some(ResponseInputItem::FileSearchToolCall(call)) =
                                                input_items.get_mut(record.item_index)
                                        {
                                            call.status =
                                                ot::ResponseFileSearchToolCallStatus::Failed;
                                            call.results =
                                                Some(vec![ot::ResponseFileSearchResult {
                                                    text: Some(format!(
                                                        "tool_search_error:{:?}",
                                                        err.error_code
                                                    )),
                                                    ..Default::default()
                                                }]);
                                        } else {
                                            input_items.push(ResponseInputItem::FileSearchToolCall(
                                                ot::ResponseFileSearchToolCall {
                                                    id: block.tool_use_id,
                                                    queries: Vec::new(),
                                                    status: ot::ResponseFileSearchToolCallStatus::Failed,
                                                    type_: ot::ResponseFileSearchToolCallType::FileSearchCall,
                                                    results: Some(vec![ot::ResponseFileSearchResult {
                                                        text: Some(format!(
                                                            "tool_search_error:{:?}",
                                                            err.error_code
                                                        )),
                                                        ..Default::default()
                                                    }]),
                                                },
                                            ));
                                        }
                                    }
                                }
                            }
                            ct::BetaContentBlockParam::Compaction(block) => {
                                flush_input_parts(
                                    &mut input_items,
                                    ResponseInputMessageRole::User,
                                    &mut message_parts,
                                );
                                input_items.push(ResponseInputItem::CompactionItem(
                                    ot::ResponseCompactionItemParam {
                                        encrypted_content: block.content.unwrap_or_default(),
                                        type_: ot::ResponseCompactionItemType::Compaction,
                                        id: None,
                                        created_by: None,
                                    },
                                ));
                            }
                            other => {
                                message_parts.push(input_text_content(json_string(&other)));
                            }
                        }
                    }
                    flush_input_parts(
                        &mut input_items,
                        ResponseInputMessageRole::User,
                        &mut message_parts,
                    );
                }
                (BetaMessageRole::Assistant, ct::BetaMessageContent::Text(text)) => {
                    if !text.is_empty() {
                        input_items.push(output_message_item(
                            format!("msg_{assistant_message_index}"),
                            text,
                        ));
                        assistant_message_index += 1;
                    }
                }
                (BetaMessageRole::Assistant, ct::BetaMessageContent::Blocks(blocks)) => {
                    for block in blocks {
                        match block {
                            ct::BetaContentBlockParam::Text(block) => {
                                if !block.text.is_empty() {
                                    input_items.push(output_message_item(
                                        format!("msg_{assistant_message_index}"),
                                        block.text,
                                    ));
                                    assistant_message_index += 1;
                                }
                            }
                            ct::BetaContentBlockParam::Thinking(block) => {
                                input_items.push(ResponseInputItem::ReasoningItem(
                                    ot::ResponseReasoningItem {
                                        id: Some(block.signature),
                                        summary: vec![ot::ResponseSummaryTextContent {
                                            text: block.thinking,
                                            type_: ot::ResponseSummaryTextContentType::SummaryText,
                                        }],
                                        type_: ot::ResponseReasoningItemType::Reasoning,
                                        content: None,
                                        encrypted_content: None,
                                        status: Some(ot::ResponseItemStatus::Completed),
                                    },
                                ));
                            }
                            ct::BetaContentBlockParam::RedactedThinking(block) => {
                                input_items.push(ResponseInputItem::ReasoningItem(
                                    ot::ResponseReasoningItem {
                                        id: Some(format!("redacted_reasoning_{reasoning_index}")),
                                        summary: Vec::new(),
                                        type_: ot::ResponseReasoningItemType::Reasoning,
                                        content: None,
                                        encrypted_content: Some(block.data),
                                        status: Some(ot::ResponseItemStatus::Completed),
                                    },
                                ));
                                reasoning_index += 1;
                            }
                            ct::BetaContentBlockParam::ToolUse(block) => {
                                let tool_name = block.name.clone();
                                let call_id = block.id.clone();
                                let input_json = json_string(&block.input);
                                let mut actual_kind = tool_registry
                                    .get(&tool_name)
                                    .copied()
                                    .unwrap_or(ClaudeToolKind::Function);
                                let item = match actual_kind {
                                    ClaudeToolKind::Function => ResponseInputItem::FunctionToolCall(
                                        ot::ResponseFunctionToolCall {
                                            arguments: input_json,
                                            call_id: call_id.clone(),
                                            name: tool_name,
                                            type_: ot::ResponseFunctionToolCallType::FunctionCall,
                                            id: Some(call_id.clone()),
                                            status: Some(ot::ResponseItemStatus::Completed),
                                        },
                                    ),
                                    ClaudeToolKind::Custom | ClaudeToolKind::ApplyPatch => {
                                        actual_kind = ClaudeToolKind::Custom;
                                        ResponseInputItem::CustomToolCall(ot::ResponseCustomToolCall {
                                            call_id: call_id.clone(),
                                            input: input_json,
                                            name: tool_name,
                                            type_: ot::ResponseCustomToolCallType::CustomToolCall,
                                            id: Some(call_id.clone()),
                                        })
                                    }
                                    ClaudeToolKind::Computer => {
                                        if let Some(action) = computer_action_from_input(&block.input) {
                                            ResponseInputItem::ComputerToolCall(
                                                ot::ResponseComputerToolCall {
                                                    id: call_id.clone(),
                                                    action,
                                                    call_id: call_id.clone(),
                                                    pending_safety_checks: Vec::new(),
                                                    status: ot::ResponseItemStatus::Completed,
                                                    type_: ot::ResponseComputerToolCallType::ComputerCall,
                                                },
                                            )
                                        } else {
                                            actual_kind = ClaudeToolKind::Custom;
                                            ResponseInputItem::CustomToolCall(ot::ResponseCustomToolCall {
                                                call_id: call_id.clone(),
                                                input: input_json,
                                                name: tool_name,
                                                type_: ot::ResponseCustomToolCallType::CustomToolCall,
                                                id: Some(call_id.clone()),
                                            })
                                        }
                                    }
                                    ClaudeToolKind::CodeInterpreter => ResponseInputItem::CodeInterpreterToolCall(
                                        ot::ResponseCodeInterpreterToolCall {
                                            id: call_id.clone(),
                                            code: str_field(&block.input, "code")
                                                .unwrap_or_default()
                                                .to_string(),
                                            container_id: str_field(&block.input, "container_id")
                                                .unwrap_or_default()
                                                .to_string(),
                                            outputs: None,
                                            status: ot::ResponseCodeInterpreterToolCallStatus::Completed,
                                            type_: ot::ResponseCodeInterpreterToolCallType::CodeInterpreterCall,
                                        },
                                    ),
                                    ClaudeToolKind::Shell => ResponseInputItem::ShellCall(
                                        ot::ResponseShellCall {
                                            action: ot::ResponseShellCallAction {
                                                commands: shell_commands(&block.input),
                                                max_output_length: None,
                                                timeout_ms: block
                                                    .input
                                                    .get("timeout_ms")
                                                    .and_then(|value| value.as_u64()),
                                            },
                                            call_id: call_id.clone(),
                                            type_: ot::ResponseShellCallType::ShellCall,
                                            id: Some(call_id.clone()),
                                            environment: None,
                                            status: Some(ot::ResponseItemStatus::Completed),
                                        },
                                    ),
                                    ClaudeToolKind::FileSearch => ResponseInputItem::FileSearchToolCall(
                                        ot::ResponseFileSearchToolCall {
                                            id: call_id.clone(),
                                            queries: file_search_queries(&block.input),
                                            status: ot::ResponseFileSearchToolCallStatus::Completed,
                                            type_: ot::ResponseFileSearchToolCallType::FileSearchCall,
                                            results: None,
                                        },
                                    ),
                                    ClaudeToolKind::WebSearch => ResponseInputItem::FunctionWebSearch(
                                        ot::ResponseFunctionWebSearch {
                                            id: Some(call_id.clone()),
                                            action: ot::ResponseFunctionWebSearchAction::Search {
                                                query: str_field(&block.input, "query")
                                                    .map(ToString::to_string),
                                                queries: {
                                                    let queries = string_list(block.input.get("queries"));
                                                    (queries.len() > 1).then_some(queries)
                                                },
                                                sources: None,
                                            },
                                            status: ot::ResponseFunctionWebSearchStatus::Completed,
                                            type_: ot::ResponseFunctionWebSearchType::WebSearchCall,
                                        },
                                    ),
                                    ClaudeToolKind::WebFetch => ResponseInputItem::FunctionWebSearch(
                                        ot::ResponseFunctionWebSearch {
                                            id: Some(call_id.clone()),
                                            action: ot::ResponseFunctionWebSearchAction::OpenPage {
                                                url: str_field(&block.input, "url")
                                                    .map(ToString::to_string),
                                            },
                                            status: ot::ResponseFunctionWebSearchStatus::Completed,
                                            type_: ot::ResponseFunctionWebSearchType::WebSearchCall,
                                        },
                                    ),
                                    ClaudeToolKind::Mcp => ResponseInputItem::FunctionToolCall(
                                        ot::ResponseFunctionToolCall {
                                            arguments: input_json,
                                            call_id: call_id.clone(),
                                            name: tool_name,
                                            type_: ot::ResponseFunctionToolCallType::FunctionCall,
                                            id: Some(call_id.clone()),
                                            status: Some(ot::ResponseItemStatus::Completed),
                                        },
                                    ),
                                };
                                let item_index = input_items.len();
                                input_items.push(item);
                                recorded_calls.insert(
                                    call_id,
                                    RecordedToolCall {
                                        item_index,
                                        kind: actual_kind,
                                    },
                                );
                            }
                            ct::BetaContentBlockParam::ServerToolUse(block) => {
                                let call_id = block.id.clone();
                                let item = match block.name {
                                    ct::BetaServerToolUseName::CodeExecution => {
                                        ResponseInputItem::CodeInterpreterToolCall(
                                            ot::ResponseCodeInterpreterToolCall {
                                                id: call_id.clone(),
                                                code: str_field(&block.input, "code")
                                                    .unwrap_or_default()
                                                    .to_string(),
                                                container_id: str_field(&block.input, "container_id")
                                                    .unwrap_or_default()
                                                    .to_string(),
                                                outputs: None,
                                                status: ot::ResponseCodeInterpreterToolCallStatus::Completed,
                                                type_: ot::ResponseCodeInterpreterToolCallType::CodeInterpreterCall,
                                            },
                                        )
                                    }
                                    ct::BetaServerToolUseName::WebSearch => {
                                        ResponseInputItem::FunctionWebSearch(ot::ResponseFunctionWebSearch {
                                            id: Some(call_id.clone()),
                                            action: ot::ResponseFunctionWebSearchAction::Search {
                                                query: str_field(&block.input, "query")
                                                    .map(ToString::to_string),
                                                queries: {
                                                    let queries = string_list(block.input.get("queries"));
                                                    (queries.len() > 1).then_some(queries)
                                                },
                                                sources: None,
                                            },
                                            status: ot::ResponseFunctionWebSearchStatus::Completed,
                                            type_: ot::ResponseFunctionWebSearchType::WebSearchCall,
                                        })
                                    }
                                    ct::BetaServerToolUseName::WebFetch => {
                                        ResponseInputItem::FunctionWebSearch(ot::ResponseFunctionWebSearch {
                                            id: Some(call_id.clone()),
                                            action: ot::ResponseFunctionWebSearchAction::OpenPage {
                                                url: str_field(&block.input, "url")
                                                    .map(ToString::to_string),
                                            },
                                            status: ot::ResponseFunctionWebSearchStatus::Completed,
                                            type_: ot::ResponseFunctionWebSearchType::WebSearchCall,
                                        })
                                    }
                                    ct::BetaServerToolUseName::BashCodeExecution => {
                                        ResponseInputItem::ShellCall(ot::ResponseShellCall {
                                            action: ot::ResponseShellCallAction {
                                                commands: shell_commands(&block.input),
                                                max_output_length: None,
                                                timeout_ms: block
                                                    .input
                                                    .get("timeout_ms")
                                                    .and_then(|value| value.as_u64()),
                                            },
                                            call_id: call_id.clone(),
                                            type_: ot::ResponseShellCallType::ShellCall,
                                            id: Some(call_id.clone()),
                                            environment: None,
                                            status: Some(ot::ResponseItemStatus::Completed),
                                        })
                                    }
                                    ct::BetaServerToolUseName::TextEditorCodeExecution => {
                                        ResponseInputItem::CustomToolCall(ot::ResponseCustomToolCall {
                                            call_id: call_id.clone(),
                                            input: json_string(&block.input),
                                            name: "text_editor_code_execution".to_string(),
                                            type_: ot::ResponseCustomToolCallType::CustomToolCall,
                                            id: Some(call_id.clone()),
                                        })
                                    }
                                    ct::BetaServerToolUseName::ToolSearchToolRegex => {
                                        ResponseInputItem::FileSearchToolCall(ot::ResponseFileSearchToolCall {
                                            id: call_id.clone(),
                                            queries: file_search_queries(&block.input),
                                            status: ot::ResponseFileSearchToolCallStatus::Completed,
                                            type_: ot::ResponseFileSearchToolCallType::FileSearchCall,
                                            results: None,
                                        })
                                    }
                                    ct::BetaServerToolUseName::ToolSearchToolBm25 => {
                                        ResponseInputItem::FileSearchToolCall(ot::ResponseFileSearchToolCall {
                                            id: call_id.clone(),
                                            queries: file_search_queries(&block.input),
                                            status: ot::ResponseFileSearchToolCallStatus::Completed,
                                            type_: ot::ResponseFileSearchToolCallType::FileSearchCall,
                                            results: None,
                                        })
                                    }
                                };
                                let kind = match block.name {
                                    ct::BetaServerToolUseName::CodeExecution => {
                                        ClaudeToolKind::CodeInterpreter
                                    }
                                    ct::BetaServerToolUseName::WebSearch => {
                                        ClaudeToolKind::WebSearch
                                    }
                                    ct::BetaServerToolUseName::WebFetch => ClaudeToolKind::WebFetch,
                                    ct::BetaServerToolUseName::BashCodeExecution => {
                                        ClaudeToolKind::Shell
                                    }
                                    ct::BetaServerToolUseName::TextEditorCodeExecution => {
                                        ClaudeToolKind::Custom
                                    }
                                    ct::BetaServerToolUseName::ToolSearchToolRegex
                                    | ct::BetaServerToolUseName::ToolSearchToolBm25 => {
                                        ClaudeToolKind::FileSearch
                                    }
                                };
                                let item_index = input_items.len();
                                input_items.push(item);
                                recorded_calls
                                    .insert(call_id, RecordedToolCall { item_index, kind });
                            }
                            ct::BetaContentBlockParam::McpToolUse(block) => {
                                let item_index = input_items.len();
                                input_items.push(ResponseInputItem::McpCall(ot::ResponseMcpCall {
                                    id: block.id.clone(),
                                    arguments: json_string(&block.input),
                                    name: block.name,
                                    server_label: block.server_name,
                                    type_: ot::ResponseMcpCallType::McpCall,
                                    approval_request_id: None,
                                    error: None,
                                    output: None,
                                    status: Some(ot::ResponseToolCallStatus::Completed),
                                }));
                                recorded_calls.insert(
                                    block.id,
                                    RecordedToolCall {
                                        item_index,
                                        kind: ClaudeToolKind::Mcp,
                                    },
                                );
                            }
                            ct::BetaContentBlockParam::Compaction(block) => {
                                input_items.push(ResponseInputItem::CompactionItem(
                                    ot::ResponseCompactionItemParam {
                                        encrypted_content: block.content.unwrap_or_default(),
                                        type_: ot::ResponseCompactionItemType::Compaction,
                                        id: None,
                                        created_by: None,
                                    },
                                ));
                            }
                            ct::BetaContentBlockParam::ContainerUpload(block) => {
                                input_items.push(output_message_item(
                                    format!("msg_{assistant_message_index}"),
                                    format!("container_upload:{}", block.file_id),
                                ));
                                assistant_message_index += 1;
                            }
                            other => {
                                input_items.push(output_message_item(
                                    format!("msg_{assistant_message_index}"),
                                    json_string(&other),
                                ));
                                assistant_message_index += 1;
                            }
                        }
                    }
                }
            }
        }

        let mut converted_tools = Vec::new();
        if let Some(tools) = body.tools {
            for tool in tools {
                match tool {
                    BetaToolUnion::Custom(tool) => {
                        if matches!(tool.type_, Some(ct::BetaCustomToolType::Custom)) {
                            converted_tools.push(ResponseTool::Custom(ot::ResponseCustomTool {
                                name: tool.name,
                                type_: ot::ResponseCustomToolType::Custom,
                                defer_loading: None,
                                description: tool.description,
                                format: Some(ot::ResponseCustomToolInputFormat::Text(
                                    ot::ResponseCustomToolTextFormat {
                                        type_: ot::ResponseCustomToolTextFormatType::Text,
                                    },
                                )),
                            }));
                        } else {
                            converted_tools.push(ResponseTool::Function(ResponseFunctionTool {
                                name: tool.name,
                                parameters: tool_input_schema_to_json_object(tool.input_schema),
                                strict: tool.common.strict,
                                type_: ResponseFunctionToolType::Function,
                                defer_loading: None,
                                description: tool.description,
                            }));
                        }
                    }
                    BetaToolUnion::CodeExecution20250522(_)
                    | BetaToolUnion::CodeExecution20250825(_) => {
                        converted_tools.push(ResponseTool::CodeInterpreter(
                            ResponseCodeInterpreterTool {
                                container: ResponseCodeInterpreterContainer::Auto(
                                    ResponseCodeInterpreterToolAuto {
                                        type_: ResponseCodeInterpreterToolAutoType::Auto,
                                        file_ids: None,
                                        memory_limit: None,
                                        network_policy: None,
                                    },
                                ),
                                type_: ResponseCodeInterpreterToolType::CodeInterpreter,
                            },
                        ));
                    }
                    BetaToolUnion::ComputerUse20241022(tool) => {
                        converted_tools.push(ResponseTool::Computer(ResponseComputerTool {
                            display_height: Some(tool.display_height_px),
                            display_width: Some(tool.display_width_px),
                            environment: Some(ResponseComputerEnvironment::Browser),
                            type_: ResponseComputerToolType::ComputerUsePreview,
                        }));
                    }
                    BetaToolUnion::ComputerUse20250124(tool) => {
                        converted_tools.push(ResponseTool::Computer(ResponseComputerTool {
                            display_height: Some(tool.display_height_px),
                            display_width: Some(tool.display_width_px),
                            environment: Some(ResponseComputerEnvironment::Browser),
                            type_: ResponseComputerToolType::ComputerUsePreview,
                        }));
                    }
                    BetaToolUnion::ComputerUse20251124(tool) => {
                        converted_tools.push(ResponseTool::Computer(ResponseComputerTool {
                            display_height: Some(tool.display_height_px),
                            display_width: Some(tool.display_width_px),
                            environment: Some(ResponseComputerEnvironment::Browser),
                            type_: ResponseComputerToolType::ComputerUsePreview,
                        }));
                    }
                    BetaToolUnion::WebSearch20250305(tool) => {
                        converted_tools.push(ResponseTool::WebSearch(ResponseWebSearchTool {
                            type_: ResponseWebSearchToolType::WebSearch,
                            filters: tool.allowed_domains.map(|allowed_domains| {
                                ResponseWebSearchFilters {
                                    allowed_domains: Some(allowed_domains),
                                }
                            }),
                            search_context_size: None,
                            user_location: tool.user_location.map(|location| {
                                ResponseApproximateLocation {
                                    city: location.city,
                                    country: location.country,
                                    region: location.region,
                                    timezone: location.timezone,
                                    type_: Some(ResponseApproximateLocationType::Approximate),
                                }
                            }),
                        }));
                    }
                    BetaToolUnion::WebFetch20250910(tool) => {
                        converted_tools.push(ResponseTool::WebSearch(ResponseWebSearchTool {
                            type_: ResponseWebSearchToolType::WebSearch,
                            filters: tool.allowed_domains.map(|allowed_domains| {
                                ResponseWebSearchFilters {
                                    allowed_domains: Some(allowed_domains),
                                }
                            }),
                            search_context_size: None,
                            user_location: None,
                        }));
                    }
                    BetaToolUnion::Bash20241022(_) | BetaToolUnion::Bash20250124(_) => {
                        converted_tools.push(ResponseTool::Shell(ResponseFunctionShellTool {
                            type_: ResponseFunctionShellToolType::Shell,
                            environment: None,
                        }));
                    }
                    BetaToolUnion::ToolSearchBm25_20251119(_)
                    | BetaToolUnion::ToolSearchRegex20251119(_) => {
                        converted_tools.push(ResponseTool::FileSearch(
                            ot::ResponseFileSearchTool {
                                type_: ot::ResponseFileSearchToolType::FileSearch,
                                vector_store_ids: Vec::new(),
                                filters: None,
                                max_num_results: None,
                                ranking_options: None,
                            },
                        ));
                    }
                    BetaToolUnion::TextEditor20241022(_)
                    | BetaToolUnion::TextEditor20250124(_)
                    | BetaToolUnion::TextEditor20250429(_)
                    | BetaToolUnion::TextEditor20250728(_) => {
                        converted_tools.push(ResponseTool::ApplyPatch(ResponseApplyPatchTool {
                            type_: ResponseApplyPatchToolType::ApplyPatch,
                        }));
                    }
                    BetaToolUnion::McpToolset(tool) => {
                        let allowed_tools = tool.configs.and_then(|configs| {
                            let names = configs
                                .into_iter()
                                .filter_map(|(name, config)| {
                                    if config.enabled.unwrap_or(true) {
                                        Some(name)
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();
                            if names.is_empty() {
                                None
                            } else {
                                Some(ResponseMcpAllowedTools::ToolNames(names))
                            }
                        });
                        converted_tools.push(ResponseTool::Mcp(ResponseMcpTool {
                            server_label: tool.mcp_server_name,
                            type_: ResponseMcpToolType::Mcp,
                            allowed_tools,
                            authorization: None,
                            connector_id: None,
                            defer_loading: None,
                            headers: None,
                            require_approval: None,
                            server_description: None,
                            server_url: None,
                        }));
                    }
                    BetaToolUnion::Memory20250818(_) => {}
                }
            }
        }
        if let Some(servers) = body.mcp_servers {
            for server in servers {
                converted_tools.push(ResponseTool::Mcp(ResponseMcpTool {
                    server_label: server.name,
                    type_: ResponseMcpToolType::Mcp,
                    allowed_tools: server
                        .tool_configuration
                        .as_ref()
                        .and_then(|config| config.allowed_tools.clone())
                        .map(ResponseMcpAllowedTools::ToolNames),
                    authorization: server.authorization_token,
                    connector_id: None,
                    defer_loading: None,
                    headers: None,
                    require_approval: None,
                    server_description: None,
                    server_url: Some(server.url),
                }));
            }
        }
        let tools = if converted_tools.is_empty() {
            None
        } else {
            Some(converted_tools)
        };

        let metadata = if let Some(user_id) = body
            .metadata
            .as_ref()
            .and_then(|value| value.user_id.clone())
        {
            let mut map = Metadata::new();
            map.insert("user_id".to_string(), user_id);
            Some(map)
        } else {
            None
        };
        let service_tier = match body.service_tier {
            Some(BetaServiceTierParam::Auto) => Some(ResponseServiceTier::Auto),
            Some(BetaServiceTierParam::StandardOnly) => Some(ResponseServiceTier::Default),
            None => match body.speed {
                Some(BetaSpeed::Fast) => Some(ResponseServiceTier::Priority),
                Some(BetaSpeed::Standard) | None => None,
            },
        };

        Ok(Self {
            method: HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                context_management,
                input: if input_items.is_empty() {
                    None
                } else {
                    Some(ResponseInput::Items(input_items))
                },
                instructions,
                max_output_tokens: Some(body.max_tokens),
                metadata,
                model: Some(model),
                parallel_tool_calls,
                reasoning,
                service_tier,
                stream: body.stream,
                temperature: body.temperature,
                text,
                tool_choice,
                tools,
                top_p: body.top_p,
                truncation,
                ..RequestBody::default()
            },
        })
    }
}
