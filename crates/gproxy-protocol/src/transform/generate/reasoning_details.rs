//! Formal OpenRouter reasoning envelopes. `format` selects a wire representation,
//! not a cryptographic verifier: callers must route replay to the original model.
use crate::{
    transform::TransformError,
    wire::{
        claude::content as c,
        openai::{
            chat::{ReasoningDetail as Detail, ReasoningDetailKind as Kind},
            responses::input as r,
        },
    },
};

pub(crate) const OPENAI: &str = "openai-responses-v1";
pub(crate) const CLAUDE: &str = "anthropic-claude-v1";

fn detail(kind: Kind, format: &str, index: i64, id: Option<String>) -> Detail {
    Detail {
        type_: kind,
        format: Some(format.into()),
        index: Some(index),
        id: id.map(Some),
        text: None,
        summary: None,
        data: None,
        signature: None,
        rest: Default::default(),
    }
}
pub(crate) fn from_responses(item: &r::ReasoningItem, index: i64) -> Vec<Detail> {
    let mut out = Vec::new();
    for part in &item.summary {
        let mut d = detail(Kind::Summary, OPENAI, index, Some(item.id.clone()));
        d.summary = Some(part.text.clone());
        out.push(d);
    }
    for part in item.content.iter().flatten() {
        let mut d = detail(Kind::Text, OPENAI, index, Some(item.id.clone()));
        d.text = Some(Some(part.text.clone()));
        out.push(d);
    }
    if let Some(Some(data)) = &item.encrypted_content {
        let mut d = detail(Kind::Encrypted, OPENAI, index, Some(item.id.clone()));
        d.data = Some(data.clone());
        out.push(d);
    }
    for (offset, detail) in out.iter_mut().enumerate() {
        detail.index = Some(index + offset as i64);
    }
    out
}
pub(crate) fn from_thinking(block: &c::ThinkingBlock, index: i64) -> Detail {
    let mut d = detail(Kind::Text, CLAUDE, index, None);
    d.text = Some(Some(block.thinking.clone()));
    d.signature = Some(Some(block.signature.clone()));
    d
}
pub(crate) fn from_redacted(block: &c::RedactedThinkingBlock, index: i64) -> Detail {
    let mut d = detail(Kind::Encrypted, CLAUDE, index, None);
    d.data = Some(block.data.clone());
    d
}

/// Restore only matching format tags. Never place Claude ciphertext in a
/// Responses encrypted_content field, or the reverse.
pub(crate) fn to_responses(details: &[Detail]) -> Result<Vec<r::ReasoningItem>, TransformError> {
    let mut out: Vec<r::ReasoningItem> = Vec::new();
    for d in details
        .iter()
        .filter(|d| d.format.as_deref() == Some(OPENAI))
    {
        let Some(id) =
            d.id.as_ref()
                .and_then(Option::as_ref)
                .filter(|id| !id.is_empty())
        else {
            if d.type_ == Kind::Encrypted {
                return Err(TransformError::missing_metadata("reasoning_details.id"));
            }
            continue;
        };
        let index = match out.iter().position(|item| &item.id == id) {
            Some(i) => i,
            None => {
                out.push(
                    r::ReasoningItem::builder(
                        r::ReasoningItemType::ReasoningItem,
                        id.clone(),
                        Vec::new(),
                    )
                    .build(),
                );
                out.len() - 1
            }
        };
        let item = &mut out[index];
        match d.type_ {
            Kind::Summary => {
                if let Some(text) = &d.summary {
                    item.summary.push(
                        r::SummaryText::builder(r::SummaryTextType::SummaryText, text.clone())
                            .build(),
                    );
                }
            }
            Kind::Text => {
                if let Some(Some(text)) = &d.text {
                    item.content.get_or_insert_default().push(
                        r::ReasoningContent::builder(
                            r::ReasoningTextType::ReasoningText,
                            text.clone(),
                        )
                        .build(),
                    );
                }
            }
            Kind::Encrypted => {
                if let Some(data) = &d.data {
                    if item
                        .encrypted_content
                        .as_ref()
                        .and_then(Option::as_ref)
                        .is_some_and(|v| v != data)
                    {
                        return Err(TransformError::shape(
                            "reasoning_details.data",
                            "conflicting ciphertext for one reasoning item",
                        ));
                    }
                    item.encrypted_content = Some(Some(data.clone()));
                }
            }
        }
    }
    Ok(out)
}
pub(crate) fn to_claude(details: &[Detail]) -> Vec<c::ContentBlock> {
    details
        .iter()
        .filter(|d| d.format.as_deref() == Some(CLAUDE))
        .filter_map(|d| match d.type_ {
            Kind::Text => d.text.as_ref().and_then(Option::as_ref).map(|text| {
                c::ContentBlock::Thinking(
                    c::ThinkingBlock::builder(
                        c::ThinkingBlockType::Tag,
                        d.signature.clone().flatten().unwrap_or_default(),
                        text.clone(),
                    )
                    .build(),
                )
            }),
            Kind::Encrypted => d.data.as_ref().map(|data| {
                c::ContentBlock::RedactedThinking(
                    c::RedactedThinkingBlock::builder(
                        c::RedactedThinkingBlockType::Tag,
                        data.clone(),
                    )
                    .build(),
                )
            }),
            Kind::Summary => None,
        })
        .collect()
}

/// Streaming text/summary fragments append. Opaque payloads and signatures are
/// snapshots: repeated identical values must not corrupt them by concatenation.
pub(crate) fn merge(
    target: &mut Vec<Detail>,
    fragments: Vec<Detail>,
) -> Result<(), TransformError> {
    for d in fragments {
        let existing = target.iter_mut().find(|v| {
            v.type_ == d.type_
                && v.format == d.format
                && ((d.index.is_some() && v.index == d.index)
                    || (d.index.is_none()
                        && d.id.as_ref().and_then(Option::as_ref).is_some()
                        && v.id == d.id))
        });
        if let Some(v) = existing {
            if v.id
                .as_ref()
                .and_then(Option::as_ref)
                .zip(d.id.as_ref().and_then(Option::as_ref))
                .is_some_and(|(a, b)| a != b)
            {
                return Err(TransformError::shape(
                    "reasoning_details.id",
                    "fragment changed item identity",
                ));
            }
            fn append(slot: &mut Option<String>, value: Option<String>) {
                if let Some(value) = value {
                    slot.get_or_insert_default().push_str(&value);
                }
            }
            if let Some(text) = d.text {
                append(v.text.get_or_insert(None), text);
            }
            append(&mut v.summary, d.summary);
            if d.signature.is_some() {
                v.signature = d.signature;
            }
            if d.data.is_some() {
                v.data = d.data;
            }
            if d.id.is_some() {
                v.id = d.id;
            }
        } else {
            target.push(d);
        }
    }
    Ok(())
}
