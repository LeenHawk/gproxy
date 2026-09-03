use serde::{Deserialize, Serialize};

use super::common::{ConversationObjectType, Metadata, Rest};
use super::generate_content::ResponseItem;

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, Default, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CreateConversationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ResponseItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
