use super::{
    common::{Budget, StreamEnd, gemini_finish},
    tools::{ChatChoice, ChatTool},
    *,
};
use crate::transform::generate::stream::chat::{ChatStreamCollector, ChatStreamLimits};

/// Text is emitted immediately. Function arguments remain bounded strings until
/// their choice finishes because Gemini requires one complete JSON object.
pub struct ChatToGeminiStream {
    source: Option<ChatStreamCollector>,
    policy: TargetIdPolicy,
    flow: IdentityFlow,
    budget: Budget,
    limits: StreamLimits,
    choices: BTreeMap<i64, ChatChoice>,
    next_tool: u64,
    failed: bool,
    done: bool,
    response_id: Option<String>,
    model: Option<String>,
    report: Report,
}

impl ChatToGeminiStream {
    pub fn new(flow: IdentityFlow, limits: StreamLimits) -> Self {
        Self::new_with_policy(flow, limits, TargetIdPolicy::new(crate::Dialect::Gemini))
            .expect("default policy matches target dialect")
    }
    pub fn new_with_policy(
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

        let source = ChatStreamCollector::with_limits(
            IdentityFlow::new(flow.namespace()),
            TargetIdPolicy::new(crate::Dialect::OpenAiChat),
            ChatStreamLimits {
                max_events: limits.max_events,
                max_bytes: limits.max_bytes,
                max_choices: limits.max_choices,
                max_tool_calls: limits.max_tools,
            },
        );
        Ok(Self {
            source: Some(source),
            policy,
            flow,
            budget: Budget::new(limits),
            limits,
            choices: BTreeMap::new(),
            next_tool: 0,
            failed: false,
            done: false,
            response_id: None,
            model: None,
            report: Default::default(),
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        chunk: s::ChatCompletionChunk,
    ) -> Result<Converted<Vec<g::GenerateContentResponseBody>>, TransformError> {
        if self.failed || self.done {
            self.failed = true;
            return Err(invalid("event after DONE or failed stream"));
        }
        let result = self.push_inner(chunk.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_inner(
        &mut self,
        chunk: s::ChatCompletionChunk,
    ) -> Result<Converted<Vec<g::GenerateContentResponseBody>>, TransformError> {
        self.budget.input(&chunk)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(chunk.clone())?;
        if !chunk.id.is_empty() {
            self.response_id = Some(super::common::id(
                &mut self.flow,
                &self.policy,
                IdentityRole::Response,
                crate::Dialect::OpenAiChat,
                Some(chunk.id.clone()),
                0,
            )?);
        }
        if !chunk.model.is_empty() {
            self.model = Some(chunk.model.clone());
        }
        let mut output = Vec::new();
        for choice in chunk.choices {
            let index = choice.index;
            let d = choice.delta;
            if !self.choices.contains_key(&index) && self.choices.len() >= self.limits.max_choices {
                return Err(limit());
            }
            let state = self.choices.entry(index).or_default();
            if let Some(text) = d.content.flatten() {
                output.push(candidate(
                    index,
                    vec![g::Part::builder().text(text).build()],
                ));
            }
            if let Some(refusal) = d.refusal.flatten() {
                state.refusal.get_or_insert_default().push_str(&refusal);
            }
            for call in d.tool_calls.flatten().into_iter().flatten() {
                let tool = match state.tools.entry(call.index) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        if self.next_tool >= self.limits.max_tools as u64 {
                            return Err(limit());
                        }
                        let ordinal = self.next_tool;
                        self.next_tool += 1;
                        entry.insert(ChatTool {
                            ordinal,
                            ..Default::default()
                        })
                    }
                };
                tool.append(call.id.flatten(), call.function.flatten())?;
            }
            if let Some(function) = d.function_call.flatten() {
                let legacy = match &mut state.legacy {
                    Some(tool) => tool,
                    slot => {
                        if self.next_tool >= self.limits.max_tools as u64 {
                            return Err(limit());
                        }
                        let ordinal = self.next_tool;
                        self.next_tool += 1;
                        slot.insert(ChatTool {
                            ordinal,
                            ..Default::default()
                        })
                    }
                };
                legacy.append(None, Some(function))?;
            }
            if choice.finish_reason.flatten().is_some() {
                let mut parts = Vec::new();
                if let Some(refusal) = state.refusal.take() {
                    parts.push(g::Part::builder().text(refusal).build());
                    self.report.changed("refusal","Gemini represents refusal as text; the source finish reason controls the terminal reason");
                }
                if let Some(tool) = state.legacy.take() {
                    parts.push(tool.part(&mut self.flow, true, &self.policy)?);
                }
                for (_, tool) in std::mem::take(&mut state.tools) {
                    parts.push(tool.part(&mut self.flow, false, &self.policy)?);
                }
                if !parts.is_empty() {
                    output.push(candidate(index, parts));
                }
            }
        }
        let mut metered = Vec::new();
        for mut body in output {
            body.response_id = self.response_id.clone();
            body.model_version = self.model.clone();
            // Retain only a bounded batch; metadata may be much larger than
            // the small text/tool fragment associated with an individual choice.
            self.budget.output(std::slice::from_ref(&body))?;
            metered.push(body);
        }
        Ok(Converted {
            value: metered,
            report: std::mem::take(&mut self.report),
        })
    }
    pub fn push_done(&mut self) -> Result<(), TransformError> {
        if self.failed || self.done {
            self.failed = true;
            return Err(invalid("duplicate DONE or failed stream"));
        }
        let result = self
            .source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push_done();
        if result.is_err() {
            self.failed = true;
        } else {
            self.done = true;
        }
        result
    }
    pub fn finish(mut self) -> Result<StreamEnd<g::GenerateContentResponseBody>, TransformError> {
        if self.failed || !self.done {
            return Err(invalid("Chat stream ended without valid SSE DONE"));
        }
        let source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?;
        self.report.diagnostics.extend(source.report.diagnostics);
        let source = source.value;
        let mut candidates = Vec::new();
        for choice in source.choices {
            let mut candidate = g::Candidate::builder()
                .index(choice.index)
                // A completed Chat choice always contains an assistant message,
                // including an explicitly empty one. Empty parts add no text.
                .content(
                    g::Content::builder()
                        .role("model")
                        .parts(Vec::new())
                        .build(),
                )
                .finish_reason(gemini_finish(choice.finish_reason))
                .build();
            candidate.logprobs_result = choice
                .logprobs
                .as_ref()
                .map(|v| super::super::logs::to_gemini(v, &mut self.report))
                .map(crate::transform::optional)
                .transpose()?
                .flatten();
            candidates.push(candidate);
        }
        let usage = source
            .usage
            .as_ref()
            .map(|u| super::super::usage::to_gemini(u, &mut self.report))
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        let mut tail = g::GenerateContentResponseBody::builder()
            .candidates(candidates)
            .response_id(super::common::id(
                &mut self.flow,
                &self.policy,
                IdentityRole::Response,
                crate::Dialect::OpenAiChat,
                Some(source.id),
                0,
            )?)
            .model_version(source.model)
            .build();
        tail.usage_metadata = usage;
        if source.service_tier.is_some() {
            self.report.omitted(
                "service_tier",
                "Gemini response has no equivalent Chat service tier",
            );
        }
        if source.system_fingerprint.is_some() {
            self.report.omitted(
                "system_fingerprint",
                "Gemini has no corresponding fingerprint field",
            );
        }
        if source.moderation.is_some() {
            self.report.omitted(
                "moderation",
                "Chat moderation has no equivalent Gemini metadata schema",
            );
        }
        self.report
            .omitted("created", "Gemini response has no creation timestamp field");
        let chunks = vec![tail];
        self.budget.output(&chunks)?;
        Ok(StreamEnd {
            chunks,
            identities: self.flow,
            report: self.report,
        })
    }
}

fn candidate(index: i64, parts: Vec<g::Part>) -> g::GenerateContentResponseBody {
    g::GenerateContentResponseBody::builder()
        .candidates(vec![
            g::Candidate::builder()
                .index(index)
                .content(g::Content::builder().role("model").parts(parts).build())
                .build(),
        ])
        .build()
}
