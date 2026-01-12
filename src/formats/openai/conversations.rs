use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;

use super::responses::SummaryTextContent;
use super::types::{
    ApplyPatchToolCall, ApplyPatchToolCallOutput, CodeInterpreterToolCall, ComputerScreenshotImage,
    ComputerToolCall, ComputerToolCallOutput, CustomToolCall, CustomToolCallOutput,
    FileSearchToolCall, FunctionShellCall, FunctionShellCallOutput, FunctionToolCall,
    FunctionToolCallOutput, ImageGenToolCall, InputContent, InputItem, LocalShellToolCall,
    LocalShellToolCallOutput, MCPApprovalRequest, MCPApprovalResponse, MCPListTools, MCPToolCall,
    MessageStatus, Metadata, OutputTextContent, ReasoningItem, ReasoningTextContent, RefusalContent,
    WebSearchToolCall,
};

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(custom = validate_metadata_optional)]
pub struct CreateConversationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    #[validate(max_items = 20)]
    pub items: Option<Vec<InputItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(custom = validate_metadata_required)]
pub struct UpdateConversationRequest {
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateConversationItemsRequest {
    #[validate]
    #[validate(max_items = 20)]
    pub items: Vec<InputItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationResource {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: ConversationObjectType,
    pub metadata: Metadata,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConversationObjectType {
    #[serde(rename = "conversation")]
    Conversation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedConversationResource {
    #[serde(rename = "object")]
    pub object_type: DeletedConversationObjectType,
    pub deleted: bool,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeletedConversationObjectType {
    #[serde(rename = "conversation.deleted")]
    ConversationDeleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationItemList {
    #[serde(rename = "object")]
    pub object_type: ConversationListObjectType,
    pub data: Vec<ConversationItem>,
    pub has_more: bool,
    pub first_id: String,
    pub last_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConversationListObjectType {
    #[serde(rename = "list")]
    List,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConversationItem {
    Message(Message),
    FunctionToolCall(FunctionToolCall),
    FunctionToolCallOutput(FunctionToolCallOutput),
    FileSearchToolCall(FileSearchToolCall),
    WebSearchToolCall(WebSearchToolCall),
    ImageGenToolCall(ImageGenToolCall),
    ComputerToolCall(ComputerToolCall),
    ComputerToolCallOutput(ComputerToolCallOutput),
    ReasoningItem(ReasoningItem),
    CodeInterpreterToolCall(CodeInterpreterToolCall),
    LocalShellToolCall(LocalShellToolCall),
    LocalShellToolCallOutput(LocalShellToolCallOutput),
    FunctionShellCall(FunctionShellCall),
    FunctionShellCallOutput(FunctionShellCallOutput),
    ApplyPatchToolCall(ApplyPatchToolCall),
    ApplyPatchToolCallOutput(ApplyPatchToolCallOutput),
    MCPListTools(MCPListTools),
    MCPApprovalRequest(MCPApprovalRequest),
    MCPApprovalResponse(MCPApprovalResponse),
    MCPToolCall(MCPToolCall),
    CustomToolCall(CustomToolCall),
    CustomToolCallOutput(CustomToolCallOutput),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub item_type: MessageType,
    pub id: String,
    pub status: MessageStatus,
    pub role: MessageRole,
    pub content: Vec<MessageContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    Unknown,
    User,
    Assistant,
    System,
    Critic,
    Discriminator,
    Developer,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Input(InputContent),
    OutputText(OutputTextContent),
    Text(TextContent),
    SummaryText(SummaryTextContent),
    ReasoningText(ReasoningTextContent),
    Refusal(RefusalContent),
    ComputerScreenshot(ComputerScreenshotImage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    #[serde(rename = "type")]
    pub content_type: TextContentType,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextContentType {
    #[serde(rename = "text")]
    Text,
}

fn validate_metadata_optional(req: &CreateConversationRequest) -> Result<(), ValidationError> {
    validate_metadata(req.metadata.as_ref())
}

fn validate_metadata_required(req: &UpdateConversationRequest) -> Result<(), ValidationError> {
    validate_metadata(Some(&req.metadata))
}

fn validate_metadata(metadata: Option<&Metadata>) -> Result<(), ValidationError> {
    let Some(metadata) = metadata else {
        return Ok(());
    };

    if metadata.len() > 16 {
        return Err(ValidationError::Custom(
            "metadata must not contain more than 16 entries".to_string(),
        ));
    }
    for (key, value) in metadata {
        if key.len() > 64 {
            return Err(ValidationError::Custom(
                "metadata keys must be at most 64 characters".to_string(),
            ));
        }
        if value.len() > 512 {
            return Err(ValidationError::Custom(
                "metadata values must be at most 512 characters".to_string(),
            ));
        }
    }

    Ok(())
}
