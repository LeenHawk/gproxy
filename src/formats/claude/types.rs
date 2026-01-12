use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaModelInfo {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub display_name: String,
    #[serde(rename = "type")]
    pub model_type: ModelObjectType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ModelObjectType {
    #[serde(rename = "model")]
    Model,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicBeta {
    Known(AnthropicBetaKnown),
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnthropicBetaKnown {
    #[serde(rename = "message-batches-2024-09-24")]
    MessageBatches20240924,
    #[serde(rename = "prompt-caching-2024-07-31")]
    PromptCaching20240731,
    #[serde(rename = "computer-use-2024-10-22")]
    ComputerUse20241022,
    #[serde(rename = "computer-use-2025-01-24")]
    ComputerUse20250124,
    #[serde(rename = "pdfs-2024-09-25")]
    Pdfs20240925,
    #[serde(rename = "token-counting-2024-11-01")]
    TokenCounting20241101,
    #[serde(rename = "token-efficient-tools-2025-02-19")]
    TokenEfficientTools20250219,
    #[serde(rename = "output-128k-2025-02-19")]
    Output128k20250219,
    #[serde(rename = "files-api-2025-04-14")]
    FilesApi20250414,
    #[serde(rename = "mcp-client-2025-04-04")]
    McpClient20250404,
    #[serde(rename = "mcp-client-2025-11-20")]
    McpClient20251120,
    #[serde(rename = "dev-full-thinking-2025-05-14")]
    DevFullThinking20250514,
    #[serde(rename = "interleaved-thinking-2025-05-14")]
    InterleavedThinking20250514,
    #[serde(rename = "code-execution-2025-05-22")]
    CodeExecution20250522,
    #[serde(rename = "extended-cache-ttl-2025-04-11")]
    ExtendedCacheTtl20250411,
    #[serde(rename = "context-1m-2025-08-07")]
    Context1m20250807,
    #[serde(rename = "context-management-2025-06-27")]
    ContextManagement20250627,
    #[serde(rename = "model-context-window-exceeded-2025-08-26")]
    ModelContextWindowExceeded20250826,
    #[serde(rename = "skills-2025-10-02")]
    Skills20251002,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Model {
    Known(KnownModel),
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KnownModel {
    #[serde(rename = "claude-opus-4-5-20251101")]
    ClaudeOpus4520251101,
    #[serde(rename = "claude-opus-4-5")]
    ClaudeOpus45,
    #[serde(rename = "claude-3-7-sonnet-latest")]
    Claude37SonnetLatest,
    #[serde(rename = "claude-3-7-sonnet-20250219")]
    Claude37Sonnet20250219,
    #[serde(rename = "claude-3-5-haiku-latest")]
    Claude35HaikuLatest,
    #[serde(rename = "claude-3-5-haiku-20241022")]
    Claude35Haiku20241022,
    #[serde(rename = "claude-haiku-4-5")]
    ClaudeHaiku45,
    #[serde(rename = "claude-haiku-4-5-20251001")]
    ClaudeHaiku4520251001,
    #[serde(rename = "claude-sonnet-4-20250514")]
    ClaudeSonnet420250514,
    #[serde(rename = "claude-sonnet-4-0")]
    ClaudeSonnet40,
    #[serde(rename = "claude-4-sonnet-20250514")]
    Claude4Sonnet20250514,
    #[serde(rename = "claude-sonnet-4-5")]
    ClaudeSonnet45,
    #[serde(rename = "claude-sonnet-4-5-20250929")]
    ClaudeSonnet4520250929,
    #[serde(rename = "claude-opus-4-0")]
    ClaudeOpus40,
    #[serde(rename = "claude-opus-4-20250514")]
    ClaudeOpus420250514,
    #[serde(rename = "claude-4-opus-20250514")]
    Claude4Opus20250514,
    #[serde(rename = "claude-opus-4-1-20250805")]
    ClaudeOpus4120250805,
    #[serde(rename = "claude-3-opus-latest")]
    Claude3OpusLatest,
    #[serde(rename = "claude-3-opus-20240229")]
    Claude3Opus20240229,
    #[serde(rename = "claude-3-haiku-20240307")]
    Claude3Haiku20240307,
}

impl Model {
    pub fn as_str(&self) -> &str {
        match self {
            Model::Known(model) => model.as_str(),
            Model::Other(value) => value.as_str(),
        }
    }
}

impl KnownModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            KnownModel::ClaudeOpus4520251101 => "claude-opus-4-5-20251101",
            KnownModel::ClaudeOpus45 => "claude-opus-4-5",
            KnownModel::Claude37SonnetLatest => "claude-3-7-sonnet-latest",
            KnownModel::Claude37Sonnet20250219 => "claude-3-7-sonnet-20250219",
            KnownModel::Claude35HaikuLatest => "claude-3-5-haiku-latest",
            KnownModel::Claude35Haiku20241022 => "claude-3-5-haiku-20241022",
            KnownModel::ClaudeHaiku45 => "claude-haiku-4-5",
            KnownModel::ClaudeHaiku4520251001 => "claude-haiku-4-5-20251001",
            KnownModel::ClaudeSonnet420250514 => "claude-sonnet-4-20250514",
            KnownModel::ClaudeSonnet40 => "claude-sonnet-4-0",
            KnownModel::Claude4Sonnet20250514 => "claude-4-sonnet-20250514",
            KnownModel::ClaudeSonnet45 => "claude-sonnet-4-5",
            KnownModel::ClaudeSonnet4520250929 => "claude-sonnet-4-5-20250929",
            KnownModel::ClaudeOpus40 => "claude-opus-4-0",
            KnownModel::ClaudeOpus420250514 => "claude-opus-4-20250514",
            KnownModel::Claude4Opus20250514 => "claude-4-opus-20250514",
            KnownModel::ClaudeOpus4120250805 => "claude-opus-4-1-20250805",
            KnownModel::Claude3OpusLatest => "claude-3-opus-latest",
            KnownModel::Claude3Opus20240229 => "claude-3-opus-20240229",
            KnownModel::Claude3Haiku20240307 => "claude-3-haiku-20240307",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MessageList(pub Vec<BetaMessageParam>);

impl Serialize for MessageList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MessageList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let items = Vec::<BetaMessageParam>::deserialize(deserializer)?;
        if items.len() > 100_000 {
            return Err(de::Error::custom("messages length must be <= 100000"));
        }
        Ok(Self(items))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetTokens(u32);

impl BudgetTokens {
    pub const MIN: u32 = 1024;

    pub fn new(value: u32) -> Result<Self, String> {
        if value >= Self::MIN {
            Ok(Self(value))
        } else {
            Err(format!(
                "budget_tokens must be >= {} (got {})",
                Self::MIN,
                value
            ))
        }
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl Serialize for BudgetTokens {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for BudgetTokens {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        BudgetTokens::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageParam {
    pub role: MessageRole,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<BetaContentBlockParam>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<BetaTextBlockParam>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaOutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<OutputEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputEffort {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaJSONOutputFormat {
    #[serde(rename = "type")]
    pub format_type: JsonOutputFormatType,
    pub schema: HashMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum JsonOutputFormatType {
    #[serde(rename = "json_schema")]
    JsonSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaThinkingConfigParam {
    #[serde(rename = "enabled")]
    Enabled { budget_tokens: BudgetTokens },
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaToolChoice {
    #[serde(rename = "auto")]
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "any")]
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "tool")]
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolUnion {
    Tool(BetaTool),
    Bash20241022(BetaToolBash20241022),
    Bash20250124(BetaToolBash20250124),
    CodeExecution20250522(BetaCodeExecutionTool20250522),
    CodeExecution20250825(BetaCodeExecutionTool20250825),
    ComputerUse20241022(BetaToolComputerUse20241022),
    ComputerUse20250124(BetaToolComputerUse20250124),
    ComputerUse20251124(BetaToolComputerUse20251124),
    Memory20250818(BetaMemoryTool20250818),
    TextEditor20241022(BetaToolTextEditor20241022),
    TextEditor20250124(BetaToolTextEditor20250124),
    TextEditor20250429(BetaToolTextEditor20250429),
    TextEditor20250728(BetaToolTextEditor20250728),
    WebSearch20250305(BetaWebSearchTool20250305),
    WebFetch20250910(BetaWebFetchTool20250910),
    ToolSearchBm2520251119(BetaToolSearchToolBm25_20251119),
    ToolSearchRegex20251119(BetaToolSearchToolRegex20251119),
    McpToolset(BetaMCPToolset),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTool {
    pub input_schema: JsonSchema,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<CustomToolType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomToolType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchema {
    #[serde(rename = "type")]
    pub schema_type: JsonSchemaType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum JsonSchemaType {
    #[serde(rename = "object")]
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AllowedCaller {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolBash20241022 {
    pub name: BashToolName,
    #[serde(rename = "type")]
    pub tool_type: BashToolType20241022,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolBash20250124 {
    pub name: BashToolName,
    #[serde(rename = "type")]
    pub tool_type: BashToolType20250124,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BashToolName {
    #[serde(rename = "bash")]
    Bash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BashToolType20241022 {
    #[serde(rename = "bash_20241022")]
    Bash20241022,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BashToolType20250124 {
    #[serde(rename = "bash_20250124")]
    Bash20250124,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionTool20250522 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub tool_type: CodeExecutionToolType20250522,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionTool20250825 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub tool_type: CodeExecutionToolType20250825,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeExecutionToolName {
    #[serde(rename = "code_execution")]
    CodeExecution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeExecutionToolType20250522 {
    #[serde(rename = "code_execution_20250522")]
    CodeExecution20250522,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeExecutionToolType20250825 {
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolComputerUse20241022 {
    pub display_height_px: u32,
    pub display_width_px: u32,
    pub name: ComputerToolName,
    #[serde(rename = "type")]
    pub tool_type: ComputerToolType20241022,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolComputerUse20250124 {
    pub display_height_px: u32,
    pub display_width_px: u32,
    pub name: ComputerToolName,
    #[serde(rename = "type")]
    pub tool_type: ComputerToolType20250124,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolComputerUse20251124 {
    pub display_height_px: u32,
    pub display_width_px: u32,
    pub name: ComputerToolName,
    #[serde(rename = "type")]
    pub tool_type: ComputerToolType20251124,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_zoom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputerToolName {
    #[serde(rename = "computer")]
    Computer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputerToolType20241022 {
    #[serde(rename = "computer_20241022")]
    Computer20241022,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputerToolType20250124 {
    #[serde(rename = "computer_20250124")]
    Computer20250124,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputerToolType20251124 {
    #[serde(rename = "computer_20251124")]
    Computer20251124,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMemoryTool20250818 {
    pub name: MemoryToolName,
    #[serde(rename = "type")]
    pub tool_type: MemoryToolType20250818,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryToolName {
    #[serde(rename = "memory")]
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryToolType20250818 {
    #[serde(rename = "memory_20250818")]
    Memory20250818,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20241022 {
    pub name: TextEditorToolName,
    #[serde(rename = "type")]
    pub tool_type: TextEditorToolType20241022,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250124 {
    pub name: TextEditorToolName,
    #[serde(rename = "type")]
    pub tool_type: TextEditorToolType20250124,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250429 {
    pub name: TextEditorToolName20250429,
    #[serde(rename = "type")]
    pub tool_type: TextEditorToolType20250429,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250728 {
    pub name: TextEditorToolName20250728,
    #[serde(rename = "type")]
    pub tool_type: TextEditorToolType20250728,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorToolName {
    #[serde(rename = "str_replace_editor")]
    StrReplaceEditor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorToolName20250429 {
    #[serde(rename = "str_replace_based_edit_tool")]
    StrReplaceBasedEditTool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorToolName20250728 {
    #[serde(rename = "str_replace_based_edit_tool")]
    StrReplaceBasedEditTool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorToolType20241022 {
    #[serde(rename = "text_editor_20241022")]
    TextEditor20241022,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorToolType20250124 {
    #[serde(rename = "text_editor_20250124")]
    TextEditor20250124,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorToolType20250429 {
    #[serde(rename = "text_editor_20250429")]
    TextEditor20250429,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorToolType20250728 {
    #[serde(rename = "text_editor_20250728")]
    TextEditor20250728,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchTool20250305 {
    pub name: WebSearchToolName,
    #[serde(rename = "type")]
    pub tool_type: WebSearchToolType20250305,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<BetaUserLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaUserLocation {
    #[serde(rename = "type")]
    pub location_type: UserLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSearchToolName {
    #[serde(rename = "web_search")]
    WebSearch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSearchToolType20250305 {
    #[serde(rename = "web_search_20250305")]
    WebSearch20250305,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchTool20250910 {
    pub name: WebFetchToolName,
    #[serde(rename = "type")]
    pub tool_type: WebFetchToolType20250910,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebFetchToolName {
    #[serde(rename = "web_fetch")]
    WebFetch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebFetchToolType20250910 {
    #[serde(rename = "web_fetch_20250910")]
    WebFetch20250910,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolBm25_20251119 {
    pub name: ToolSearchBm25Name,
    #[serde(rename = "type")]
    pub tool_type: ToolSearchBm25Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSearchBm25Name {
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSearchBm25Type {
    #[serde(rename = "tool_search_tool_bm25_20251119")]
    ToolSearchToolBm2520251119,
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolRegex20251119 {
    pub name: ToolSearchRegexName,
    #[serde(rename = "type")]
    pub tool_type: ToolSearchRegexType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSearchRegexName {
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSearchRegexType {
    #[serde(rename = "tool_search_tool_regex_20251119")]
    ToolSearchToolRegex20251119,
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolset {
    pub mcp_server_name: String,
    #[serde(rename = "type")]
    pub tool_type: MCPToolsetType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<HashMap<String, BetaMCPToolConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_config: Option<BetaMCPToolDefaultConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPToolsetType {
    #[serde(rename = "mcp_toolset")]
    McpToolset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolDefaultConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRequestMCPServerURLDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub server_type: MCPServerType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_configuration: Option<BetaRequestMCPServerToolConfiguration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPServerType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRequestMCPServerToolConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlockParam {
    Text(BetaTextBlockParam),
    Image(BetaImageBlockParam),
    Document(BetaRequestDocumentBlock),
    SearchResult(BetaSearchResultBlockParam),
    Thinking(BetaThinkingBlockParam),
    RedactedThinking(BetaRedactedThinkingBlockParam),
    ToolUse(BetaToolUseBlockParam),
    ToolResult(BetaToolResultBlockParam),
    ServerToolUse(BetaServerToolUseBlockParam),
    WebSearchToolResult(BetaWebSearchToolResultBlockParam),
    WebFetchToolResult(BetaWebFetchToolResultBlockParam),
    CodeExecutionToolResult(BetaCodeExecutionToolResultBlockParam),
    BashCodeExecutionToolResult(BetaBashCodeExecutionToolResultBlockParam),
    TextEditorCodeExecutionToolResult(BetaTextEditorCodeExecutionToolResultBlockParam),
    ToolSearchToolResult(BetaToolSearchToolResultBlockParam),
    MCPToolUse(BetaMCPToolUseBlockParam),
    MCPToolResult(BetaMCPToolResultBlockParam),
    ContainerUpload(BetaContainerUploadBlockParam),
    ToolReference(BetaToolReferenceBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextBlockParam {
    pub text: String,
    #[serde(rename = "type")]
    pub block_type: TextBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<BetaTextCitationParam>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TextBlockType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaImageBlockParam {
    pub source: BetaImageSource,
    #[serde(rename = "type")]
    pub block_type: ImageBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ImageBlockType {
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaImageSource {
    Base64(BetaBase64ImageSource),
    Url(BetaURLImageSource),
    File(BetaFileImageSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBase64ImageSource {
    pub data: String,
    pub media_type: ImageMediaType,
    #[serde(rename = "type")]
    pub source_type: ImageSourceTypeBase64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaURLImageSource {
    #[serde(rename = "type")]
    pub source_type: ImageSourceTypeUrl,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaFileImageSource {
    pub file_id: String,
    #[serde(rename = "type")]
    pub source_type: ImageSourceTypeFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSourceTypeBase64 {
    #[serde(rename = "base64")]
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSourceTypeUrl {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSourceTypeFile {
    #[serde(rename = "file")]
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageMediaType {
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRequestDocumentBlock {
    pub source: BetaDocumentSource,
    #[serde(rename = "type")]
    pub block_type: DocumentBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DocumentBlockType {
    #[serde(rename = "document")]
    Document,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaDocumentSource {
    Base64Pdf(BetaBase64PDFSource),
    PlainText(BetaPlainTextSource),
    Content(BetaContentBlockSource),
    UrlPdf(BetaURLPDFSource),
    File(BetaFileDocumentSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBase64PDFSource {
    pub data: String,
    pub media_type: PdfMediaType,
    #[serde(rename = "type")]
    pub source_type: DocumentSourceTypeBase64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaPlainTextSource {
    pub data: String,
    pub media_type: PlainTextMediaType,
    #[serde(rename = "type")]
    pub source_type: DocumentSourceTypeText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContentBlockSource {
    pub content: ContentBlockSourceContent,
    #[serde(rename = "type")]
    pub source_type: DocumentSourceTypeContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaURLPDFSource {
    #[serde(rename = "type")]
    pub source_type: DocumentSourceTypeUrl,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaFileDocumentSource {
    pub file_id: String,
    #[serde(rename = "type")]
    pub source_type: DocumentSourceTypeFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlockSourceContent {
    Text(String),
    Blocks(Vec<ContentBlockSourceBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlockSourceBlock {
    Text(BetaTextBlockParam),
    Image(BetaImageBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentSourceTypeBase64 {
    #[serde(rename = "base64")]
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentSourceTypeText {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentSourceTypeContent {
    #[serde(rename = "content")]
    Content,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentSourceTypeUrl {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentSourceTypeFile {
    #[serde(rename = "file")]
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PdfMediaType {
    #[serde(rename = "application/pdf")]
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlainTextMediaType {
    #[serde(rename = "text/plain")]
    TextPlain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaSearchResultBlockParam {
    pub content: Vec<BetaTextBlockParam>,
    pub source: String,
    pub title: String,
    #[serde(rename = "type")]
    pub block_type: SearchResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SearchResultBlockType {
    #[serde(rename = "search_result")]
    SearchResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaThinkingBlockParam {
    pub signature: String,
    pub thinking: String,
    #[serde(rename = "type")]
    pub block_type: ThinkingBlockType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ThinkingBlockType {
    #[serde(rename = "thinking")]
    Thinking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRedactedThinkingBlockParam {
    pub data: String,
    #[serde(rename = "type")]
    pub block_type: RedactedThinkingBlockType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RedactedThinkingBlockType {
    #[serde(rename = "redacted_thinking")]
    RedactedThinking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolUseBlockParam {
    pub id: String,
    pub input: HashMap<String, Value>,
    pub name: String,
    #[serde(rename = "type")]
    pub block_type: ToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaCaller>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ToolUseBlockType {
    #[serde(rename = "tool_use")]
    ToolUse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: ToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ToolResultContentParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ToolResultBlockType {
    #[serde(rename = "tool_result")]
    ToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContentParam {
    Text(String),
    Blocks(Vec<ToolResultContentBlockParam>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContentBlockParam {
    Text(BetaTextBlockParam),
    Image(BetaImageBlockParam),
    SearchResult(BetaSearchResultBlockParam),
    Document(BetaRequestDocumentBlock),
    ToolReference(BetaToolReferenceBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaServerToolUseBlockParam {
    pub id: String,
    pub input: HashMap<String, Value>,
    pub name: ServerToolName,
    #[serde(rename = "type")]
    pub block_type: ServerToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaCaller>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ServerToolUseBlockType {
    #[serde(rename = "server_tool_use")]
    ServerToolUse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerToolName {
    #[serde(rename = "web_search")]
    WebSearch,
    #[serde(rename = "web_fetch")]
    WebFetch,
    #[serde(rename = "code_execution")]
    CodeExecution,
    #[serde(rename = "bash_code_execution")]
    BashCodeExecution,
    #[serde(rename = "text_editor_code_execution")]
    TextEditorCodeExecution,
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex,
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaCaller {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution { tool_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultBlockParam {
    pub content: BetaWebSearchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: WebSearchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebSearchToolResultBlockParamContent {
    Results(Vec<BetaWebSearchResultBlockParam>),
    Error(BetaWebSearchToolRequestError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchResultBlockParam {
    pub encrypted_content: String,
    pub title: String,
    #[serde(rename = "type")]
    pub block_type: WebSearchResultBlockType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WebSearchResultBlockType {
    #[serde(rename = "web_search_result")]
    WebSearchResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchToolRequestError {
    pub error_code: BetaWebSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: WebSearchToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "max_uses_exceeded")]
    MaxUsesExceeded,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "query_too_long")]
    QueryTooLong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSearchToolResultErrorType {
    #[serde(rename = "web_search_tool_result_error")]
    WebSearchToolResultError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSearchToolResultBlockType {
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultBlockParam {
    pub content: BetaWebFetchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: WebFetchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebFetchToolResultBlockParamContent {
    Error(BetaWebFetchToolResultErrorBlockParam),
    Result(BetaWebFetchBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultErrorBlockParam {
    pub error_code: BetaWebFetchToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: WebFetchToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "url_too_long")]
    UrlTooLong,
    #[serde(rename = "url_not_allowed")]
    UrlNotAllowed,
    #[serde(rename = "url_not_accessible")]
    UrlNotAccessible,
    #[serde(rename = "unsupported_content_type")]
    UnsupportedContentType,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "max_uses_exceeded")]
    MaxUsesExceeded,
    #[serde(rename = "unavailable")]
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebFetchToolResultErrorType {
    #[serde(rename = "web_fetch_tool_result_error")]
    WebFetchToolResultError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchBlockParam {
    pub content: BetaRequestDocumentBlock,
    #[serde(rename = "type")]
    pub block_type: WebFetchResultType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "time::serde::rfc3339::option")]
    pub retrieved_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WebFetchResultType {
    #[serde(rename = "web_fetch_result")]
    WebFetchResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebFetchToolResultBlockType {
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultBlockParam {
    pub content: BetaCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: CodeExecutionToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaCodeExecutionToolResultBlockParamContent {
    Error(BetaCodeExecutionToolResultErrorParam),
    Result(BetaCodeExecutionResultBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultErrorParam {
    pub error_code: BetaCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: CodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeExecutionToolResultErrorType {
    #[serde(rename = "code_execution_tool_result_error")]
    CodeExecutionToolResultError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionResultBlockParam {
    pub content: Vec<BetaCodeExecutionOutputBlockParam>,
    pub return_code: u32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub block_type: CodeExecutionResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionOutputBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub output_type: CodeExecutionOutputType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CodeExecutionOutputType {
    #[serde(rename = "code_execution_output")]
    CodeExecutionOutput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CodeExecutionResultType {
    #[serde(rename = "code_execution_result")]
    CodeExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeExecutionToolResultBlockType {
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultBlockParam {
    pub content: BetaBashCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: BashCodeExecutionToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaBashCodeExecutionToolResultBlockParamContent {
    Error(BetaBashCodeExecutionToolResultErrorParam),
    Result(BetaBashCodeExecutionResultBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultErrorParam {
    pub error_code: BetaBashCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: BashCodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
    #[serde(rename = "output_file_too_large")]
    OutputFileTooLarge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BashCodeExecutionToolResultErrorType {
    #[serde(rename = "bash_code_execution_tool_result_error")]
    BashCodeExecutionToolResultError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionResultBlockParam {
    pub content: Vec<BetaBashCodeExecutionOutputBlockParam>,
    pub return_code: u32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub block_type: BashCodeExecutionResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionOutputBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub output_type: BashCodeExecutionOutputType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BashCodeExecutionOutputType {
    #[serde(rename = "bash_code_execution_output")]
    BashCodeExecutionOutput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BashCodeExecutionResultType {
    #[serde(rename = "bash_code_execution_result")]
    BashCodeExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BashCodeExecutionToolResultBlockType {
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultBlockParam {
    pub content: BetaTextEditorCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTextEditorCodeExecutionToolResultBlockParamContent {
    Error(BetaTextEditorCodeExecutionToolResultErrorParam),
    View(BetaTextEditorCodeExecutionViewResultBlockParam),
    Create(BetaTextEditorCodeExecutionCreateResultBlockParam),
    StrReplace(BetaTextEditorCodeExecutionStrReplaceResultBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultErrorParam {
    pub error_code: BetaTextEditorCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: TextEditorCodeExecutionToolResultErrorType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
    #[serde(rename = "file_not_found")]
    FileNotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionToolResultErrorType {
    #[serde(rename = "text_editor_code_execution_tool_result_error")]
    TextEditorCodeExecutionToolResultError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionViewResultBlockParam {
    pub content: String,
    pub file_type: TextEditorCodeExecutionFileType,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionViewResultType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionFileType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "pdf")]
    Pdf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionViewResultType {
    #[serde(rename = "text_editor_code_execution_view_result")]
    TextEditorCodeExecutionViewResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionCreateResultBlockParam {
    pub is_file_update: bool,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionCreateResultType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionCreateResultType {
    #[serde(rename = "text_editor_code_execution_create_result")]
    TextEditorCodeExecutionCreateResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionStrReplaceResultBlockParam {
    pub lines: Vec<String>,
    pub new_lines: u32,
    pub new_start: u32,
    pub old_lines: u32,
    pub old_start: u32,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionStrReplaceResultType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionStrReplaceResultType {
    #[serde(rename = "text_editor_code_execution_str_replace_result")]
    TextEditorCodeExecutionStrReplaceResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionToolResultBlockType {
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultBlockParam {
    pub content: BetaToolSearchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: ToolSearchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolSearchToolResultBlockParamContent {
    Error(BetaToolSearchToolResultErrorParam),
    Result(BetaToolSearchToolSearchResultBlockParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultErrorParam {
    pub error_code: BetaToolSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: ToolSearchToolResultErrorType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetaToolSearchToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSearchToolResultErrorType {
    #[serde(rename = "tool_search_tool_result_error")]
    ToolSearchToolResultError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolSearchResultBlockParam {
    pub tool_references: Vec<BetaToolReferenceBlockParam>,
    #[serde(rename = "type")]
    pub block_type: ToolSearchToolSearchResultType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ToolSearchToolSearchResultType {
    #[serde(rename = "tool_search_tool_search_result")]
    ToolSearchToolSearchResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSearchToolResultBlockType {
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolUseBlockParam {
    pub id: String,
    pub input: HashMap<String, Value>,
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub block_type: MCPToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MCPToolUseBlockType {
    #[serde(rename = "mcp_tool_use")]
    MCPToolUse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: MCPToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<MCPToolResultContentParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MCPToolResultContentParam {
    Text(String),
    Blocks(Vec<BetaTextBlockParam>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MCPToolResultBlockType {
    #[serde(rename = "mcp_tool_result")]
    MCPToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContainerUploadBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub block_type: ContainerUploadBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ContainerUploadBlockType {
    #[serde(rename = "container_upload")]
    ContainerUpload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolReferenceBlockParam {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub block_type: ToolReferenceBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ToolReferenceBlockType {
    #[serde(rename = "tool_reference")]
    ToolReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCacheControlEphemeral {
    #[serde(rename = "type")]
    pub cache_type: CacheControlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheControlTtl>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CacheControlType {
    #[serde(rename = "ephemeral")]
    Ephemeral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheControlTtl {
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCitationsConfigParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaTextCitationParam {
    #[serde(rename = "char_location")]
    CharLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_char_index: u32,
        start_char_index: u32,
    },
    #[serde(rename = "page_location")]
    PageLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_page_number: u32,
        start_page_number: u32,
    },
    #[serde(rename = "content_block_location")]
    ContentBlockLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_block_index: u32,
        start_block_index: u32,
    },
    #[serde(rename = "web_search_result_location")]
    WebSearchResultLocation {
        cited_text: String,
        encrypted_index: String,
        title: String,
        url: String,
    },
    #[serde(rename = "search_result_location")]
    SearchResultLocation {
        cited_text: String,
        end_block_index: u32,
        search_result_index: u32,
        source: String,
        start_block_index: u32,
        title: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContextManagementConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<BetaContextManagementEdit>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContextManagementEdit {
    ClearToolUses(BetaClearToolUses20250919Edit),
    ClearThinking(BetaClearThinking20251015Edit),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearToolUses20250919Edit {
    #[serde(rename = "type")]
    pub edit_type: ClearToolUsesEditType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_at_least: Option<BetaInputTokensClearAtLeast>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_tool_inputs: Option<ClearToolInputs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<BetaToolUsesKeep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<BetaContextManagementTrigger>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ClearToolUsesEditType {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaInputTokensClearAtLeast {
    #[serde(rename = "type")]
    pub kind: InputTokensClearAtLeastType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InputTokensClearAtLeastType {
    #[serde(rename = "input_tokens")]
    InputTokens,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClearToolInputs {
    All(bool),
    Selected(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolUsesKeep {
    #[serde(rename = "type")]
    pub kind: ToolUsesKeepType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ToolUsesKeepType {
    #[serde(rename = "tool_uses")]
    ToolUses,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContextManagementTrigger {
    InputTokens(BetaInputTokensTrigger),
    ToolUses(BetaToolUsesTrigger),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaInputTokensTrigger {
    #[serde(rename = "type")]
    pub kind: InputTokensTriggerType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InputTokensTriggerType {
    #[serde(rename = "input_tokens")]
    InputTokens,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolUsesTrigger {
    #[serde(rename = "type")]
    pub kind: ToolUsesTriggerType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ToolUsesTriggerType {
    #[serde(rename = "tool_uses")]
    ToolUses,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearThinking20251015Edit {
    #[serde(rename = "type")]
    pub edit_type: ClearThinkingEditType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<ThinkingKeep>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ClearThinkingEditType {
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThinkingKeep {
    Turns(BetaThinkingTurns),
    AllTurns(BetaAllThinkingTurns),
    AllLiteral(AllLiteral),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaThinkingTurns {
    #[serde(rename = "type")]
    pub kind: ThinkingTurnsType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ThinkingTurnsType {
    #[serde(rename = "thinking_turns")]
    ThinkingTurns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaAllThinkingTurns {
    #[serde(rename = "type")]
    pub kind: AllThinkingTurnsType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AllThinkingTurnsType {
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AllLiteral {
    #[serde(rename = "all")]
    All,
}

impl fmt::Display for BudgetTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
