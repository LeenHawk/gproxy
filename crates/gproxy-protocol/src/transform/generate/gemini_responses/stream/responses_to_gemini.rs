use super::{
    common::{Budget, StreamEnd, StreamLimits, declared, invalid, limit, measure},
    context::ResponsesToGeminiContext,
    identity,
    response_items::Item,
};
use crate::{
    Dialect,
    transform::generate::stream::{
        gemini::{GeminiStreamCollector, GeminiStreamLimits},
        responses::{ResponsesStreamCollector, ResponsesStreamLimits},
    },
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        gemini as g,
        openai::responses::{response as r, stream as s},
    },
};
use std::collections::{BTreeMap, BTreeSet};

pub struct ResponsesToGeminiStream {
    source: Option<ResponsesStreamCollector>,
    target: Option<GeminiStreamCollector>,
    pub(super) flow: IdentityFlow,
    pub(super) policy: TargetIdPolicy,
    pub(super) limits: StreamLimits,
    budget: Budget,
    pub(super) used: BTreeSet<String>,
    pub(super) native_calls: BTreeSet<String>,
    pub(super) model: Option<String>,
    response_id: Option<String>,
    observed_usage: Option<r::ResponseUsage>,
    failed: bool,
    pub(super) terminal: bool,
    image_progress: bool,
    pub(super) image_only: bool,
    pub(super) jpeg_only: bool,
    pub(super) items: BTreeMap<i64, Item>,
    pub(super) cursor: i64,
    pub(super) held: usize,
    pub(super) parts: usize,
    pub(super) tools: usize,
}

