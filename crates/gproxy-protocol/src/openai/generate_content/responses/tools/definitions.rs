use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseTool {
    #[serde(rename = "function")]
    Function {
        name: String,
        parameters: ResponseFunctionParameters,
        // Some OpenAI-compatible Responses providers echo `strict: null`.
        #[serde(default, skip_serializing_if = "ResponseFunctionStrict::is_absent")]
        strict: ResponseFunctionStrict,
        #[serde(skip_serializing_if = "Option::is_none")]
        defer_loading: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_schema: Option<JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
        async_: Option<bool>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "file_search")]
    FileSearch {
        vector_store_ids: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filters: Option<FileSearchFilter>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_num_results: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ranking_options: Option<FileSearchRankingOptions>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "computer")]
    Computer {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview {
        display_height: u32,
        display_width: u32,
        environment: ComputerUseEnvironment,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "web_search")]
    WebSearch {
        #[serde(skip_serializing_if = "Option::is_none")]
        filters: Option<WebSearchFilters>,
        // gproxy extension: forwarded to Claude web_search `max_uses`; real OpenAI
        // upstreams do not accept it, so OpenAI-bound transforms never set it.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_uses: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_context_size: Option<SearchContextSize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_location: Option<WebSearchUserLocation>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "web_search_2025_08_26")]
    WebSearch20250826 {
        #[serde(skip_serializing_if = "Option::is_none")]
        filters: Option<WebSearchFilters>,
        // gproxy extension: see `WebSearch::max_uses`.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_uses: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_context_size: Option<SearchContextSize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_location: Option<WebSearchUserLocation>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    // gproxy extension: mirrors Claude web_fetch so Responses clients can
    // configure fetch behaviour on Claude routes.
    #[serde(rename = "web_fetch")]
    WebFetch {
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_domains: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_domains: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_content_tokens: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_uses: Option<u64>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    // gproxy extension: mirrors Claude memory_20250818.
    #[serde(rename = "memory")]
    Memory {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "x_search")]
    XSearch {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "collections_search")]
    CollectionsSearch {
        vector_store_ids: Vec<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "mcp")]
    Mcp {
        server_label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_tools: Option<McpAllowedTools>,
        #[serde(skip_serializing_if = "Option::is_none")]
        authorization: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        connector_id: Option<McpConnectorId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        defer_loading: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<BTreeMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        require_approval: Option<McpRequireApproval>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tunnel_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "code_execution")]
    CodeExecution {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "code_interpreter")]
    CodeInterpreter {
        container: CodeInterpreterContainer,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "image_generation")]
    ImageGeneration {
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<ImageGenerationAction>,
        #[serde(skip_serializing_if = "Option::is_none")]
        background: Option<ImageBackground>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_fidelity: Option<ImageInputFidelity>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_image_mask: Option<ImageMask>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<OpenAiModelId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        moderation: Option<ImageModeration>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_compression: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_format: Option<ImageOutputFormat>,
        #[serde(skip_serializing_if = "Option::is_none")]
        partial_images: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        quality: Option<ImageEditQuality>,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<ResponseImageGenerationSize>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "local_shell")]
    LocalShell {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "shell")]
    Shell {
        #[serde(skip_serializing_if = "Option::is_none")]
        environment: Option<ResponseShellEnvironment>,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "custom")]
    Custom {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        defer_loading: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<CustomToolInputFormat>,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
        async_: Option<bool>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "namespace")]
    Namespace {
        description: String,
        name: String,
        tools: Vec<ResponseNamespaceTool>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "tool_search")]
    ToolSearch {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        execution: Option<ToolSearchExecution>,
        /// `upstream_docs/openai/docs/Responses.md`, `ToolSearch.parameters`:
        /// optional unknown or null.
        #[serde(skip_serializing_if = "Option::is_none")]
        parameters: Option<Value>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "programmatic_tool_calling")]
    ProgrammaticToolCalling {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "web_search_preview")]
    WebSearchPreview {
        #[serde(skip_serializing_if = "Option::is_none")]
        search_content_types: Option<Vec<SearchContentType>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_context_size: Option<SearchContextSize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_location: Option<WebSearchPreviewUserLocation>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311 {
        #[serde(skip_serializing_if = "Option::is_none")]
        search_content_types: Option<Vec<SearchContentType>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_context_size: Option<SearchContextSize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_location: Option<WebSearchPreviewUserLocation>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "apply_patch")]
    ApplyPatch {
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        // gproxy extension: forwarded to Claude text_editor `max_characters`.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_characters: Option<u64>,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseNamespaceTool {
    #[serde(rename = "function")]
    Function {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        defer_loading: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_schema: Option<JsonSchema>,
        /// `upstream_docs/openai/docs/Responses.md`,
        /// `Namespace.tools[].Function.parameters`: optional unknown or null.
        #[serde(skip_serializing_if = "Option::is_none")]
        parameters: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
        async_: Option<bool>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "custom")]
    Custom {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        defer_loading: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<CustomToolInputFormat>,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_callers: Option<Vec<ToolCaller>>,
        #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
        async_: Option<bool>,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseFunctionParameters {
    Schema(JsonSchema),
    Null,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseFunctionStrict {
    Value(bool),
    Null,
    #[default]
    Absent,
}

impl ResponseFunctionStrict {
    fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}
