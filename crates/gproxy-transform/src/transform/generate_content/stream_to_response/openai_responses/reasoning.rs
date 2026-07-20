use std::collections::BTreeMap;

use crate::protocol::openai;

use super::message::{TextPartState, non_empty};

pub(super) struct ReasoningState {
    index: u32,
    pub(super) id: Option<String>,
    summary: BTreeMap<u32, TextPartState>,
    content: BTreeMap<u32, TextPartState>,
    pub(super) encrypted_content: Option<String>,
    pub(super) status: Option<openai::ResponseItemLifecycleStatus>,
}

impl ReasoningState {
    pub(super) fn new(index: u32) -> Self {
        Self {
            index,
            id: None,
            summary: BTreeMap::new(),
            content: BTreeMap::new(),
            encrypted_content: None,
            status: None,
        }
    }

    pub(super) fn summary_part(&mut self, index: u32) -> &mut TextPartState {
        self.summary.entry(index).or_default()
    }

    pub(super) fn content_part(&mut self, index: u32) -> &mut TextPartState {
        self.content.entry(index).or_default()
    }

    pub(super) fn seed_summary(&mut self, parts: Vec<openai::ResponseReasoningSummaryPart>) {
        for (index, part) in parts.into_iter().enumerate() {
            self.summary_part(u32::try_from(index).unwrap_or(u32::MAX))
                .set_done(part.text);
        }
    }

    pub(super) fn seed_content(&mut self, parts: Vec<openai::ResponseReasoningTextPart>) {
        for (index, part) in parts.into_iter().enumerate() {
            self.content_part(u32::try_from(index).unwrap_or(u32::MAX))
                .set_done(part.text);
        }
    }

    pub(super) fn has_content(&self) -> bool {
        !self.summary.is_empty() || !self.content.is_empty() || self.encrypted_content.is_some()
    }

    pub(super) fn finish(self) -> openai::ResponseItem {
        let summary = self
            .summary
            .into_values()
            .filter_map(|part| {
                non_empty(part.finish_plain()).map(|text| openai::ResponseReasoningSummaryPart {
                    text,
                    type_: openai::ResponseReasoningSummaryType::SummaryText,
                    extra: Default::default(),
                })
            })
            .collect::<Vec<_>>();
        let content = self
            .content
            .into_values()
            .filter_map(|part| {
                non_empty(part.finish_plain()).map(|text| openai::ResponseReasoningTextPart {
                    text,
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    extra: Default::default(),
                })
            })
            .collect::<Vec<_>>();

        openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            id: Some(
                self.id
                    .unwrap_or_else(|| format!("reasoning_{}", self.index)),
            ),
            summary,
            content: (!content.is_empty()).then_some(content),
            encrypted_content: self.encrypted_content,
            status: self
                .status
                .or(Some(openai::ResponseItemLifecycleStatus::Completed)),
            extra: Default::default(),
        })
    }
}
