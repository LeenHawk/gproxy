strict_string_enum!(ResponseStatus {
    Completed => "completed",
    Failed => "failed",
    InProgress => "in_progress",
    Cancelled => "cancelled",
    Queued => "queued",
    Incomplete => "incomplete",
});

strict_string_enum!(IncompleteReason {
    MaxOutputTokens => "max_output_tokens",
    ContentFilter => "content_filter",
});

extensible_string_enum!(ResponseErrorCode, ResponseErrorCodeKnown {
    ServerError => "server_error",
    RateLimitExceeded => "rate_limit_exceeded",
    InvalidPrompt => "invalid_prompt",
    VectorStoreTimeout => "vector_store_timeout",
    InvalidImage => "invalid_image",
    InvalidImageFormat => "invalid_image_format",
    InvalidBase64Image => "invalid_base64_image",
    InvalidImageUrl => "invalid_image_url",
    ImageTooLarge => "image_too_large",
    ImageTooSmall => "image_too_small",
    ImageParseError => "image_parse_error",
    ImageContentPolicyViolation => "image_content_policy_violation",
    InvalidImageMode => "invalid_image_mode",
    ImageFileTooLarge => "image_file_too_large",
    UnsupportedImageMediaType => "unsupported_image_media_type",
    EmptyImageFile => "empty_image_file",
    FailedToDownloadImage => "failed_to_download_image",
    ImageFileNotFound => "image_file_not_found",
});

strict_string_enum!(ResponseItemLifecycleStatus {
    InProgress => "in_progress",
    Completed => "completed",
    Incomplete => "incomplete",
});

strict_string_enum!(ResponseFileSearchCallStatus {
    InProgress => "in_progress",
    Searching => "searching",
    Completed => "completed",
    Incomplete => "incomplete",
    Failed => "failed",
});

strict_string_enum!(ResponseWebSearchCallStatus {
    InProgress => "in_progress",
    Searching => "searching",
    Completed => "completed",
    Failed => "failed",
});

strict_string_enum!(ResponseComputerCallOutputStatus {
    InProgress => "in_progress",
    Completed => "completed",
    Incomplete => "incomplete",
    Failed => "failed",
});

strict_string_enum!(ResponseImageGenerationCallStatus {
    InProgress => "in_progress",
    Completed => "completed",
    Generating => "generating",
    Failed => "failed",
});

strict_string_enum!(ResponseCodeInterpreterCallStatus {
    InProgress => "in_progress",
    Completed => "completed",
    Incomplete => "incomplete",
    Interpreting => "interpreting",
    Failed => "failed",
});

strict_string_enum!(ResponseApplyPatchCallStatus {
    InProgress => "in_progress",
    Completed => "completed",
});

strict_string_enum!(ResponseApplyPatchCallOutputStatus {
    Completed => "completed",
    Failed => "failed",
});

strict_string_enum!(ResponseMcpCallStatus {
    InProgress => "in_progress",
    Completed => "completed",
    Incomplete => "incomplete",
    Calling => "calling",
    Failed => "failed",
});

strict_string_enum!(ResponsePhase {
    Commentary => "commentary",
    FinalAnswer => "final_answer",
});

extensible_string_enum!(ResponseItemType, ResponseItemTypeKnown {
    Message => "message",
    FileSearchCall => "file_search_call",
    ComputerCall => "computer_call",
    ComputerCallOutput => "computer_call_output",
    WebSearchCall => "web_search_call",
    FunctionCall => "function_call",
    FunctionCallOutput => "function_call_output",
    ToolSearchCall => "tool_search_call",
    ToolSearchOutput => "tool_search_output",
    AdditionalTools => "additional_tools",
    Reasoning => "reasoning",
    Compaction => "compaction",
    ImageGenerationCall => "image_generation_call",
    CodeInterpreterCall => "code_interpreter_call",
    LocalShellCall => "local_shell_call",
    LocalShellCallOutput => "local_shell_call_output",
    ShellCall => "shell_call",
    ShellCallOutput => "shell_call_output",
    ApplyPatchCall => "apply_patch_call",
    ApplyPatchCallOutput => "apply_patch_call_output",
    McpListTools => "mcp_list_tools",
    McpApprovalRequest => "mcp_approval_request",
    McpApprovalResponse => "mcp_approval_response",
    McpCall => "mcp_call",
    CustomToolCall => "custom_tool_call",
    CustomToolCallOutput => "custom_tool_call_output",
    CompactionTrigger => "compaction_trigger",
    ItemReference => "item_reference",
});

