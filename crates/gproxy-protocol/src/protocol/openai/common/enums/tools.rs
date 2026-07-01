use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSearchExecution {
    #[serde(rename = "server")]
    Server,
    #[serde(rename = "client")]
    Client,
}

strict_string_enum!(ToolChoiceMode {
    None => "none",
    Auto => "auto",
    Required => "required",
});

strict_string_enum!(AllowedToolsMode {
    Auto => "auto",
    Required => "required",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatToolCallType {
    #[serde(rename = "function")]
    Function,
    #[serde(rename = "custom")]
    Custom,
}

strict_string_enum!(CustomToolGrammarSyntax {
    Lark => "lark",
    Regex => "regex",
});

strict_string_enum!(CodeInterpreterContainerType {
    Auto => "auto",
});

strict_string_enum!(CodeInterpreterMemoryLimit {
    OneG => "1g",
    FourG => "4g",
    SixteenG => "16g",
    SixtyFourG => "64g",
});

strict_string_enum!(ImageGenerationAction {
    Generate => "generate",
    Edit => "edit",
    Auto => "auto",
});

strict_string_enum!(ToolType {
    Function => "function",
    Custom => "custom",
    FileSearch => "file_search",
    WebSearchPreview => "web_search_preview",
    WebSearchPreview20250311 => "web_search_preview_2025_03_11",
    WebSearch => "web_search",
    WebSearch20250826 => "web_search_2025_08_26",
    Computer => "computer",
    ComputerUse => "computer_use",
    ComputerUsePreview => "computer_use_preview",
    CodeInterpreter => "code_interpreter",
    ImageGeneration => "image_generation",
    Mcp => "mcp",
    ApplyPatch => "apply_patch",
    Shell => "shell",
    LocalShell => "local_shell",
    ToolSearch => "tool_search",
    Namespace => "namespace",
});
