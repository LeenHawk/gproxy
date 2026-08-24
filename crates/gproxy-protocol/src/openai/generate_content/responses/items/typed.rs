use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

use super::super::tools::ResponseTool;
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum TypedResponseItem {
    #[serde(rename = "file_search_call")]
    FileSearchCall {
        id: String,
        queries: Vec<String>,
        status: ResponseFileSearchCallStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        results: Option<Vec<FileSearchResult>>,
        #[serde(default, flatten)]
        rest: Rest,
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
        #[serde(default, flatten)]
        rest: Rest,
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
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "web_search_call")]
    WebSearchCall {
        id: String,
        action: WebSearchAction,
        status: ResponseWebSearchCallStatus,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        arguments: String,
        call_id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        call_id: String,
        output: ResponseOutput,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
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
        #[serde(default, flatten)]
        rest: Rest,
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
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "additional_tools")]
    AdditionalTools {
        role: AdditionalToolsRole,
        tools: Vec<ResponseTool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        summary: Vec<ResponseReasoningSummaryPart>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ResponseReasoningTextPart>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "compaction")]
    Compaction {
        encrypted_content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "image_generation_call")]
    ImageGenerationCall {
        id: String,
        // Required-nullable on the wire while image generation is pending.
        result: Option<String>,
        status: ResponseImageGenerationCallStatus,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "code_interpreter_call")]
    CodeInterpreterCall {
        id: String,
        // Required-nullable on the wire while code is unavailable.
        code: Option<String>,
        container_id: String,
        // Required-nullable on the wire before execution produces output.
        outputs: Option<Vec<CodeInterpreterOutput>>,
        status: ResponseCodeInterpreterCallStatus,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "local_shell_call")]
    LocalShellCall {
        id: String,
        action: LocalShellAction,
        call_id: String,
        status: ResponseItemLifecycleStatus,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "local_shell_call_output")]
    LocalShellCallOutput {
        id: String,
        output: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "shell_call")]
    ShellCall {
        action: ShellAction,
        call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        environment: Option<ShellEnvironment>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "shell_call_output")]
    ShellCallOutput {
        call_id: String,
        output: Vec<ShellCallOutputContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_output_length: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "apply_patch_call")]
    ApplyPatchCall {
        call_id: String,
        operation: ApplyPatchOperation,
        status: ResponseApplyPatchCallStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "apply_patch_call_output")]
    ApplyPatchCallOutput {
        call_id: String,
        status: ResponseApplyPatchCallOutputStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "mcp_list_tools")]
    McpListTools {
        id: String,
        server_label: String,
        tools: Vec<McpToolDescription>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "mcp_approval_request")]
    McpApprovalRequest {
        id: String,
        arguments: String,
        name: String,
        server_label: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "mcp_approval_response")]
    McpApprovalResponse {
        approval_request_id: String,
        approve: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
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
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "custom_tool_call")]
    CustomToolCall {
        call_id: String,
        input: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "custom_tool_call_output")]
    CustomToolCallOutput {
        call_id: String,
        output: ResponseOutput,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ResponseCaller>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<ResponseItemLifecycleStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        created_by: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "program")]
    Program {
        id: String,
        call_id: String,
        code: String,
        fingerprint: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "program_output")]
    ProgramOutput {
        id: String,
        call_id: String,
        result: String,
        status: ResponseItemLifecycleStatus,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "multi_agent_call")]
    MultiAgentCall {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        agent: Option<ResponseAgent>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "multi_agent_call_output")]
    MultiAgentCallOutput {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<Vec<ResponseOutputContentPart>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        agent: Option<ResponseAgent>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "agent_message")]
    AgentMessage {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        recipient: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        agent: Option<ResponseAgent>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "compaction_trigger")]
    CompactionTrigger {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "item_reference")]
    ItemReference {
        id: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
}
