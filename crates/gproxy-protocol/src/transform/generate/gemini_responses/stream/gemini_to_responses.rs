use super::{
    common::{Budget, StreamEnd, StreamLimits, declared, invalid, limit, measure},
    context::{GeminiToResponsesContext, clean_context, clone_response},
    identity,
    response_events::ResponseEvents,
    usage::GeminiUsageProgress,
};
use crate::{
    Dialect,
    transform::generate::stream::gemini::{GeminiStreamCollector, GeminiStreamLimits},
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        gemini as g,
        openai::responses::{response as r, stream as s},
    },
};

pub struct GeminiToResponsesStream {
    source: Option<GeminiStreamCollector>,
    pub(super) target: ResponseEvents,
    pub(super) flow: IdentityFlow,
    pub(super) policy: TargetIdPolicy,
    pub(super) context: GeminiToResponsesContext,
    pub(super) limits: StreamLimits,
    pub(super) budget: Budget,
    model: Option<String>,
    source_id: Option<String>,
    emitted_id: Option<String>,
    started: bool,
    failed: bool,
    pending: Vec<g::Part>,
    pending_bytes: usize,
    usage: GeminiUsageProgress,
    usage_after_content: bool,
    pub(super) part_index: usize,
    pub(super) item_count: usize,

    pub(super) call_ids: std::collections::BTreeSet<String>,
    pub(super) client_tools: crate::transform::generate::client_tools::Bindings,
}