strict_string_enum!(ResponseIncludable {
    FileSearchCallResults => "file_search_call.results",
    WebSearchCallResults => "web_search_call.results",
    WebSearchCallActionSources => "web_search_call.action.sources",
    MessageInputImageImageUrl => "message.input_image.image_url",
    ComputerCallOutputOutputImageUrl => "computer_call_output.output.image_url",
    CodeInterpreterCallOutputs => "code_interpreter_call.outputs",
    ReasoningEncryptedContent => "reasoning.encrypted_content",
    MessageOutputTextLogprobs => "message.output_text.logprobs",
});

extensible_string_enum!(ResponseStreamEventType, ResponseStreamEventTypeKnown {
    ResponseCreated => "response.created",
    ResponseInProgress => "response.in_progress",
    ResponseCompleted => "response.completed",
    ResponseFailed => "response.failed",
    ResponseIncomplete => "response.incomplete",
    ResponseQueued => "response.queued",
    ResponseOutputItemAdded => "response.output_item.added",
    ResponseOutputItemDone => "response.output_item.done",
    ResponseContentPartAdded => "response.content_part.added",
    ResponseContentPartDone => "response.content_part.done",
    ResponseOutputTextDelta => "response.output_text.delta",
    ResponseOutputTextDone => "response.output_text.done",
    ResponseOutputTextAnnotationAdded => "response.output_text.annotation.added",
    ResponseFunctionCallArgumentsDelta => "response.function_call_arguments.delta",
    ResponseFunctionCallArgumentsDone => "response.function_call_arguments.done",
    ResponseCustomToolCallInputDelta => "response.custom_tool_call_input.delta",
    ResponseCustomToolCallInputDone => "response.custom_tool_call_input.done",
    ResponseRefusalDelta => "response.refusal.delta",
    ResponseRefusalDone => "response.refusal.done",
    ResponseReasoningSummaryPartAdded => "response.reasoning_summary_part.added",
    ResponseReasoningSummaryPartDone => "response.reasoning_summary_part.done",
    ResponseReasoningSummaryTextDelta => "response.reasoning_summary_text.delta",
    ResponseReasoningSummaryTextDone => "response.reasoning_summary_text.done",
    ResponseReasoningTextDelta => "response.reasoning_text.delta",
    ResponseReasoningTextDone => "response.reasoning_text.done",
    ResponseAudioDelta => "response.audio.delta",
    ResponseAudioDone => "response.audio.done",
    ResponseAudioTranscriptDelta => "response.audio.transcript.delta",
    ResponseAudioTranscriptDone => "response.audio.transcript.done",
    ResponseImageGenerationCallCompleted => "response.image_generation_call.completed",
    ResponseImageGenerationCallGenerating => "response.image_generation_call.generating",
    ResponseImageGenerationCallInProgress => "response.image_generation_call.in_progress",
    ResponseImageGenerationCallPartialImage => "response.image_generation_call.partial_image",
    ResponseFileSearchCallInProgress => "response.file_search_call.in_progress",
    ResponseFileSearchCallSearching => "response.file_search_call.searching",
    ResponseFileSearchCallCompleted => "response.file_search_call.completed",
    ResponseWebSearchCallInProgress => "response.web_search_call.in_progress",
    ResponseWebSearchCallSearching => "response.web_search_call.searching",
    ResponseWebSearchCallCompleted => "response.web_search_call.completed",
    ResponseCodeInterpreterCallInProgress => "response.code_interpreter_call.in_progress",
    ResponseCodeInterpreterCallInterpreting => "response.code_interpreter_call.interpreting",
    ResponseCodeInterpreterCallCompleted => "response.code_interpreter_call.completed",
    ResponseCodeInterpreterCallCodeDelta => "response.code_interpreter_call_code.delta",
    ResponseCodeInterpreterCallCodeDone => "response.code_interpreter_call_code.done",
    ResponseMcpCallArgumentsDelta => "response.mcp_call_arguments.delta",
    ResponseMcpCallArgumentsDone => "response.mcp_call_arguments.done",
    ResponseMcpCallInProgress => "response.mcp_call.in_progress",
    ResponseMcpCallCompleted => "response.mcp_call.completed",
    ResponseMcpCallFailed => "response.mcp_call.failed",
    ResponseMcpListToolsInProgress => "response.mcp_list_tools.in_progress",
    ResponseMcpListToolsCompleted => "response.mcp_list_tools.completed",
    ResponseMcpListToolsFailed => "response.mcp_list_tools.failed",
    Error => "error",
});
