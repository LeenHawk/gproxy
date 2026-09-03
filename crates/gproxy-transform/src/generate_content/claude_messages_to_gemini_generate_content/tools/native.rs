use gproxy_protocol::claude;

pub(super) fn command(command: &claude::CommandTool) -> bool {
    match command {
        claude::CommandTool::Bash20241022(tool) => common(&tool.common),
        claude::CommandTool::Bash20250124(tool) => common(&tool.common),
        claude::CommandTool::CodeExecution20250522(tool) => common_without_examples(&tool.common),
        claude::CommandTool::CodeExecution20250825(tool) => common_without_examples(&tool.common),
        claude::CommandTool::CodeExecution20260120(tool) => common_without_examples(&tool.common),
        claude::CommandTool::CodeExecution20260521(tool) => common_without_examples(&tool.common),
        _ => false,
    }
}

pub(super) fn editor(editor: &claude::TextEditorTool) -> bool {
    match editor {
        claude::TextEditorTool::TextEditor20241022(tool) => common(&tool.common),
        claude::TextEditorTool::TextEditor20250124(tool) => common(&tool.common),
        claude::TextEditorTool::TextEditor20250429(tool) => common(&tool.common),
        claude::TextEditorTool::TextEditor20250728(tool) => {
            tool.max_characters.is_none() && common(&tool.common)
        }
        _ => false,
    }
}

fn common(common: &claude::ToolCommon) -> bool {
    common.allowed_callers.is_none()
        && common.cache_control.is_none()
        && common.defer_loading.is_none()
        && common.input_examples.is_empty()
        && common.strict.is_none()
}

fn common_without_examples(common: &claude::ToolCommonWithoutInputExamples) -> bool {
    common.allowed_callers.is_none()
        && common.cache_control.is_none()
        && common.defer_loading.is_none()
        && common.strict.is_none()
}
