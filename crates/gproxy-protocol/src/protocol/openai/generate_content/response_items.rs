use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::common::*;
use super::response_tools::ResponseTool;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ResponseItem {
    Message(ResponseMessageItem),
    Typed(TypedResponseItem),
    Unknown(UnknownResponseItem),
}

impl<'de> Deserialize<'de> for ResponseItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let type_name = value.get("type").and_then(Value::as_str);

        let Some(type_name) = type_name else {
            if let Ok(message) = serde_json::from_value::<ResponseMessageItem>(value.clone()) {
                return Ok(Self::Message(message));
            }

            if let Some(item_reference) = item_reference_without_type(&value) {
                return Ok(Self::Typed(item_reference));
            }

            return serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom);
        };

        let item_type =
            serde_json::from_value::<ResponseItemType>(Value::String(type_name.to_owned()))
                .map_err(de::Error::custom)?;

        match item_type {
            ResponseItemType::Known(ResponseItemTypeKnown::Message) => {
                serde_json::from_value(value)
                    .map(Self::Message)
                    .map_err(de::Error::custom)
            }
            ResponseItemType::Known(_) => serde_json::from_value(value)
                .map(Self::Typed)
                .map_err(de::Error::custom),
            ResponseItemType::Unknown(_) => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

fn item_reference_without_type(value: &Value) -> Option<TypedResponseItem> {
    let object = value.as_object()?;
    let id = object.get("id")?.as_str()?.to_owned();
    let mut extra = Extra::new();

    for (key, value) in object {
        if key != "id" {
            extra.insert(key.clone(), value.clone());
        }
    }

    Some(TypedResponseItem::ItemReference { id, extra })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ResponseOutputItem(pub ResponseItem);

impl<'de> Deserialize<'de> for ResponseOutputItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let item = ResponseItem::deserialize(deserializer)?;
        validate_response_output_item(&item).map_err(de::Error::custom)?;
        Ok(Self(item))
    }
}

fn validate_response_output_item(item: &ResponseItem) -> Result<(), &'static str> {
    let ResponseItem::Typed(typed) = item else {
        return Ok(());
    };

    match typed {
        TypedResponseItem::ComputerCallOutput { id, status, .. } => {
            require_some(id, "computer_call_output.id")?;
            require_some(status, "computer_call_output.status")?;
        }
        TypedResponseItem::FunctionCallOutput { id, status, .. } => {
            require_some(id, "function_call_output.id")?;
            require_some(status, "function_call_output.status")?;
        }
        TypedResponseItem::ToolSearchCall {
            id,
            call_id,
            execution,
            status,
            ..
        } => {
            require_some(id, "tool_search_call.id")?;
            require_some(call_id, "tool_search_call.call_id")?;
            require_some(execution, "tool_search_call.execution")?;
            require_some(status, "tool_search_call.status")?;
        }
        TypedResponseItem::ToolSearchOutput {
            id,
            call_id,
            execution,
            status,
            ..
        } => {
            require_some(id, "tool_search_output.id")?;
            require_some(call_id, "tool_search_output.call_id")?;
            require_some(execution, "tool_search_output.execution")?;
            require_some(status, "tool_search_output.status")?;
        }
        TypedResponseItem::AdditionalTools { id, .. } => {
            require_some(id, "additional_tools.id")?;
        }
        TypedResponseItem::ShellCall {
            id,
            environment,
            status,
            ..
        } => {
            require_some(id, "shell_call.id")?;
            require_some(environment, "shell_call.environment")?;
            require_some(status, "shell_call.status")?;
        }
        TypedResponseItem::ShellCallOutput {
            id,
            max_output_length,
            status,
            ..
        } => {
            require_some(id, "shell_call_output.id")?;
            require_some(max_output_length, "shell_call_output.max_output_length")?;
            require_some(status, "shell_call_output.status")?;
        }
        _ => {}
    }

    Ok(())
}

