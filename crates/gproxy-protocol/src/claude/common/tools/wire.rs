use serde::{Deserialize, Serialize};

macro_rules! single_wire_enum {
    ($name:ident { $variant:ident => $wire:literal }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[non_exhaustive]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

single_wire_enum!(BashToolName { Bash => "bash" });
single_wire_enum!(BashTool20241022Type { Bash20241022 => "bash_20241022" });
single_wire_enum!(BashTool20250124Type { Bash20250124 => "bash_20250124" });
single_wire_enum!(CodeExecutionToolName { CodeExecution => "code_execution" });
single_wire_enum!(CodeExecutionTool20250522Type { CodeExecution20250522 => "code_execution_20250522" });
single_wire_enum!(CodeExecutionTool20250825Type { CodeExecution20250825 => "code_execution_20250825" });
single_wire_enum!(CodeExecutionTool20260120Type { CodeExecution20260120 => "code_execution_20260120" });
single_wire_enum!(CodeExecutionTool20260521Type { CodeExecution20260521 => "code_execution_20260521" });
single_wire_enum!(MemoryToolName { Memory => "memory" });
single_wire_enum!(MemoryTool20250818Type { Memory20250818 => "memory_20250818" });
single_wire_enum!(ToolSearchBm25ToolName { ToolSearchBm25 => "tool_search_tool_bm25" });
single_wire_enum!(ToolSearchRegexToolName { ToolSearchRegex => "tool_search_tool_regex" });
single_wire_enum!(StrReplaceEditorToolName { StrReplaceEditor => "str_replace_editor" });
single_wire_enum!(StrReplaceBasedEditToolName { StrReplaceBasedEditTool => "str_replace_based_edit_tool" });
single_wire_enum!(TextEditorTool20241022Type { TextEditor20241022 => "text_editor_20241022" });
single_wire_enum!(TextEditorTool20250124Type { TextEditor20250124 => "text_editor_20250124" });
single_wire_enum!(TextEditorTool20250429Type { TextEditor20250429 => "text_editor_20250429" });
single_wire_enum!(TextEditorTool20250728Type { TextEditor20250728 => "text_editor_20250728" });
single_wire_enum!(ComputerToolName { Computer => "computer" });
single_wire_enum!(ComputerTool20241022Type { Computer20241022 => "computer_20241022" });
single_wire_enum!(ComputerTool20250124Type { Computer20250124 => "computer_20250124" });
single_wire_enum!(ComputerTool20251124Type { Computer20251124 => "computer_20251124" });
single_wire_enum!(WebSearchToolName { WebSearch => "web_search" });
single_wire_enum!(WebSearchTool20250305Type { WebSearch20250305 => "web_search_20250305" });
single_wire_enum!(WebSearchTool20260209Type { WebSearch20260209 => "web_search_20260209" });
single_wire_enum!(WebSearchTool20260318Type { WebSearch20260318 => "web_search_20260318" });
single_wire_enum!(WebFetchToolName { WebFetch => "web_fetch" });
single_wire_enum!(WebFetchTool20250910Type { WebFetch20250910 => "web_fetch_20250910" });
single_wire_enum!(WebFetchTool20260209Type { WebFetch20260209 => "web_fetch_20260209" });
single_wire_enum!(WebFetchTool20260309Type { WebFetch20260309 => "web_fetch_20260309" });
single_wire_enum!(WebFetchTool20260318Type { WebFetch20260318 => "web_fetch_20260318" });
single_wire_enum!(AdvisorToolName { Advisor => "advisor" });
single_wire_enum!(AdvisorTool20260301Type { Advisor20260301 => "advisor_20260301" });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolSearchBm25ToolType {
    #[serde(rename = "tool_search_tool_bm25_20251119")]
    ToolSearchBm2520251119,
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchBm25,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolSearchRegexToolType {
    #[serde(rename = "tool_search_tool_regex_20251119")]
    ToolSearchRegex20251119,
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchRegex,
}
