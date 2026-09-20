//! DeepSeek reasoning_content and OpenRouter reasoning/reasoning_details.
//! Source: upstream_docs/openrouter/Create a chat completion.md, ReasoningDetailUnion.
use super::content::present_nullable;
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
pub enum ReasoningDetailKind {
    #[serde(rename = "reasoning.text")]
    Text,
    #[serde(rename = "reasoning.summary")]
    Summary,
    #[serde(rename = "reasoning.encrypted")]
    Encrypted,
}
/// Payload fields are optional because streaming detail fragments may carry
/// only a signature or identity. Encrypted data is never interpreted as text.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
pub struct ReasoningDetail {
    #[serde(rename = "type")]
    pub type_: ReasoningDetailKind,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub signature: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub fn visible_reasoning(
    content: &Option<Option<String>>,
    reasoning: &Option<Option<String>>,
    details: &Option<Option<Vec<ReasoningDetail>>>,
) -> Option<String> {
    if let Some(text) = super::reasoning_text(content, reasoning).filter(|s| !s.is_empty()) {
        return Some(text.into());
    }
    let texts: Vec<_> = details
        .as_ref()
        .and_then(Option::as_ref)
        .into_iter()
        .flatten()
        .filter_map(|d| match d.type_ {
            ReasoningDetailKind::Text => d.text.as_ref().and_then(Option::as_deref),
            ReasoningDetailKind::Summary => d.summary.as_deref(),
            ReasoningDetailKind::Encrypted => None,
        })
        .collect();
    if texts.is_empty() {
        super::reasoning_text(content, reasoning).map(str::to_owned)
    } else {
        Some(texts.join(""))
    }
}
