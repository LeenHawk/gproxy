use super::super::ClaudeGeminiUsageFacts;
use super::{
    claude_blocks::Blocks,
    common::{Budget, StreamEnd, StreamLimits, bound, invalid, normalize_gemini, response_id},
    usage::ClaudeUsageProgress,
};
use crate::transform::generate::stream::{
    claude::{ClaudeStreamCollector, ClaudeStreamLimits},
    gemini::{GeminiStreamCollector, GeminiStreamLimits},
};
use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{DeclaredFields, claude::stream as s, gemini as g},
};

#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeToGeminiContext {
    pub usage: ClaudeGeminiUsageFacts,
}

pub struct ClaudeToGeminiStream {
    source: Option<ClaudeStreamCollector>,
    target: Option<GeminiStreamCollector>,
    flow: IdentityFlow,
    policy: TargetIdPolicy,
    budget: Budget,
    limits: StreamLimits,
    blocks: Blocks,
    facts: ClaudeGeminiUsageFacts,
    usage: ClaudeUsageProgress,
    response_id: Option<String>,
    model: Option<String>,
    failed: bool,
    stopped: bool,
}

impl ClaudeToGeminiStream {
    pub fn new(
        context: ClaudeToGeminiContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        Self::new_with_policy(
            context,
            flow,
            limits,
            TargetIdPolicy::new(crate::Dialect::Gemini),
        )
    }
    pub fn new_with_policy(
        context: ClaudeToGeminiContext,
        flow: IdentityFlow,
        limits: StreamLimits,
        policy: TargetIdPolicy,
    ) -> Result<Self, TransformError> {
        if policy.dialect != crate::Dialect::Gemini {
            return Err(TransformError::shape(
                "stream.policy",
                "target policy dialect mismatch",
            ));
        }

        Ok(Self {
            source: Some(ClaudeStreamCollector::new(ClaudeStreamLimits {
                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            })),
            target: Some(GeminiStreamCollector::new(GeminiStreamLimits {
                max_bytes: limits.max_bytes,
            })),
            flow,
            policy,
            budget: Budget::new(limits),
            limits,
            blocks: Blocks::new(limits),
            facts: context.usage,
            usage: Default::default(),
            response_id: None,
            model: None,
            failed: false,
            stopped: false,
        })
    }
    pub(crate) fn reserve_external_ids(
        &mut self,
        role: crate::transform::identity::IdentityRole,
        ids: &std::collections::BTreeSet<String>,
    ) -> Result<(), TransformError> {
        self.flow.reserve_external_ids(role, ids).map_err(|e| {
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
        if self.failed || self.stopped {
            self.failed = true;
            return Err(invalid("stream", "event after message_stop or failure"));
        }
        let result = self.inner(event.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn inner(
        &mut self,
        event: s::StreamEvent,
    ) -> Result<Converted<Vec<g::GenerateContentResponseBody>>, TransformError> {
        self.budget.input(&event)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("stream", "source consumed"))?
            .push(event.clone())?;
        match event {
            s::StreamEvent::MessageStart(v) => {
                self.usage.start(&v.message.usage)?;
                self.response_id = Some(response_id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::Claude,
                    Some(v.message.id),
                )?);
                self.model = Some(v.message.model);
            }
            s::StreamEvent::ContentBlockStart(v) => {
                self.blocks.start(v, &mut self.flow, &self.policy)?
            }
            s::StreamEvent::ContentBlockDelta(v) => self.blocks.delta(v)?,
            s::StreamEvent::ContentBlockStop(v) => self.blocks.stop(v.index)?,
            s::StreamEvent::MessageDelta(v) => self.usage.delta(&v.usage)?,
            s::StreamEvent::MessageStop(_) => self.stopped = true,
            s::StreamEvent::Ping(_) => {}
            s::StreamEvent::Error(_) => {
                return Err(invalid("stream.error", "Claude upstream error"));
            }
        }
        let mut out = Vec::new();
        while let Some(part) = self.blocks.next()? {
            let body = g::GenerateContentResponseBody::builder()
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
                .build();
            self.emit(&mut out, body)?;
        }
        self.blocks.check_pending()?;
        Ok(Converted {
            value: out,
            report: Report::default(),
        })
    }
    fn emit(
        &mut self,
        out: &mut Vec<g::GenerateContentResponseBody>,
        mut body: g::GenerateContentResponseBody,
    ) -> Result<(), TransformError> {
        body.response_id = self.response_id.clone();
        body.model_version = self.model.clone();
        self.budget.output(&body)?;
        self.target
            .as_mut()
            .ok_or_else(|| invalid("stream", "target consumed"))?
            .push(body.clone())?;
        out.push(body);
        Ok(())
    }
    pub fn finish(mut self) -> Result<StreamEnd<g::GenerateContentResponseBody>, TransformError> {
        if self.failed || !self.stopped || !self.blocks.is_empty() {
            return Err(invalid(
                "stream",
                "failed stream, premature EOF or pending blocks",
            ));
        }
        let mut source = self
            .source
            .take()
            .ok_or_else(|| invalid("stream", "source consumed"))?
            .finish()?
            .value;
        self.usage.finalize(&mut source.usage, self.facts)?;
        let converted = super::super::claude_to_gemini_response(
            source,
            self.facts,
            &mut self.flow.clone(),
            &self.policy,
        )?;
        let mut expected = converted.value;
        bound(&expected, self.limits.max_bytes)?;
        let mut tail = expected.clone();
        for candidate in tail.candidates.iter_mut().flatten() {
            if let Some(content) = &mut candidate.content {
                content.parts = Some(Vec::new());
            }
        }
        let mut out = Vec::new();
        self.emit(&mut out, tail)?;
        let mut actual = self
            .target
            .take()
            .ok_or_else(|| invalid("stream", "target consumed"))?
            .finish()?
            .value;
        normalize_gemini(&mut actual);
        normalize_gemini(&mut expected);
        if actual != expected {
            return Err(invalid(
                "stream.parity",
                "native Gemini output differs from completed Claude conversion",
            ));
        }
        Ok(StreamEnd {
            chunks: out,
            identities: self.flow,
            report: converted.report,
        })
    }
}