impl GeminiToResponsesStream {
    pub fn new(
        context: GeminiToResponsesContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(context, flow, TargetIdPolicy::new(Dialect::OpenAi), limits)
    }
    pub fn new_with_policy(
        context: GeminiToResponsesContext,
        flow: IdentityFlow,
        policy: TargetIdPolicy,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        if policy.dialect != Dialect::OpenAi {
            return Err(invalid("Responses target policy required"));
        }
        let context = clean_context(context, limits)?;
        let client_tools = crate::transform::generate::client_tools::Bindings::for_target(
            &context.response.request,
            Dialect::Gemini,
        )?;
        Ok(Self {
            source: Some(GeminiStreamCollector::new(GeminiStreamLimits {
                max_bytes: limits.max_bytes,
            })),
            target: ResponseEvents::new(limits),
            flow,
            policy,
            model: context.actual_model.clone(),
            context,
            limits,
            budget: Budget::new(limits),
            source_id: None,
            emitted_id: None,
            started: false,
            failed: false,
            pending: Vec::new(),
            pending_bytes: 0,
            usage: GeminiUsageProgress::default(),
            usage_after_content: false,
            part_index: 0,
            item_count: 0,

            call_ids: Default::default(),
            client_tools,
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        chunk: g::GenerateContentResponseBody,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        if self.failed {
            return Err(invalid("stream failed"));
        }
        let result = self.inner(declared(chunk));
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        chunk: g::GenerateContentResponseBody,
    ) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
        self.budget.input(&chunk)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(chunk.clone())?;
        if let Some(model) = &chunk.model_version {
            if self.model.as_ref().is_some_and(|old| old != model) {
                return Err(invalid("native model conflicts with actual model fact"));
            }
            self.model = Some(model.clone());
        }
        if let Some(id) = &chunk.response_id {
            self.source_id = Some(id.clone());
            if let Some(previous) = &self.emitted_id {
                let bound = identity::response_id(
                    &mut self.flow,
                    &self.policy,
                    Dialect::Gemini,
                    Some(id.clone()),
                )?;
                if &bound != previous {
                    return Err(invalid("late source response ID changed emitted ID"));
                }
            }
        }
        let usage_present = chunk.usage_metadata.is_some();
        if let Some(usage) = &chunk.usage_metadata {
            self.usage.observe(usage);
            self.usage_after_content = true;
        }
        let mut out = Vec::new();
        self.start(&mut out)?;
        for candidate in chunk.candidates.into_iter().flatten() {
            if candidate.index.is_some_and(|n| n != 0) {
                continue;
            }
            if let Some(content) = candidate.content {
                if content.role.as_deref().is_some_and(|r| r != "model") {
                    return Err(invalid("generated content must have model role"));
                }
                for part in content.parts.into_iter().flatten() {
                    if !usage_present {
                        self.usage_after_content = false;
                    }
                    if self.started {
                        self.part(part, &mut out)?;
                    } else {
                        let size = measure(
                            &part,
                            self.limits.max_pending.saturating_sub(self.pending_bytes),
                        )?;
                        self.pending_bytes =
                            self.pending_bytes.checked_add(size).ok_or_else(limit)?;
                        if self.pending_bytes > self.limits.max_pending {
                            return Err(limit());
                        }
                        self.pending.push(part);
                    }
                }
            }
        }
        Ok(Converted {
            value: out,
            report: Report::default(),
        })
    }
    fn start(&mut self, out: &mut Vec<s::StreamEvent>) -> Result<(), TransformError> {
        if self.started {
            return Ok(());
        }
        let Some(model) = self.model.clone() else {
            return Ok(());
        };
        let id = identity::response_id(
            &mut self.flow,
            &self.policy,
            Dialect::Gemini,
            self.source_id.clone(),
        )?;
        let mut body = clone_response(&self.context.response).into_response(
            id.clone(),
            self.context.response.created_at,
            model,
        )?;
        body.status = Some(r::ResponseStatus::InProgress);
        self.target.emit(&mut self.budget, out, |sequence_number| {
            s::StreamEvent::Created(s::ResponseCreated {
                sequence_number,
                response: body,
                rest: Default::default(),
            })
        })?;
        self.emitted_id = Some(id);
        self.started = true;
        self.pending_bytes = 0;
        for part in std::mem::take(&mut self.pending) {
            self.part(part, out)?;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<StreamEnd<s::StreamEvent>, TransformError> {
        if self.failed {
            return Err(invalid("stream failed"));
        }
        let mut source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?
            .value;
        if !self.usage_after_content {
            return Err(TransformError::missing_metadata(
                "final Gemini usage after last content",
            ));
        }
        let usage = source
            .usage_metadata
            .as_mut()
            .ok_or_else(|| TransformError::missing_metadata("final Gemini usage"))?;
        let changed = self
            .usage
            .effective(usage, self.context.final_thinking_tokens)?;
        let model = self
            .model
            .clone()
            .ok_or_else(|| TransformError::missing_metadata("actual model for Responses header"))?;
        source.model_version = Some(model);
        let mut out = Vec::new();
        self.start(&mut out)?;
        let converted = super::super::gemini_to_responses_response(
            source,
            self.context.response,
            &mut self.flow.clone(),
            &self.policy,
        )?;
        let expected = converted.value;
        measure(&expected, self.limits.max_bytes)?;
        if expected.output.len() != self.item_count {
            return Err(invalid("native Gemini output item count changed"));
        }
        for (index, item) in expected.output.iter().enumerate() {
            // Complete inline image bytes were emitted atomically when that
            // actual Gemini part arrived; never emit a second item.done.
            if matches!(item, r::ResponseOutputItem::ImageGenerationCall(_)) {
                continue;
            }
            if matches!(item, r::ResponseOutputItem::FunctionCall(_)) {
                self.target
                    .arguments_done(&mut self.budget, &mut out, index as i64, item)?;
            }
            self.target
                .item_done(&mut self.budget, &mut out, index as i64, item.clone())?;
        }
        self.target
            .emit(&mut self.budget, &mut out, |sequence_number| {
                if expected.status == Some(r::ResponseStatus::Incomplete) {
                    s::StreamEvent::Incomplete(s::ResponseIncomplete {
                        sequence_number,
                        response: expected.clone(),
                        rest: Default::default(),
                    })
                } else {
                    s::StreamEvent::Completed(s::ResponseCompleted {
                        sequence_number,
                        response: expected.clone(),
                        rest: Default::default(),
                    })
                }
            })?;
        let actual = self.target.finish()?;
        if actual != expected {
            return Err(invalid(
                "native Responses result differs from completed Gemini conversion",
            ));
        }
        let mut report = converted.report;
        if changed {
            report.changed("usage","final split resolved from current counters/explicit final thinking; stale prefix components are lower bounds");
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report,
        })
    }
}
