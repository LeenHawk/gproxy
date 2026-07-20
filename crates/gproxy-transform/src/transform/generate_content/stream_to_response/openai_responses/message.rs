use std::collections::BTreeMap;

use crate::protocol::openai;

use super::sanitize::{sanitize_annotation, stream_logprob};

pub(super) struct MessageState {
    index: u32,
    pub(super) id: Option<String>,
    pub(super) status: Option<openai::ResponseItemLifecycleStatus>,
    text: BTreeMap<u32, TextPartState>,
    refusal: BTreeMap<u32, TextPartState>,
}

impl MessageState {
    pub(super) fn new(index: u32) -> Self {
        Self {
            index,
            id: None,
            status: None,
            text: BTreeMap::new(),
            refusal: BTreeMap::new(),
        }
    }

    pub(super) fn text_part(&mut self, index: u32) -> &mut TextPartState {
        self.text.entry(index).or_default()
    }

    pub(super) fn refusal_part(&mut self, index: u32) -> &mut TextPartState {
        self.refusal.entry(index).or_default()
    }

    pub(super) fn seed_content(&mut self, parts: Vec<openai::ResponseMessageOutputContentPart>) {
        for (index, part) in parts.into_iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            match part {
                openai::ResponseMessageOutputContentPart::OutputText {
                    annotations,
                    logprobs,
                    text,
                    ..
                } => {
                    let part = self.text_part(index);
                    part.set_done(text);
                    part.seed_annotations(annotations);
                    part.logprobs = logprobs.unwrap_or_default();
                }
                openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
                    self.refusal_part(index).set_done(refusal);
                }
            }
        }
    }

    pub(super) fn has_content(&self) -> bool {
        !self.text.is_empty() || !self.refusal.is_empty()
    }

    pub(super) fn finish(self) -> openai::ResponseItem {
        let mut content = Vec::new();
        content.extend(
            self.text
                .into_values()
                .filter_map(TextPartState::finish_text),
        );
        content.extend(self.refusal.into_values().filter_map(|part| {
            non_empty(part.finish_plain()).map(|refusal| {
                openai::ResponseMessageOutputContentPart::Refusal {
                    refusal,
                    extra: Default::default(),
                }
            })
        }));

        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
            openai::ResponseOutputMessageItem {
                type_: openai::ResponseMessageItemType::Message,
                id: self.id.unwrap_or_else(|| format!("msg_{}", self.index)),
                role: openai::ResponseOutputMessageRole::Assistant,
                content,
                status: self
                    .status
                    .unwrap_or(openai::ResponseItemLifecycleStatus::Completed),
                phase: None,
                extra: Default::default(),
            },
        ))
    }
}

#[derive(Default)]
pub(super) struct TextPartState {
    delta: String,
    done: Option<String>,
    logprobs: Vec<openai::TokenLogprob>,
    annotations: BTreeMap<u32, openai::ResponseAnnotation>,
}

impl TextPartState {
    pub(super) fn push_delta(&mut self, value: String) {
        self.delta.push_str(&value);
    }

    pub(super) fn set_done(&mut self, value: String) {
        self.done = Some(value);
    }

    pub(super) fn push_logprobs(&mut self, value: Option<Vec<openai::StreamTokenLogprob>>) {
        self.logprobs
            .extend(value.unwrap_or_default().into_iter().map(stream_logprob));
    }

    pub(super) fn set_logprobs(&mut self, value: Option<Vec<openai::StreamTokenLogprob>>) {
        if let Some(value) = value {
            self.logprobs = value.into_iter().map(stream_logprob).collect();
        }
    }

    pub(super) fn push_annotation(&mut self, index: u32, value: openai::ResponseAnnotation) {
        self.annotations.insert(index, sanitize_annotation(value));
    }

    fn seed_annotations(&mut self, values: Vec<openai::ResponseAnnotation>) {
        self.annotations
            .extend(values.into_iter().enumerate().map(|(index, value)| {
                (
                    u32::try_from(index).unwrap_or(u32::MAX),
                    sanitize_annotation(value),
                )
            }));
    }

    pub(super) fn finish_plain(self) -> String {
        self.done.unwrap_or(self.delta)
    }

    fn finish_text(self) -> Option<openai::ResponseMessageOutputContentPart> {
        let text = non_empty(self.done.unwrap_or(self.delta))?;
        Some(openai::ResponseMessageOutputContentPart::OutputText {
            annotations: self.annotations.into_values().collect(),
            logprobs: (!self.logprobs.is_empty()).then_some(self.logprobs),
            text,
            extra: Default::default(),
        })
    }
}

pub(super) fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
