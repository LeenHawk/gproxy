use serde::Deserialize;

use super::Tool;

impl<'de> Deserialize<'de> for Tool {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let type_ = value.get("type").and_then(serde_json::Value::as_str);
        let known = match type_ {
            Some(
                "bash_20241022"
                | "bash_20250124"
                | "code_execution_20250522"
                | "code_execution_20250825"
                | "code_execution_20260120"
                | "code_execution_20260521"
                | "memory_20250818"
                | "tool_search_tool_bm25_20251119"
                | "tool_search_tool_bm25"
                | "tool_search_tool_regex_20251119"
                | "tool_search_tool_regex",
            ) => decode(value, Tool::Command),
            Some(
                "text_editor_20241022"
                | "text_editor_20250124"
                | "text_editor_20250429"
                | "text_editor_20250728",
            ) => decode(value, Tool::TextEditor),
            Some("computer_20241022" | "computer_20250124" | "computer_20251124") => {
                decode(value, Tool::Computer)
            }
            Some("web_search_20250305" | "web_search_20260209" | "web_search_20260318") => {
                decode(value, Tool::WebSearch)
            }
            Some(
                "web_fetch_20250910" | "web_fetch_20260209" | "web_fetch_20260309"
                | "web_fetch_20260318",
            ) => decode(value, Tool::WebFetch),
            Some("advisor_20260301") => decode(value, Tool::Advisor),
            Some("mcp_toolset") => decode(value, Tool::McpToolset),
            Some("custom") | None if value.get("input_schema").is_some() => {
                decode(value, Tool::Custom)
            }
            _ => Ok(Tool::Unknown(value)),
        };
        known.map_err(serde::de::Error::custom)
    }
}

fn decode<T>(
    value: serde_json::Value,
    wrap: impl FnOnce(T) -> Tool,
) -> Result<Tool, serde_json::Error>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value).map(wrap)
}
