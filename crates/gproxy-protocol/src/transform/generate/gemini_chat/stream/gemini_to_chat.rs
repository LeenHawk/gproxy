use super::{
    common::{Budget, StreamEnd, chat_finish, id},
    *,
};
use crate::transform::generate::stream::{
    chat,
    gemini::{GeminiStreamCollector, GeminiStreamLimits},
};
/// Factual invocation timestamp; `model` is an actual model identity when the
/// source does not supply one. Without either model fact, chunks remain bounded
/// and pending until the source supplies its model version.
#[derive(Debug, Clone)]
pub struct GeminiToChatContext {
    pub created: i64,
    pub model: Option<String>,
}
pub struct GeminiToChatStream {
    source: Option<GeminiStreamCollector>,
    flow: IdentityFlow,
    budget: Budget,
    limits: StreamLimits,
    calls: super::super::content::Calls,
    tool_indexes: BTreeMap<i64, i64>,
    context: GeminiToChatContext,
    response_id: Option<String>,
    model: Option<String>,
    pending: Vec<g::GenerateContentResponseBody>,
    emitted: bool,
    failed: bool,
    report: Report,
}
impl GeminiToChatStream {
    pub fn new(
        context: GeminiToChatContext,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Result<Self, TransformError> {
        if context.created < 0 || context.model.as_ref().is_some_and(|v| v.is_empty()) {
            return Err(invalid("invalid factual creation/model context"));
        }
        if let Some(model) = &context.model {
            super::common::bounded(model, limits.max_bytes)?;
        }
        Ok(Self {
            source: Some(GeminiStreamCollector::new(GeminiStreamLimits {
                max_events: limits.max_events,
                max_bytes: limits.max_bytes,
                max_candidates: limits.max_choices,
                max_parts: limits.max_events,
            })),
            flow,
            budget: Budget::new(limits),
            limits,
            calls: Default::default(),
            tool_indexes: BTreeMap::new(),
            model: context.model.clone(),
            context,
            response_id: None,
            pending: Vec::new(),
            emitted: false,
            failed: false,
            report: Default::default(),
        })
    }
    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        chunk: g::GenerateContentResponseBody,
    ) -> Result<Converted<Vec<s::ChatCompletionChunk>>, TransformError> {
        if self.failed {
            return Err(invalid("stream already failed"));
        }
        let result = self.push_inner(chunk.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_inner(
        &mut self,
        chunk: g::GenerateContentResponseBody,
    ) -> Result<Converted<Vec<s::ChatCompletionChunk>>, TransformError> {
        self.budget.input(&chunk)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(chunk.clone())?;
        if let Some(model) = &chunk.model_version {
            if self.emitted && self.model.as_ref().is_some_and(|v| v != model) {
                return Err(invalid(
                    "source model contradicts already emitted model fact",
                ));
            }
            self.model = Some(model.clone());
        }
        if let Some(value) = &chunk.response_id {
            self.response_id = Some(value.clone());
        }
        if self.model.is_none() {
            self.pending.push(chunk);
            return Ok(Converted {
                value: Vec::new(),
                report: Report::default(),
            });
        }
        let mut output = Vec::new();
        for pending in std::mem::take(&mut self.pending) {
            self.content(pending, &mut output)?;
        }
        self.content(chunk, &mut output)?;
        self.emitted |= !output.is_empty();
        Ok(Converted {
            value: output,
            report: std::mem::take(&mut self.report),
        })
    }
    fn content(
        &mut self,
        chunk: g::GenerateContentResponseBody,
        out: &mut Vec<s::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        let policy = TargetIdPolicy::new(crate::Dialect::OpenAiChat);
        // Resolve again when a late source ID arrives. IdentityFlow attaches it to
        // the same logical response without changing an already emitted client ID.
        let response = id(
            &mut self.flow,
            &policy,
            IdentityRole::Response,
            crate::Dialect::Gemini,
            self.response_id.clone(),
            0,
        )?;
        for candidate in chunk.candidates.into_iter().flatten() {
            let index = candidate.index.unwrap_or(0);
            if let Some(reason) = candidate.finish_reason
                && reason != g::FinishReason::Unspecified
            {
                chat_finish(
                    reason,
                    self.tool_indexes.get(&index).is_some_and(|n| *n > 0),
                )?;
            }
            let Some(mut content) = candidate.content else {
                continue;
            };
            if content.role.is_none() {
                content.role = Some("model".into());
            }
            if content.role.as_deref() != Some("model") {
                return Err(invalid("generated candidate must have model role"));
            }
            let incoming_tools = content
                .parts
                .iter()
                .flatten()
                .filter(|p| p.function_call.is_some())
                .count();
            if self.calls.index.saturating_add(incoming_tools as u64) > self.limits.max_tools as u64
            {
                return Err(limit());
            }
            for part in content.parts.iter().flatten() {
                if part
                    .function_call
                    .as_ref()
                    .is_some_and(|call| call.name.is_empty())
                {
                    return Err(TransformError::missing_metadata("function_call.name"));
                }
            }
            let messages = super::super::content::gemini_content_to_chat(
                content,
                &mut self.report,
                &mut self.flow,
                &policy,
                &mut self.calls,
            )?;
            for message in messages {
                let c::ChatMessage::Assistant(message) = message else {
                    return Err(invalid(
                        "model candidate produced nonassistant Chat message",
                    ));
                };
                let mut delta = s::Delta::builder().role(s::DeltaRole::Assistant).build();
                let mut has_content = false;
                if let Some(content) = message.content.flatten() {
                    let mut text = String::new();
                    match content {
                        c::AssistantContent::Text(v) => text = v,
                        c::AssistantContent::Parts(parts) => {
                            for part in parts {
                                match part {
                                    c::AssistantContentPart::Text(v) => text.push_str(&v.text),
                                    c::AssistantContentPart::Refusal(_) => {
                                        return Err(invalid(
                                            "unexpected refusal in model text mapping",
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    delta.content = Some(Some(text));
                    has_content = true;
                }
                let mut tools = Vec::new();
                for tool in message.tool_calls.into_iter().flatten() {
                    let c::MessageToolCall::Function(tool) = tool else {
                        return Err(invalid("Gemini function produced nonfunction call"));
                    };
                    let next = self.tool_indexes.entry(index).or_default();
                    let mut call = s::DeltaToolCall::builder(*next).build();
                    *next = next.checked_add(1).ok_or_else(limit)?;
                    call.id = Some(Some(tool.id));
                    call.type_ = Some(Some(s::DeltaToolCallType::Function));
                    call.function = Some(Some(
                        s::DeltaFunctionCall::builder()
                            .name(tool.function.name)
                            .arguments(tool.function.arguments)
                            .build(),
                    ));
                    tools.push(call);
                }
                if !tools.is_empty() {
                    delta.tool_calls = Some(Some(tools));
                    has_content = true;
                }
                if has_content {
                    let chunk = self.chunk(
                        response.clone(),
                        vec![s::StreamChoice::builder(index, delta).build()],
                    );
                    self.emit(chunk, out)?;
                }
            }
        }
        Ok(())
    }
    fn emit(
        &mut self,
        chunk: s::ChatCompletionChunk,
        out: &mut Vec<s::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        // Meter before retaining each chunk, including a late-model flush of
        // many pending source events. Never multiply metadata across an
        // unchecked batch.
        self.budget.output(std::slice::from_ref(&chunk))?;
        out.push(chunk);
        Ok(())
    }
    fn chunk(&self, response: String, choices: Vec<s::StreamChoice>) -> s::ChatCompletionChunk {
        s::ChatCompletionChunk::builder(
            response,
            choices,
            self.context.created,
            self.model.clone().expect("model checked before emission"),
            s::ChunkObject::ChatCompletionChunk,
        )
        .build()
    }
    pub fn finish(mut self) -> Result<StreamEnd<s::ChatCompletionChunk>, TransformError> {
        if self.failed {
            return Err(invalid("stream failed before finish"));
        }
        let source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?
            .value;
        if self.model.is_none() {
            return Err(TransformError::missing_metadata(
                "response.model_version or factual context.model",
            ));
        }
        let mut chunks = Vec::new();
        for pending in std::mem::take(&mut self.pending) {
            self.content(pending, &mut chunks)?;
        }
        let policy = TargetIdPolicy::new(crate::Dialect::OpenAiChat);
        let response = id(
            &mut self.flow,
            &policy,
            IdentityRole::Response,
            crate::Dialect::Gemini,
            source.response_id,
            0,
        )?;
        let candidates = source
            .candidates
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("prompt blocked without generated candidates"))?;
        let mut choices = Vec::new();
        for (expected, candidate) in candidates.into_iter().enumerate() {
            let index = candidate.index.unwrap_or(0);
            if index != expected as i64 {
                return Err(invalid(
                    "final candidate indexes cannot form a complete native Chat choice set",
                ));
            }
            let finish = chat_finish(
                candidate
                    .finish_reason
                    .ok_or_else(|| TransformError::missing_metadata("candidate.finish_reason"))?,
                self.tool_indexes.get(&index).is_some_and(|n| *n > 0),
            )?;
            let logs = candidate
                .logprobs_result
                .map(|v| super::super::logs::to_chat(v, &mut self.report))
                .transpose()?
                .map(chat::logs::synthesize)
                .transpose()?;
            let mut choice = s::StreamChoice::builder(
                index,
                s::Delta::builder().role(s::DeltaRole::Assistant).build(),
            )
            .finish_reason(finish)
            .build();
            choice.logprobs = logs.map(Some);
            choices.push(choice);
            for (present, field) in [
                (candidate.safety_ratings.is_some(), "safety_ratings"),
                (candidate.citation_metadata.is_some(), "citation_metadata"),
                (
                    candidate.grounding_attributions.is_some(),
                    "grounding_attributions",
                ),
                (candidate.grounding_metadata.is_some(), "grounding_metadata"),
                (candidate.avg_logprobs.is_some(), "avg_logprobs"),
                (
                    candidate.url_context_metadata.is_some(),
                    "url_context_metadata",
                ),
                (candidate.token_count.is_some(), "token_count"),
                (candidate.finish_message.is_some(), "finish_message"),
            ] {
                if present {
                    self.report.omitted(
                        format!("candidate.{field}"),
                        "Chat stream has no corresponding declared metadata field",
                    );
                }
            }
        }
        let terminal = self.chunk(response.clone(), choices);
        self.emit(terminal, &mut chunks)?;
        if let Some(usage) = source.usage_metadata {
            let usage = super::super::usage::to_chat(&usage, &mut self.report)?;
            let mut tail = self.chunk(response, Vec::new());
            tail.usage = Some(Some(chat::usage::synthesize(usage)?));
            self.emit(tail, &mut chunks)?;
        }
        if source.prompt_feedback.is_some() {
            self.report.omitted(
                "prompt_feedback",
                "Chat has no equivalent Gemini prompt feedback",
            );
        }
        if source.model_status.is_some() {
            self.report
                .omitted("model_status", "Chat has no Gemini model-status field");
        }
        Ok(StreamEnd {
            chunks,
            identities: self.flow,
            report: self.report,
        })
    }
}
