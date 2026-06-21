pub fn synthetic_tool_call_id(prefix: &str, index: usize) -> String {
    format!("{prefix}_{index}")
}

pub fn preserve_tool_call_id(id: impl Into<String>) -> String {
    id.into()
}

pub fn tool_result_is_error(value: Option<bool>) -> bool {
    value.unwrap_or(false)
}
