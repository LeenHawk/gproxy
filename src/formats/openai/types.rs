use serde::{Deserialize, Serialize};
use serde_json::Number;
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonValue {
    Null(()),
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

pub type JsonSchema = HashMap<String, JsonValue>;
pub type Metadata = HashMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub created: i64,
    #[serde(rename = "object")]
    pub object_type: ModelObjectType,
    pub owned_by: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ModelObjectType {
    #[serde(rename = "model")]
    Model,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    Auto,
    Default,
    Flex,
    Scale,
    Priority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptCacheRetention {
    InMemory,
    #[serde(rename = "24h")]
    Hours24,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Truncation {
    Auto,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSummary {
    Auto,
    Concise,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<ReasoningSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageDetail {
    Low,
    High,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContent {
    InputText {
        text: String,
    },
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        detail: ImageDetail,
    },
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallOutputContentParam {
    InputText {
        #[validate(max_length = 10_485_760)]
        text: String,
    },
    InputImage {
        #[validate(max_length = 20_971_520)]
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<ImageDetail>,
    },
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[validate(max_length = 33_554_432)]
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum FunctionCallOutputParam {
    Text(#[validate(max_length = 10_485_760)] String),
    Parts(#[validate] Vec<ToolCallOutputContentParam>),
}

pub type InputMessageContentList = Vec<InputContent>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EasyInputMessageContent {
    Text(String),
    Parts(InputMessageContentList),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EasyInputMessage {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<EasyInputMessageType>,
    pub role: EasyInputMessageRole,
    pub content: EasyInputMessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EasyInputMessageType {
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EasyInputMessageRole {
    User,
    Assistant,
    System,
    Developer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    InProgress,
    Completed,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMessage {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<InputMessageType>,
    pub role: InputMessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
    pub content: InputMessageContentList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMessageType {
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMessageRole {
    User,
    System,
    Developer,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum InputParam {
    Text(String),
    Items(#[validate] Vec<InputItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum InputItem {
    EasyMessage(EasyInputMessage),
    Item(#[validate] Item),
    Reference(ItemReferenceParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemReferenceParam {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<ItemReferenceType>,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemReferenceType {
    #[serde(rename = "item_reference")]
    ItemReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConversationParam {
    Id(String),
    Object(ConversationParamObject),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationParamObject {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<ResponsePromptVariables>,
}

pub type ResponsePromptVariables = HashMap<String, PromptVariableValue>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptVariableValue {
    Text(String),
    Input(InputContent),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResponseTextParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub format: Option<TextResponseFormatConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseStreamOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextResponseFormatConfiguration {
    Text,
    JsonObject,
    JsonSchema {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[validate(min_length = 1)]
        #[validate(max_length = 64)]
        #[validate(custom = validate_name)]
        name: String,
        schema: JsonSchema,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoiceParam {
    Mode(ToolChoiceMode),
    Allowed(ToolChoiceAllowed),
    Hosted(ToolChoiceTypes),
    Function(ToolChoiceFunction),
    MCP(ToolChoiceMCP),
    Custom(ToolChoiceCustom),
    ApplyPatch(SpecificApplyPatchParam),
    Shell(SpecificFunctionShellParam),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoiceMode {
    None,
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceAllowed {
    #[serde(rename = "type")]
    pub choice_type: ToolChoiceAllowedType,
    pub mode: ToolChoiceAllowedMode,
    pub tools: Vec<AllowedToolDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoiceAllowedType {
    #[serde(rename = "allowed_tools")]
    AllowedTools,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoiceAllowedMode {
    Auto,
    Required,
}

pub type AllowedToolDefinition = HashMap<String, JsonValue>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceTypes {
    #[serde(rename = "type")]
    pub tool_type: ToolChoiceHostedTool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoiceHostedTool {
    #[serde(rename = "file_search")]
    FileSearch,
    #[serde(rename = "web_search_preview")]
    WebSearchPreview,
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
    #[serde(rename = "image_generation")]
    ImageGeneration,
    #[serde(rename = "code_interpreter")]
    CodeInterpreter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceFunction {
    #[serde(rename = "type")]
    pub choice_type: ToolChoiceFunctionType,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoiceFunctionType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceMCP {
    #[serde(rename = "type")]
    pub choice_type: ToolChoiceMCPType,
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoiceMCPType {
    #[serde(rename = "mcp")]
    MCP,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceCustom {
    #[serde(rename = "type")]
    pub choice_type: ToolChoiceCustomType,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoiceCustomType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificApplyPatchParam {
    #[serde(rename = "type")]
    pub choice_type: SpecificApplyPatchType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpecificApplyPatchType {
    #[serde(rename = "apply_patch")]
    ApplyPatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificFunctionShellParam {
    #[serde(rename = "type")]
    pub choice_type: SpecificFunctionShellType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpecificFunctionShellType {
    #[serde(rename = "shell")]
    Shell,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    Function(FunctionTool),
    FileSearch(#[validate] FileSearchTool),
    ComputerUsePreview(ComputerUsePreviewTool),
    #[serde(rename = "web_search", alias = "web_search_2025_08_26")]
    WebSearch(WebSearchTool),
    MCP(MCPTool),
    CodeInterpreter(#[validate] CodeInterpreterTool),
    #[serde(rename = "image_generation")]
    ImageGeneration(#[validate] ImageGenTool),
    #[serde(rename = "local_shell")]
    LocalShell,
    #[serde(rename = "shell")]
    Shell,
    Custom(CustomToolParam),
    #[serde(rename = "web_search_preview", alias = "web_search_preview_2025_03_11")]
    WebSearchPreview(WebSearchPreviewTool),
    #[serde(rename = "apply_patch")]
    ApplyPatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Option<JsonSchema>,
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FileSearchTool {
    pub vector_store_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 1)]
    #[validate(maximum = 50)]
    pub max_num_results: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub ranking_options: Option<RankingOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Filters>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RankingOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranker: Option<RankerVersionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0.0)]
    #[validate(maximum = 1.0)]
    pub score_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hybrid_search: Option<HybridSearchOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchOptions {
    pub embedding_weight: f64,
    pub text_weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RankerVersionType {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default-2024-11-15")]
    Default20241115,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Filters {
    Comparison(ComparisonFilter),
    Compound(CompoundFilter),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonFilter {
    #[serde(rename = "type")]
    pub operator: ComparisonOperator,
    pub key: String,
    pub value: ComparisonFilterValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComparisonFilterValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Items(Vec<ComparisonFilterValueItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComparisonFilterValueItem {
    Text(String),
    Number(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompoundFilter {
    #[serde(rename = "type")]
    pub operator: CompoundFilterType,
    pub filters: Vec<Filters>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompoundFilterType {
    And,
    Or,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerUsePreviewTool {
    pub environment: ComputerEnvironment,
    pub display_width: i64,
    pub display_height: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerEnvironment {
    Windows,
    Mac,
    Linux,
    Ubuntu,
    Browser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<WebSearchFilters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<ApproximateLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<WebSearchContextSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchPreviewTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<ApproximateLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<WebSearchContextSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproximateLocation {
    #[serde(rename = "type")]
    pub location_type: ApproximateLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApproximateLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchContextSize {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPTool {
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<MCPConnectorId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<MCPAllowedTools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<MCPRequireApproval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPConnectorId {
    #[serde(rename = "connector_dropbox")]
    ConnectorDropbox,
    #[serde(rename = "connector_gmail")]
    ConnectorGmail,
    #[serde(rename = "connector_googlecalendar")]
    ConnectorGoogleCalendar,
    #[serde(rename = "connector_googledrive")]
    ConnectorGoogleDrive,
    #[serde(rename = "connector_microsoftteams")]
    ConnectorMicrosoftTeams,
    #[serde(rename = "connector_outlookcalendar")]
    ConnectorOutlookCalendar,
    #[serde(rename = "connector_outlookemail")]
    ConnectorOutlookEmail,
    #[serde(rename = "connector_sharepoint")]
    ConnectorSharepoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MCPAllowedTools {
    List(Vec<String>),
    Filter(MCPToolFilter),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MCPRequireApproval {
    Mode(MCPApprovalMode),
    Filters(MCPApprovalFilters),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MCPApprovalMode {
    Always,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPApprovalFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always: Option<MCPToolFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub never: Option<MCPToolFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPToolFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CodeInterpreterTool {
    #[validate]
    pub container: CodeInterpreterContainer,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum CodeInterpreterContainer {
    Id(String),
    Auto(#[validate] CodeInterpreterContainerAuto),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CodeInterpreterContainerAuto {
    #[serde(rename = "type")]
    pub container_type: CodeInterpreterContainerType,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(max_items = 50)]
    pub file_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<ContainerMemoryLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeInterpreterContainerType {
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContainerMemoryLimit {
    #[serde(rename = "1g")]
    OneGig,
    #[serde(rename = "4g")]
    FourGig,
    #[serde(rename = "16g")]
    SixteenGig,
    #[serde(rename = "64g")]
    SixtyFourGig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ImageGenTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageGenQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageGenSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ImageGenOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0)]
    #[validate(maximum = 100)]
    pub output_compression: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ImageGenModeration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageGenBackground>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_fidelity: Option<InputFidelity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_image_mask: Option<ImageGenInputImageMask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0)]
    #[validate(maximum = 3)]
    pub partial_images: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageGenQuality {
    Low,
    Medium,
    High,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageGenSize {
    #[serde(rename = "1024x1024")]
    Size1024x1024,
    #[serde(rename = "1024x1536")]
    Size1024x1536,
    #[serde(rename = "1536x1024")]
    Size1536x1024,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageGenOutputFormat {
    Png,
    Webp,
    Jpeg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageGenModeration {
    Auto,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageGenBackground {
    Transparent,
    Opaque,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputFidelity {
    High,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenInputImageMask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomToolParam {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CustomToolFormat {
    Text,
    Grammar {
        definition: String,
        syntax: GrammarSyntax,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarSyntax {
    Lark,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputContent {
    OutputText(OutputTextContent),
    Refusal(RefusalContent),
    ReasoningText(ReasoningTextContent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputTextContent {
    #[serde(rename = "type")]
    pub content_type: OutputTextContentType,
    pub text: String,
    pub annotations: Vec<Annotation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<LogProb>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputTextContentType {
    #[serde(rename = "output_text")]
    OutputText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefusalContent {
    #[serde(rename = "type")]
    pub content_type: RefusalContentType,
    pub refusal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefusalContentType {
    #[serde(rename = "refusal")]
    Refusal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTextContent {
    #[serde(rename = "type")]
    pub content_type: ReasoningTextContentType,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReasoningTextContentType {
    #[serde(rename = "reasoning_text")]
    ReasoningText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    #[serde(rename = "type")]
    pub summary_type: SummaryType,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SummaryType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: OutputMessageType,
    pub role: OutputMessageRole,
    pub content: Vec<OutputMessageContent>,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputMessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputMessageContent {
    OutputText(OutputTextContent),
    Refusal(RefusalContent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Annotation {
    FileCitation(FileCitationBody),
    UrlCitation(UrlCitationBody),
    ContainerFileCitation(ContainerFileCitationBody),
    FilePath(FilePath),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCitationBody {
    #[serde(rename = "type")]
    pub annotation_type: FileCitationType,
    pub file_id: String,
    pub index: i64,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileCitationType {
    #[serde(rename = "file_citation")]
    FileCitation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlCitationBody {
    #[serde(rename = "type")]
    pub annotation_type: UrlCitationType,
    pub url: String,
    pub start_index: i64,
    pub end_index: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UrlCitationType {
    #[serde(rename = "url_citation")]
    UrlCitation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerFileCitationBody {
    #[serde(rename = "type")]
    pub annotation_type: ContainerFileCitationType,
    pub container_id: String,
    pub file_id: String,
    pub start_index: i64,
    pub end_index: i64,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContainerFileCitationType {
    #[serde(rename = "container_file_citation")]
    ContainerFileCitation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePath {
    #[serde(rename = "type")]
    pub path_type: FilePathType,
    pub file_id: String,
    pub index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilePathType {
    #[serde(rename = "file_path")]
    FilePath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopLogProb {
    pub token: String,
    pub logprob: f64,
    pub bytes: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogProb {
    pub token: String,
    pub logprob: f64,
    pub bytes: Vec<i64>,
    pub top_logprobs: Vec<TopLogProb>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSearchToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: FileSearchToolCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<FileSearchCallStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<FileSearchResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileSearchToolCallType {
    #[serde(rename = "file_search_call")]
    FileSearchCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSearchCallStatus {
    InProgress,
    Searching,
    Completed,
    Incomplete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSearchResult {
    pub file_id: String,
    pub text: String,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<VectorStoreFileAttributes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

pub type VectorStoreFileAttributes = HashMap<String, VectorStoreFileAttributeValue>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VectorStoreFileAttributeValue {
    Text(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: WebSearchToolCallType,
    pub status: WebSearchCallStatus,
    pub action: WebSearchAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSearchToolCallType {
    #[serde(rename = "web_search_call")]
    WebSearchCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchCallStatus {
    InProgress,
    Searching,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSearchAction {
    Search {
        query: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sources: Option<Vec<WebSearchSource>>,
    },
    OpenPage {
        url: String,
    },
    Find {
        url: String,
        pattern: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchSource {
    #[serde(rename = "type")]
    pub source_type: WebSearchSourceType,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSearchSourceType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerToolCall {
    #[serde(rename = "type")]
    pub call_type: ComputerToolCallType,
    pub id: String,
    pub call_id: String,
    pub action: ComputerAction,
    pub pending_safety_checks: Vec<ComputerCallSafetyCheckParam>,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputerToolCallType {
    #[serde(rename = "computer_call")]
    ComputerCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComputerAction {
    Click {
        button: ClickButtonType,
        x: i64,
        y: i64,
    },
    #[serde(rename = "double_click")]
    DoubleClick {
        x: i64,
        y: i64,
    },
    Drag {
        path: Vec<DragPoint>,
    },
    Keypress {
        keys: Vec<String>,
    },
    Move {
        x: i64,
        y: i64,
    },
    Screenshot,
    Scroll {
        x: i64,
        y: i64,
        scroll_x: i64,
        scroll_y: i64,
    },
    Type {
        text: String,
    },
    Wait,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickButtonType {
    Left,
    Right,
    Wheel,
    Back,
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DragPoint {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerCallSafetyCheckParam {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerScreenshotImage {
    #[serde(rename = "type")]
    pub image_type: ComputerScreenshotImageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputerScreenshotImageType {
    #[serde(rename = "computer_screenshot")]
    ComputerScreenshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ComputerToolCallOutput {
    #[serde(rename = "type")]
    pub output_type: ComputerToolCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    pub call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_safety_checks: Option<Vec<ComputerCallSafetyCheckParam>>,
    pub output: ComputerScreenshotImage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputerToolCallOutputType {
    #[serde(rename = "computer_call_output")]
    ComputerCallOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionToolCall {
    #[serde(rename = "type")]
    pub call_type: FunctionToolCallType,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionToolCallType {
    #[serde(rename = "function_call")]
    FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolCallOutput {
    Text(String),
    Parts(Vec<InputContent>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionToolCallOutput {
    #[serde(rename = "type")]
    pub output_type: FunctionToolCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: ToolCallOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionToolCallOutputType {
    #[serde(rename = "function_call_output")]
    FunctionCallOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FunctionCallOutputItemParam {
    #[serde(rename = "type")]
    pub output_type: FunctionToolCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    pub call_id: String,
    #[validate]
    pub output: FunctionCallOutputParam,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningItem {
    #[serde(rename = "type")]
    pub item_type: ReasoningItemType,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    pub summary: Vec<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ReasoningTextContent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReasoningItemType {
    #[serde(rename = "reasoning")]
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompactionSummaryItemParam {
    #[serde(rename = "type")]
    pub item_type: CompactionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(max_length = 10_485_760)]
    pub encrypted_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionBody {
    #[serde(rename = "type")]
    pub item_type: CompactionType,
    pub id: String,
    pub encrypted_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactionType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenToolCall {
    #[serde(rename = "type")]
    pub call_type: ImageGenToolCallType,
    pub id: String,
    pub status: ImageGenCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageGenToolCallType {
    #[serde(rename = "image_generation_call")]
    ImageGenerationCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageGenCallStatus {
    InProgress,
    Completed,
    Generating,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeInterpreterToolCall {
    #[serde(rename = "type")]
    pub call_type: CodeInterpreterToolCallType,
    pub id: String,
    pub status: CodeInterpreterCallStatus,
    pub container_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<CodeInterpreterOutput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeInterpreterToolCallType {
    #[serde(rename = "code_interpreter_call")]
    CodeInterpreterCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeInterpreterCallStatus {
    InProgress,
    Completed,
    Incomplete,
    Interpreting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeInterpreterOutput {
    Logs(CodeInterpreterOutputLogs),
    Image(CodeInterpreterOutputImage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeInterpreterOutputLogs {
    #[serde(rename = "type")]
    pub output_type: CodeInterpreterOutputLogsType,
    pub logs: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeInterpreterOutputLogsType {
    #[serde(rename = "logs")]
    Logs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeInterpreterOutputImage {
    #[serde(rename = "type")]
    pub output_type: CodeInterpreterOutputImageType,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeInterpreterOutputImageType {
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellExecAction {
    #[serde(rename = "type")]
    pub action_type: LocalShellExecActionType,
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    pub env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LocalShellExecActionType {
    #[serde(rename = "exec")]
    Exec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellToolCall {
    #[serde(rename = "type")]
    pub call_type: LocalShellToolCallType,
    pub id: String,
    pub call_id: String,
    pub action: LocalShellExecAction,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LocalShellToolCallType {
    #[serde(rename = "local_shell_call")]
    LocalShellCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellToolCallOutput {
    #[serde(rename = "type")]
    pub output_type: LocalShellToolCallOutputType,
    pub id: String,
    pub call_id: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LocalShellToolCallOutputType {
    #[serde(rename = "local_shell_call_output")]
    LocalShellCallOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionShellAction {
    pub commands: Vec<String>,
    pub timeout_ms: Option<i64>,
    pub max_output_length: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionShellCall {
    #[serde(rename = "type")]
    pub call_type: FunctionShellCallType,
    pub id: String,
    pub call_id: String,
    pub action: FunctionShellAction,
    pub status: MessageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionShellCallType {
    #[serde(rename = "shell_call")]
    ShellCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionShellCallOutcome {
    Timeout,
    Exit { exit_code: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionShellCallOutputContent {
    pub stdout: String,
    pub stderr: String,
    pub outcome: FunctionShellCallOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionShellCallOutput {
    #[serde(rename = "type")]
    pub output_type: FunctionShellCallOutputType,
    pub id: String,
    pub call_id: String,
    pub output: Vec<FunctionShellCallOutputContent>,
    pub max_output_length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionShellCallOutputType {
    #[serde(rename = "shell_call_output")]
    ShellCallOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionShellActionParam {
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FunctionShellCallItemParam {
    #[serde(rename = "type")]
    pub call_type: FunctionShellCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    pub call_id: String,
    pub action: FunctionShellActionParam,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FunctionShellCallOutputContentParam {
    #[validate(max_length = 10_485_760)]
    pub stdout: String,
    #[validate(max_length = 10_485_760)]
    pub stderr: String,
    pub outcome: FunctionShellCallOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FunctionShellCallOutputItemParam {
    #[serde(rename = "type")]
    pub output_type: FunctionShellCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    pub call_id: String,
    #[validate]
    pub output: Vec<FunctionShellCallOutputContentParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchToolCall {
    #[serde(rename = "type")]
    pub call_type: ApplyPatchToolCallType,
    pub id: String,
    pub call_id: String,
    pub status: ApplyPatchCallStatus,
    pub operation: ApplyPatchOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApplyPatchToolCallType {
    #[serde(rename = "apply_patch_call")]
    ApplyPatchCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPatchCallStatus {
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApplyPatchOperation {
    CreateFile {
        #[validate(min_length = 1)]
        path: String,
        #[validate(max_length = 10_485_760)]
        diff: String,
    },
    DeleteFile {
        #[validate(min_length = 1)]
        path: String,
    },
    UpdateFile {
        #[validate(min_length = 1)]
        path: String,
        #[validate(max_length = 10_485_760)]
        diff: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchToolCallOutput {
    #[serde(rename = "type")]
    pub output_type: ApplyPatchToolCallOutputType,
    pub id: String,
    pub call_id: String,
    pub status: ApplyPatchCallOutputStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApplyPatchToolCallOutputType {
    #[serde(rename = "apply_patch_call_output")]
    ApplyPatchCallOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPatchCallOutputStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApplyPatchToolCallItemParam {
    #[serde(rename = "type")]
    pub call_type: ApplyPatchToolCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    pub call_id: String,
    pub status: ApplyPatchCallStatus,
    #[validate]
    pub operation: ApplyPatchOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApplyPatchToolCallOutputItemParam {
    #[serde(rename = "type")]
    pub output_type: ApplyPatchToolCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    pub call_id: String,
    pub status: ApplyPatchCallOutputStatus,
    #[validate(max_length = 10_485_760)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPListTools {
    #[serde(rename = "type")]
    pub item_type: MCPListToolsType,
    pub id: String,
    pub server_label: String,
    pub tools: Vec<MCPListToolsTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPListToolsType {
    #[serde(rename = "mcp_list_tools")]
    MCPListTools,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPListToolsTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: JsonSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<JsonSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPApprovalRequest {
    #[serde(rename = "type")]
    pub item_type: MCPApprovalRequestType,
    pub id: String,
    pub server_label: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPApprovalRequestType {
    #[serde(rename = "mcp_approval_request")]
    MCPApprovalRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPApprovalResponse {
    #[serde(rename = "type")]
    pub item_type: MCPApprovalResponseType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub approval_request_id: String,
    pub approve: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPApprovalResponseType {
    #[serde(rename = "mcp_approval_response")]
    MCPApprovalResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPToolCall {
    #[serde(rename = "type")]
    pub call_type: MCPToolCallType,
    pub id: String,
    pub server_label: String,
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MCPToolCallStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPToolCallType {
    #[serde(rename = "mcp_call")]
    MCPCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MCPToolCallStatus {
    InProgress,
    Completed,
    Incomplete,
    Calling,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomToolCall {
    #[serde(rename = "type")]
    pub call_type: CustomToolCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub name: String,
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomToolCallType {
    #[serde(rename = "custom_tool_call")]
    CustomToolCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomToolCallOutput {
    #[serde(rename = "type")]
    pub output_type: CustomToolCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: ToolCallOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomToolCallOutputType {
    #[serde(rename = "custom_tool_call_output")]
    CustomToolCallOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum Item {
    InputMessage(InputMessage),
    OutputMessage(OutputMessage),
    FileSearchToolCall(FileSearchToolCall),
    ComputerToolCall(ComputerToolCall),
    ComputerToolCallOutput(#[validate] ComputerToolCallOutput),
    WebSearchToolCall(WebSearchToolCall),
    FunctionToolCall(FunctionToolCall),
    FunctionCallOutputItem(#[validate] FunctionCallOutputItemParam),
    ReasoningItem(ReasoningItem),
    CompactionSummaryItem(#[validate] CompactionSummaryItemParam),
    ImageGenToolCall(ImageGenToolCall),
    CodeInterpreterToolCall(CodeInterpreterToolCall),
    LocalShellToolCall(LocalShellToolCall),
    LocalShellToolCallOutput(LocalShellToolCallOutput),
    FunctionShellCallItem(#[validate] FunctionShellCallItemParam),
    FunctionShellCallOutputItem(#[validate] FunctionShellCallOutputItemParam),
    ApplyPatchToolCallItem(#[validate] ApplyPatchToolCallItemParam),
    ApplyPatchToolCallOutputItem(#[validate] ApplyPatchToolCallOutputItemParam),
    MCPListTools(MCPListTools),
    MCPApprovalRequest(MCPApprovalRequest),
    MCPApprovalResponse(MCPApprovalResponse),
    MCPToolCall(MCPToolCall),
    CustomToolCall(CustomToolCall),
    CustomToolCallOutput(CustomToolCallOutput),
}

fn validate_name(name: &str) -> Result<(), ValidationError> {
    if name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(ValidationError::Custom(
            "name must match [A-Za-z0-9_-] only".to_string(),
        ))
    }
}
