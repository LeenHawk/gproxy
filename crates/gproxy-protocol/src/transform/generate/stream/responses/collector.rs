use super::{
    items::{Item, item_id},
    *,
};
use crate::{transform::Report, wire::DeclaredFields};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
pub struct ResponsesStreamLimits {
    pub max_bytes: usize,

    pub max_text_bytes: usize,
    pub max_json_bytes: usize,
}

impl Default for ResponsesStreamLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,

            max_text_bytes: 16 * 1024 * 1024,
            max_json_bytes: 16 * 1024 * 1024,
        }
    }
}

pub struct ResponsesStreamCollector {
    pub(super) limits: ResponsesStreamLimits,
    pub(super) sequence: Option<i64>,
    pub(super) bytes: usize,

    pub(super) items: Vec<Item>,
    pub(super) ids: HashSet<String>,
    pub(super) response: Option<r::GenerateContentResponseBody>,
    pub(super) done: bool,
    pub(super) failed: bool,
    pub(super) text_bytes: usize,
    pub(super) json_bytes: usize,
    pub(super) report: Report,
}

impl ResponsesStreamCollector {
    pub fn new(limits: ResponsesStreamLimits) -> Self {
        Self {
            limits,
            sequence: None,
            bytes: 0,

            items: Vec::new(),
            ids: HashSet::new(),
            response: None,
            done: false,
            failed: false,
            text_bytes: 0,
            json_bytes: 0,
            report: Report::default(),
        }
    }
    pub fn push(&mut self, event: s::StreamEvent) -> Result<(), TransformError> {
        if self.done || self.failed {
            self.failed = true;
            return Err(invalid("event after terminal/failed stream"));
        }
        let result = self.push_inner(event.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_inner(&mut self, event: s::StreamEvent) -> Result<(), TransformError> {
        let event = crate::transform::generate::multi_agent::attribute_event(event)?;
        if let (Some(agent), Some(index)) = (event.agent(), event.output_index())
            && let Some(item) = usize::try_from(index)
                .ok()
                .and_then(|index| self.items.get(index))
            && item
                .value
                .agent()
                .map_or("/root", |owner| owner.agent_name.as_str())
                != agent.agent_name
        {
            return Err(invalid("event agent differs from its output item"));
        }

        self.bytes += bounded(&event, self.limits.max_bytes.saturating_sub(self.bytes))?;
        let seq = super::events::sequence(&event);
        if seq < 0 || self.sequence.is_some_and(|v| seq <= v) {
            return Err(invalid("nonmonotonic sequence number"));
        }
        self.sequence = Some(seq);
        if self.response.is_none()
            && !matches!(event, s::StreamEvent::Created(_) | s::StreamEvent::Error(_))
        {
            return Err(invalid("response.created required before other events"));
        }
        match event {
            s::StreamEvent::Created(v) => {
                if self.response.is_some() {
                    return Err(invalid("duplicate/nonempty response.created"));
                }
                self.check_response(&v.response, false)?;
                self.seed_output(&v.response.output)?;
                self.response = Some(v.response);
            }
            s::StreamEvent::Queued(v) => {
                self.check_response(&v.response, false)?;
                self.snapshot_output(&v.response.output)?;
                if v.response.status != Some(r::ResponseStatus::Queued) {
                    return Err(invalid("queued status mismatch"));
                }
            }
            s::StreamEvent::InProgress(v) => {
                self.check_response(&v.response, false)?;
                self.snapshot_output(&v.response.output)?;
                if v.response.status != Some(r::ResponseStatus::InProgress) {
                    return Err(invalid("in_progress status mismatch"));
                }
            }
            s::StreamEvent::Completed(v) => {
                self.terminal(v.response, r::ResponseStatus::Completed)?
            }
            s::StreamEvent::Incomplete(v) => {
                self.terminal(v.response, r::ResponseStatus::Incomplete)?
            }
            s::StreamEvent::Failed(value) => {
                self.check_response(&value.response, true)?;
                if value.response.status != Some(r::ResponseStatus::Failed) {
                    return Err(invalid("failed event status mismatch"));
                }
                let detail = value
                    .response
                    .error
                    .map(|error| format!("{:?}: {}", error.code, error.message))
                    .unwrap_or_else(|| "upstream response failed without error detail".into());
                return Err(TransformError::invalid_result(
                    "responses.stream.failed",
                    detail,
                ));
            }
            s::StreamEvent::Error(error) => {
                return Err(TransformError::invalid_result(
                    "responses.stream.error",
                    format!(
                        "{}{}: {}",
                        error.code.as_deref().unwrap_or("error"),
                        error
                            .param
                            .as_ref()
                            .map(|p| format!(" ({p})"))
                            .unwrap_or_default(),
                        error.message
                    ),
                ));
            }
            s::StreamEvent::OutputItemAdded(v) => {
                if v.output_index != self.items.len() as i64 {
                    return Err(invalid("noncontiguous/duplicate output index"));
                }

                if let Some(id) = item_id(&v.item)
                    && (id.is_empty() || !self.ids.insert(id.to_owned()))
                {
                    return Err(invalid("empty/duplicate item id"));
                }
                self.charge_item(&v.item)?;
                let (text, json) = super::lifecycle::item_sizes(&v.item);
                self.charge(text, false)?;
                self.charge(json, true)?;
                self.items.push(Item::new(v.item));
            }
            s::StreamEvent::OutputItemDone(v) => {
                self.charge_item(&v.item)?;
                if let Some(id) = item_id(&v.item) {
                    self.item(v.output_index, id)?;
                }
                let item = self
                    .items
                    .get_mut(
                        usize::try_from(v.output_index)
                            .map_err(|_| invalid("negative output index"))?,
                    )
                    .ok_or_else(|| invalid("item done without added"))?;
                item.finish(v.item)?;
                self.check_retained_sizes()?;
            }
            s::StreamEvent::ContentPartAdded(v) => {
                self.add_part(v.output_index, &v.item_id, v.content_index, false, v.part)?
            }
            s::StreamEvent::ContentPartDone(v) => self
                .part(v.output_index, &v.item_id, v.content_index, false)?
                .finish(v.part)?,
            s::StreamEvent::ReasoningSummaryPartAdded(v) => self.add_part(
                v.output_index,
                &v.item_id,
                v.summary_index,
                true,
                s::OutputContentPart::Reasoning(s::ReasoningText {
                    text: v.part.text,
                    type_: s::ReasoningTextType::ReasoningText,
                    rest: Default::default(),
                }),
            )?,
            s::StreamEvent::ReasoningSummaryPartDone(v) => {
                let part = self.part(v.output_index, &v.item_id, v.summary_index, true)?;
                part.incomplete = v.status == Some(s::SummaryPartStatus::Incomplete);
                part.finish(s::OutputContentPart::Reasoning(s::ReasoningText {
                    text: v.part.text,
                    type_: s::ReasoningTextType::ReasoningText,
                    rest: Default::default(),
                }))?;
            }
            s::StreamEvent::OutputTextDelta(v) => {
                self.delta(v.output_index, &v.item_id, v.content_index, 0, &v.delta)?
                    .logs
                    .extend(v.logprobs);
            }
            s::StreamEvent::RefusalDelta(v) => {
                self.delta(v.output_index, &v.item_id, v.content_index, 1, &v.delta)?;
            }
            s::StreamEvent::ReasoningTextDelta(v) => {
                self.delta(v.output_index, &v.item_id, v.content_index, 2, &v.delta)?;
            }
            s::StreamEvent::ReasoningSummaryTextDelta(v) => {
                self.delta(v.output_index, &v.item_id, v.summary_index, 3, &v.delta)?;
            }
            s::StreamEvent::OutputTextDone(v) => self.text_done(
                v.output_index,
                &v.item_id,
                v.content_index,
                0,
                &v.text,
                &v.logprobs,
            )?,
            s::StreamEvent::RefusalDone(v) => self.text_done(
                v.output_index,
                &v.item_id,
                v.content_index,
                1,
                &v.refusal,
                &[],
            )?,
            s::StreamEvent::ReasoningTextDone(v) => {
                self.text_done(v.output_index, &v.item_id, v.content_index, 2, &v.text, &[])?
            }
            s::StreamEvent::ReasoningSummaryTextDone(v) => {
                self.text_done(v.output_index, &v.item_id, v.summary_index, 3, &v.text, &[])?
            }
            s::StreamEvent::OutputTextAnnotationAdded(v) => {
                let p = self.part(v.output_index, &v.item_id, v.content_index, false)?;
                if p.done {
                    return Err(invalid("annotation after part done"));
                }
                let s::OutputContentPart::Text(text) = &mut p.value else {
                    return Err(invalid("annotation on nontext part"));
                };
                if v.annotation_index != p.annotation_index as i64 {
                    return Err(invalid("noncontiguous annotation index"));
                }
                p.annotation_index += 1;
                let known = v
                    .annotation
                    .get("type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|v| {
                        matches!(
                            v,
                            "url_citation"
                                | "file_citation"
                                | "container_file_citation"
                                | "file_path"
                        )
                    });
                match serde_json::from_value::<i::OutputAnnotation>(v.annotation) {
                    Ok(annotation) => text.annotations.push(annotation.into_declared()),
                    Err(_) if known => return Err(invalid("malformed known annotation")),
                    Err(_) => self.report.omitted(
                        "response.output_text.annotation.added",
                        "unknown annotation has no typed final output representation",
                    ),
                }
            }
            s::StreamEvent::FunctionCallArgumentsDelta(v) => {
                self.argument(v.output_index, &v.item_id, 0, &v.delta, false)?
            }
            s::StreamEvent::FunctionCallArgumentsDone(v) => {
                if !matches!(&self.item(v.output_index,&v.item_id)?.value,r::ResponseOutputItem::FunctionCall(c) if c.name==v.name)
                {
                    return Err(invalid("function done name mismatch"));
                }
                self.argument(v.output_index, &v.item_id, 0, &v.arguments, true)?;
            }
            s::StreamEvent::CustomToolInputDelta(v) => {
                self.argument(v.output_index, &v.item_id, 1, &v.delta, false)?
            }
            s::StreamEvent::CustomToolInputDone(v) => {
                self.argument(v.output_index, &v.item_id, 1, &v.input, true)?
            }
            s::StreamEvent::CodeInterpreterCodeDelta(v) => {
                self.argument(v.output_index, &v.item_id, 2, &v.delta, false)?
            }
            s::StreamEvent::CodeInterpreterCodeDone(v) => {
                self.argument(v.output_index, &v.item_id, 2, &v.code, true)?
            }
            s::StreamEvent::McpArgumentsDelta(v) => {
                self.argument(v.output_index, &v.item_id, 3, &v.delta, false)?
            }
            s::StreamEvent::McpArgumentsDone(v) => {
                self.argument(v.output_index, &v.item_id, 3, &v.arguments, true)?
            }
            s::StreamEvent::ImageCall(v) => {
                self.item(v.output_index, &v.item_id)?.progress(0, 0)?
            }
            s::StreamEvent::ImageGenerating(v) => {
                self.item(v.output_index, &v.item_id)?.progress(0, 1)?
            }
            s::StreamEvent::ImageCompleted(v) => {
                self.item(v.output_index, &v.item_id)?.progress(0, 2)?
            }
            s::StreamEvent::CodeInterpreterInProgress(v) => {
                self.item(v.output_index, &v.item_id)?.progress(1, 0)?
            }
            s::StreamEvent::CodeInterpreterInterpreting(v) => {
                self.item(v.output_index, &v.item_id)?.progress(1, 1)?
            }
            s::StreamEvent::CodeInterpreterCompleted(v) => {
                self.item(v.output_index, &v.item_id)?.progress(1, 2)?
            }
            s::StreamEvent::FileSearchInProgress(v) => {
                self.item(v.output_index, &v.item_id)?.progress(2, 0)?
            }
            s::StreamEvent::FileSearchSearching(v) => {
                self.item(v.output_index, &v.item_id)?.progress(2, 1)?
            }
            s::StreamEvent::FileSearchCompleted(v) => {
                self.item(v.output_index, &v.item_id)?.progress(2, 2)?
            }
            s::StreamEvent::WebSearchInProgress(v) => {
                self.item(v.output_index, &v.item_id)?.progress(3, 0)?
            }
            s::StreamEvent::WebSearchSearching(v) => {
                self.item(v.output_index, &v.item_id)?.progress(3, 1)?
            }
            s::StreamEvent::WebSearchCompleted(v) => {
                self.item(v.output_index, &v.item_id)?.progress(3, 2)?
            }
            s::StreamEvent::McpInProgress(v) => {
                self.item(v.output_index, &v.item_id)?.progress(4, 0)?
            }
            s::StreamEvent::McpCompleted(v) => {
                self.item(v.output_index, &v.item_id)?.progress(4, 2)?
            }
            s::StreamEvent::McpFailed(v) => {
                self.item(v.output_index, &v.item_id)?.progress(4, 3)?
            }
            s::StreamEvent::McpListToolsInProgress(v) => {
                self.item(v.output_index, &v.item_id)?.progress(5, 0)?
            }
            s::StreamEvent::McpListToolsCompleted(v) => {
                self.item(v.output_index, &v.item_id)?.progress(5, 2)?
            }
            s::StreamEvent::McpListToolsFailed(v) => {
                self.item(v.output_index, &v.item_id)?.progress(5, 3)?
            }
            s::StreamEvent::ImagePartial(v) => {
                if v.partial_image_index < 0
                    || !matches!(
                        self.item(v.output_index, &v.item_id)?.value,
                        r::ResponseOutputItem::ImageGenerationCall(_)
                    )
                {
                    return Err(invalid("image preview identity/type mismatch"));
                }
                if !self
                    .report
                    .diagnostics
                    .iter()
                    .any(|d| d.field == "response.image_generation_call.partial_image")
                {
                    self.report.omitted(
                        "response.image_generation_call.partial_image",
                        "preview frames are superseded by the canonical image result",
                    );
                }
            }
            s::StreamEvent::AudioDelta(_)
            | s::StreamEvent::AudioDone(_)
            | s::StreamEvent::AudioTranscriptDelta(_)
            | s::StreamEvent::AudioTranscriptDone(_) => {
                return Err(TransformError::unsupported(
                    "response.audio",
                    "native response output has no corresponding audio item; use audio event consumer",
                ));
            }
        }
        Ok(())
    }
}
