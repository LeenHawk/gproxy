use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::*;
use super::generate_content::ResponseItem;

pub type CreateConversationWireModel =
    OpenAiWireModel<CreateConversationRequestBody, ConversationObject>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateConversationRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ResponseItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationObject {
    pub id: String,
    pub created_at: u64,
    pub metadata: Metadata,
    pub object: ConversationObjectType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