fn require_some<T>(value: &Option<T>, field: &'static str) -> Result<(), &'static str> {
    value.as_ref().map(|_| ()).ok_or(field)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ResponseMessageItem {
    Output(ResponseOutputMessageItem),
    Input(ResponseInputMessageItem),
    EasyInput(ResponseEasyInputMessageItem),
}

impl<'de> Deserialize<'de> for ResponseMessageItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let role = value.get("role").and_then(Value::as_str);
        let has_id = value.get("id").is_some();
        let has_status = value.get("status").is_some();

        if role == Some("assistant") && has_id && has_status {
            return serde_json::from_value(value)
                .map(Self::Output)
                .map_err(de::Error::custom);
        }

        if has_id || has_status {
            return serde_json::from_value(value)
                .map(Self::Input)
                .map_err(de::Error::custom);
        }

        serde_json::from_value(value)
            .map(Self::EasyInput)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputMessageItem {
    #[serde(rename = "type")]
    pub type_: ResponseMessageItemType,
    pub id: String,
    pub role: ResponseOutputMessageRole,
    pub content: Vec<ResponseMessageOutputContentPart>,
    pub status: ResponseItemLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputMessageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseMessageItemType>,
    pub role: ResponseInputMessageRole,
    pub content: Vec<ResponseInputContentPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemLifecycleStatus>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseEasyInputMessageItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseMessageItemType>,
    pub role: ResponseEasyInputMessageRole,
    pub content: ResponseEasyInputContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMessageItemType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseOutputMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseEasyInputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TypedResponseItem {
    #[serde(rename = "file_search_call")]
    FileSearchCall {
        id: String,
        queries: Vec<String>,
        status: ResponseFileSearchCallStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        results: Option<Vec<FileSearchResult>>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "computer_call")]
    ComputerCall {
        id: String,
        call_id: String,
        pending_safety_checks: Vec<SafetyCheck>,
        status: ResponseItemLifecycleStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<ComputerAction>,
        #[serde(skip_serializing_if = "Option::is_none")]
        actions: Option<Vec<ComputerAction>>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "computer_call_output")]
    ComputerCallOutput {
        call_id: String,
        output: ComputerScreenshot,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        acknowledged_safety_checks: Option<Vec<SafetyCheck>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseComputerCallOutputStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "web_search_call")]
    WebSearchCall {
        id: String,
        action: WebSearchAction,
        status: ResponseWebSearchCallStatus,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        arguments: String,
        call_id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        call_id: String,
        output: ResponseOutput,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "tool_search_call")]
    ToolSearchCall {
        arguments: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        execution: Option<ToolSearchExecution>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "tool_search_output")]
    ToolSearchOutput {
        tools: Vec<ResponseTool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        execution: Option<ToolSearchExecution>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "additional_tools")]
    AdditionalTools {
        role: AdditionalToolsRole,
        tools: Vec<ResponseTool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        id: String,
        summary: Vec<ResponseReasoningSummaryPart>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ResponseReasoningTextPart>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "compaction")]
    Compaction {
        encrypted_content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "image_generation_call")]
    ImageGenerationCall {
        id: String,
        result: String,
        status: ResponseImageGenerationCallStatus,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "code_interpreter_call")]
    CodeInterpreterCall {
        id: String,
        code: Option<String>,
        container_id: String,
        outputs: Option<Vec<CodeInterpreterOutput>>,
        status: ResponseCodeInterpreterCallStatus,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "local_shell_call")]
    LocalShellCall {
        id: String,
        action: LocalShellAction,
        call_id: String,
        status: ResponseItemLifecycleStatus,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "local_shell_call_output")]
    LocalShellCallOutput {
        id: String,
        output: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "shell_call")]
    ShellCall {
        action: ShellAction,
        call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        environment: Option<ShellEnvironment>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "shell_call_output")]
    ShellCallOutput {
        call_id: String,
        output: Vec<ShellCallOutputContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_output_length: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "apply_patch_call")]
    ApplyPatchCall {
        call_id: String,
        operation: ApplyPatchOperation,
        status: ResponseApplyPatchCallStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "apply_patch_call_output")]
    ApplyPatchCallOutput {
        call_id: String,
        status: ResponseApplyPatchCallOutputStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "mcp_list_tools")]
    McpListTools {
        id: String,
        server_label: String,
        tools: Vec<McpToolDescription>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "mcp_approval_request")]
    McpApprovalRequest {
        id: String,
        arguments: String,
        name: String,
        server_label: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "mcp_approval_response")]
    McpApprovalResponse {
        approval_request_id: String,
        approve: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "mcp_call")]
    McpCall {
        id: String,
        arguments: String,
        name: String,
        server_label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        approval_request_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseMcpCallStatus>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "custom_tool_call")]
    CustomToolCall {
        call_id: String,
        input: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "custom_tool_call_output")]
    CustomToolCallOutput {
        call_id: String,
        output: ResponseOutput,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "compaction_trigger")]
    CompactionTrigger {
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "item_reference")]
    ItemReference {
        id: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownResponseItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseItemType>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutput {
    Text(String),
    Parts(Vec<ResponseToolOutputContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseEasyInputContent {
    Text(String),
    Parts(Vec<ResponseInputContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseInputContentPart {
    #[serde(rename = "input_text")]
    InputText {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_image")]
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<DetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_file")]
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<InputFileDetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_audio")]
    InputAudio {
        input_audio: InputAudioContent,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseToolOutputContentPart {
    #[serde(rename = "input_text")]
    InputText {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_image")]
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<DetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_file")]
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<InputFileDetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseMessageOutputContentPart {
    #[serde(rename = "output_text")]
    OutputText {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<ResponseAnnotation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<TokenLogprob>>,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "refusal")]
    Refusal {
        refusal: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseOutputContentPart {
    #[serde(rename = "output_text")]
    OutputText {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<ResponseAnnotation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<TokenLogprob>>,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "refusal")]
    Refusal {
        refusal: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "reasoning_text")]
    ReasoningText {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

pub type ResponseContentPart = ResponseOutputContentPart;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAudioContent {
    pub data: String,
    pub format: InputAudioFormat,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseAnnotation {
    #[serde(rename = "file_citation")]
    FileCitation {
        file_id: String,
        filename: String,
        index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "url_citation")]
    UrlCitation {
        end_index: u32,
        start_index: u32,
        title: String,
        url: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "container_file_citation")]
    ContainerFileCitation {
        container_id: String,
        end_index: u32,
        file_id: String,
        filename: String,
        start_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "file_path")]
    FilePath {
        file_id: String,
        index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSearchResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<FileSearchResultAttributes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

pub type FileSearchResultAttributes = BTreeMap<String, FileSearchResultAttributeValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchResultAttributeValue {
    String(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyCheck {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ComputerAction {
    #[serde(rename = "click")]
    Click {
        button: ComputerMouseButton,
        x: f64,
        y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    #[serde(rename = "double_click")]
    DoubleClick { keys: Vec<String>, x: f64, y: f64 },
    #[serde(rename = "drag")]
    Drag {
        path: Vec<ComputerCoordinate>,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    #[serde(rename = "keypress")]
    Keypress { keys: Vec<String> },
    #[serde(rename = "move")]
    Move {
        x: f64,
        y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    #[serde(rename = "screenshot")]
    Screenshot {},
    #[serde(rename = "scroll")]
    Scroll {
        scroll_x: f64,
        scroll_y: f64,
        x: f64,
        y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    #[serde(rename = "type")]
    Type { text: String },
    #[serde(rename = "wait")]
    Wait {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputerMouseButton {
    #[serde(rename = "left")]
    Left,
    #[serde(rename = "right")]
    Right,
    #[serde(rename = "wheel")]
    Wheel,
    #[serde(rename = "back")]
    Back,
    #[serde(rename = "forward")]
    Forward,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerCoordinate {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerScreenshot {
    #[serde(rename = "type")]
    pub type_: ComputerScreenshotType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputerScreenshotType {
    #[serde(rename = "computer_screenshot")]
    ComputerScreenshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebSearchAction {
    #[serde(rename = "search")]
    Search {
        #[serde(skip_serializing_if = "Option::is_none")]
        queries: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sources: Option<Vec<WebSearchSource>>,
    },
    #[serde(rename = "open_page")]
    OpenPage {
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    #[serde(rename = "find_in_page")]
    FindInPage { pattern: String, url: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchSource {
    #[serde(rename = "type")]
    pub type_: WebSearchSourceType,
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSearchSourceType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdditionalToolsRole {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "critic")]
    Critic,
    #[serde(rename = "discriminator")]
    Discriminator,
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningSummaryPart {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseReasoningSummaryType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseReasoningSummaryType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningTextPart {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseReasoningTextType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseReasoningTextType {
    #[serde(rename = "reasoning_text")]
    ReasoningText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CodeInterpreterOutput {
    #[serde(rename = "logs")]
    Logs { logs: String },
    #[serde(rename = "image")]
    Image { url: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalShellAction {
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    #[serde(rename = "type")]
    pub type_: LocalShellActionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalShellActionType {
    #[serde(rename = "exec")]
    Exec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellAction {
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ShellEnvironment {
    #[serde(rename = "local")]
    Local {
        #[serde(skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<ShellSkillReference>>,
    },
    #[serde(rename = "container_reference")]
    ContainerReference { container_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellSkillReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellCallOutputContent {
    pub outcome: ShellCallOutcome,
    pub stderr: String,
    pub stdout: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ShellCallOutcome {
    #[serde(rename = "timeout")]
    Timeout {},
    #[serde(rename = "exit")]
    Exit { exit_code: i32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ApplyPatchOperation {
    #[serde(rename = "create_file")]
    CreateFile { diff: String, path: String },
    #[serde(rename = "delete_file")]
    DeleteFile { path: String },
    #[serde(rename = "update_file")]
    UpdateFile { diff: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolDescription {
    pub input_schema: Value,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: `ResponseItem`/`ResponseMessageItem` must serialize FLAT
    /// (untagged), matching their hand-written flat `Deserialize`. Before the
    /// `#[serde(untagged)]` fix the derived `Serialize` emitted
    /// `{"Message":{"EasyInput":{...}}}`, so the Responses backend saw no
    /// top-level `role` and rejected `input[0]` with role `''`.
    #[test]
    fn input_message_serializes_flat() {
        let flat = serde_json::json!({"type": "message", "role": "user", "content": "hi"});
        let item: ResponseItem = serde_json::from_value(flat.clone()).unwrap();
        let back = serde_json::to_value(&item).unwrap();
        assert!(
            back.get("Message").is_none() && back.get("EasyInput").is_none(),
            "must not be externally tagged: {back}"
        );
        assert_eq!(back["role"], "user", "{back}");
        assert_eq!(back, flat);
    }
}