impl ResponsesToGeminiStream {
    pub fn new(
        context: ResponsesToGeminiContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(context, flow, TargetIdPolicy::new(Dialect::Gemini), limits)
    }
    pub fn new_with_policy(
        context: ResponsesToGeminiContext,
        flow: IdentityFlow,
        policy: TargetIdPolicy,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        if policy.dialect != Dialect::Gemini {
            return Err(invalid("Gemini target policy required"));
        }
        Ok(Self {
            source: Some(ResponsesStreamCollector::new(ResponsesStreamLimits {
                max_events: limits.max_events,
                max_bytes: limits.max_bytes,
                max_items: limits.max_items,
                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            })),
            target: Some(GeminiStreamCollector::new(GeminiStreamLimits {
                max_events: limits.max_events,
                max_bytes: limits.max_bytes,
                max_candidates: 1,
                max_parts: limits.max_parts,
            })),
            flow,
            policy,
            limits,
            budget: Budget::new(limits),
            used: Default::default(),
            native_calls: Default::default(),
            model: None,
            response_id: None,
            observed_usage: None,
            failed: false,
            terminal: false,
            image_progress: false,
            jpeg_only: context.image_mime == Some(g::ImageMimeType::ImageJpeg),
            image_only: context
                .response_modalities
                .as_ref()
                .is_some_and(|v| !v.is_empty() && v.iter().all(|v| *v == g::Modality::Image)),
            items: Default::default(),
            cursor: 0,
            held: 0,
            parts: 0,
            tools: 0,
        })
    }
    pub(crate) fn reserve_external_ids(
        &mut self,
        role: crate::transform::identity::IdentityRole,
        ids: &std::collections::BTreeSet<String>,
        max: usize,
    ) -> Result<(), TransformError> {
        self.flow.reserve_external_ids(role, ids, max).map_err(|e| {
            TransformError::new(
                crate::transform::TransformErrorKind::Conflict,
                "fanout.ids",
                e.to_string(),
            )
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        event: s::StreamEvent,
    ) -> Result<Converted<Vec<g::GenerateContentResponseBody>>, TransformError> {
        if self.failed || self.terminal {
            self.failed = true;
            return Err(invalid("event after terminal or failure"));
        }
        let result = self.inner(declared(event));
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        event: s::StreamEvent,
    ) -> Result<Converted<Vec<g::GenerateContentResponseBody>>, TransformError> {
        let event = crate::transform::generate::multi_agent::attribute_event(event)?;
        self.budget.input(&event)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(event.clone())?;
        let mut out = Vec::new();
        match event {
            s::StreamEvent::Created(v) => {
                self.model = Some(v.response.model.clone());
                let id = identity::response_id(
                    &mut self.flow,
                    &self.policy,
                    Dialect::OpenAi,
                    Some(v.response.id.clone()),
                )?;
                self.response_id = Some(id.clone());
                self.observe(&v.response)?;
                self.emit(
                    g::GenerateContentResponseBody::builder()
                        .response_id(id)
                        .model_version(v.response.model)
                        .build(),
                    &mut out,
                )?;
                for (index, item) in v.response.output.into_iter().enumerate() {
                    self.add_item(index as i64, item)?;
                }
            }
            s::StreamEvent::InProgress(v) => self.observe(&v.response)?,
            s::StreamEvent::Queued(v) => self.observe(&v.response)?,
            s::StreamEvent::OutputItemAdded(v) => self.add_item(v.output_index, v.item)?,
            s::StreamEvent::ContentPartAdded(v) => {
                self.bind(v.output_index, &v.item_id)?;
                self.add_part(v.output_index, v.content_index, v.part)?;
            }
            s::StreamEvent::OutputTextDelta(v) => {
                self.bind(v.output_index, &v.item_id)?;
                self.append_text(v.output_index, v.content_index, v.delta, false)?;
            }
            s::StreamEvent::RefusalDelta(v) => {
                self.bind(v.output_index, &v.item_id)?;
                self.append_text(v.output_index, v.content_index, v.delta, false)?;
            }
            s::StreamEvent::ReasoningTextDelta(v) => {
                self.bind(v.output_index, &v.item_id)?;
                self.append_text(v.output_index, v.content_index, v.delta, true)?;
            }
            s::StreamEvent::ContentPartDone(v) => {
                self.part_done(v.output_index, v.content_index)?
            }
            s::StreamEvent::FunctionCallArgumentsDelta(v) => {
                self.bind(v.output_index, &v.item_id)?;
                self.reserve_arguments(v.output_index, v.delta.len())?;
            }
            s::StreamEvent::FunctionCallArgumentsDone(v) => {
                self.bind(v.output_index, &v.item_id)?;
                self.arguments_done(v.output_index, v.arguments)?;
            }
            s::StreamEvent::OutputItemDone(v) => self.item_done(v.output_index, v.item)?,
            s::StreamEvent::ReasoningSummaryPartAdded(v) => {
                self.count_part()?;
                self.summary(v.output_index, v.part.text.len())?;
            }
            s::StreamEvent::ReasoningSummaryTextDelta(v) => {
                self.summary(v.output_index, v.delta.len())?
            }
            s::StreamEvent::Completed(v) => {
                self.observe(&v.response)?;
                self.terminal = true;
            }
            s::StreamEvent::Incomplete(v) => {
                self.observe(&v.response)?;
                self.terminal = true;
            }
            s::StreamEvent::OutputTextDone(_)
            | s::StreamEvent::RefusalDone(_)
            | s::StreamEvent::ReasoningTextDone(_)
            | s::StreamEvent::OutputTextAnnotationAdded(_)
            | s::StreamEvent::ReasoningSummaryTextDone(_)
            | s::StreamEvent::ReasoningSummaryPartDone(_) => {}
            s::StreamEvent::ImageCall(_)
            | s::StreamEvent::ImageGenerating(_)
            | s::StreamEvent::ImageCompleted(_)
            | s::StreamEvent::ImagePartial(_) => {
                self.image_progress = true;
            }
            s::StreamEvent::Failed(_) | s::StreamEvent::Error(_) => {
                return Err(invalid("native source failure"));
            }
            _ if self.image_only => {}
            _ => {}
        }
        self.flush(&mut out)?;
        Ok(Converted {
            value: out,
            report: Report::default(),
        })
    }
    fn observe(&mut self, body: &r::GenerateContentResponseBody) -> Result<(), TransformError> {
        if let Some(usage) = body.usage.as_ref().and_then(Option::as_ref) {
            super::super::usage::to_gemini(usage.clone(), &mut Report::default())?;
            if let Some(old) = &self.observed_usage
                && (usage.input_tokens < old.input_tokens
                    || usage.output_tokens < old.output_tokens
                    || usage.total_tokens < old.total_tokens
                    || usage.input_tokens_details.cached_tokens
                        < old.input_tokens_details.cached_tokens
                    || usage.input_tokens_details.cache_write_tokens
                        < old.input_tokens_details.cache_write_tokens
                    || usage.output_tokens_details.reasoning_tokens
                        < old.output_tokens_details.reasoning_tokens)
            {
                return Err(invalid("native cumulative usage decreased"));
            }
            self.observed_usage = Some(usage.clone());
        }
        Ok(())
    }
    pub(super) fn reserve(&mut self, bytes: usize) -> Result<(), TransformError> {
        self.held = self
            .held
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.max_pending)
            .ok_or_else(limit)?;
        Ok(())
    }
    pub(super) fn release(&mut self, bytes: usize) {
        self.held -= bytes;
    }
    pub(super) fn emit(
        &mut self,
        chunk: g::GenerateContentResponseBody,
        out: &mut Vec<g::GenerateContentResponseBody>,
    ) -> Result<(), TransformError> {
        self.budget.output(&chunk)?;
        self.target
            .as_mut()
            .ok_or_else(|| invalid("target consumed"))?
            .push(chunk.clone())?;
        out.push(chunk);
        Ok(())
    }
    pub(super) fn emit_part(
        &mut self,
        part: g::Part,
        out: &mut Vec<g::GenerateContentResponseBody>,
    ) -> Result<(), TransformError> {
        self.emit(
            g::GenerateContentResponseBody::builder()
                .candidates(vec![
                    g::Candidate::builder()
                        .index(0)
                        .content(
                            g::Content::builder()
                                .role("model")
                                .parts(vec![part])
                                .build(),
                        )
                        .build(),
                ])
                .build(),
            out,
        )
    }
    pub fn finish(mut self) -> Result<StreamEnd<g::GenerateContentResponseBody>, TransformError> {
        if self.failed || !self.terminal {
            return Err(invalid("failed stream or premature EOF"));
        }
        let source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?
            .value;
        let only_image = [g::Modality::Image];
        let mut converted = super::super::responses_to_gemini_response_with_modalities(
            source.clone(),
            Default::default(),
            self.image_only.then_some(only_image.as_slice()),
        )?;
        if self.image_progress {
            converted.report.omitted("image.progress", "Gemini inline images expose the final image; preview/progress events are not additional generated images");
        }
        let mut check_flow = self.flow.clone();
        converted.value.response_id = Some(identity::response_id(
            &mut check_flow,
            &self.policy,
            Dialect::OpenAi,
            Some(source.id.clone()),
        )?);
        let originals: Vec<_> = source
            .output
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if self.image_only {
                    return None;
                }
                if let r::ResponseOutputItem::FunctionCall(call) = item {
                    Some((index as i64, call))
                } else {
                    None
                }
            })
            .collect();
        let projected: Vec<_> = converted
            .value
            .candidates
            .iter_mut()
            .flatten()
            .filter_map(|c| c.content.as_mut())
            .flat_map(|c| c.parts.iter_mut().flatten())
            .filter(|p| p.function_call.is_some())
            .collect();
        if originals.len() != projected.len() {
            return Err(invalid("canonical function count changed"));
        }
        let mut used = BTreeSet::new();
        for ((index, call), part) in originals.into_iter().zip(projected) {
            identity::call(
                part,
                call,
                index,
                (&mut check_flow, &self.policy),
                &mut used,
            )?;
        }
        measure(&converted.value, self.limits.max_bytes)?;
        let mut tail = converted.value.clone();
        for candidate in tail.candidates.iter_mut().flatten() {
            if let Some(content) = &mut candidate.content {
                content.parts = Some(Vec::new());
            }
        }
        let mut out = Vec::new();
        self.emit(tail, &mut out)?;
        let mut actual = self
            .target
            .take()
            .ok_or_else(|| invalid("target consumed"))?
            .finish()?
            .value;
        identity::normalize_text(&mut actual);
        identity::normalize_text(&mut converted.value);
        if actual != converted.value {
            return Err(invalid(
                "native Gemini stream differs from completed Responses conversion",
            ));
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report: converted.report,
        })
    }
}
