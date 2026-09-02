use serde::{Deserialize, Serialize};

use super::common::{ConversationObjectType, Metadata, Rest};
use super::generate_content::ResponseItem;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CreateConversationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ResponseItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub created_at: u64,
    pub metadata: Metadata,
    pub object: ConversationObjectType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

pub type CreateConversationRequestBody = CreateConversationRequest;
pub type ConversationObject = Conversation;
pub type CreateConversationWireModel =
    super::common::OpenAiWireModel<CreateConversationRequest, Conversation>;
