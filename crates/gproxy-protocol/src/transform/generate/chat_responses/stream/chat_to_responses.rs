use crate::transform::generate::reasoning_details as rd;
mod blocks;
use super::common::{Budget, StreamEnd, StreamLimits, declared, invalid, limit};
use crate::transform::generate::stream::{
    chat::{ChatStreamCollector, ChatStreamLimits},
    responses::ResponsesStreamCollector,
};
use crate::{
    Dialect,
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, OutputItemKind, SourceIdentity, TargetIdPolicy},
    },
    wire::openai::{
        chat::{response as c, stream as cs},
        responses::{input as i, response as r, stream as rs},
    },
};
use std::collections::BTreeMap;

/// Original Responses request and effective controls retained during request
/// preparation. A stream cannot invent these fields from a Chat chunk.
pub struct ChatToResponsesContext {
    pub response: super::super::response::ResponsesResponseContext,
}

impl From<super::super::response::ResponsesResponseContext> for ChatToResponsesContext {
    fn from(response: super::super::response::ResponsesResponseContext) -> Self {
        Self { response }
    }
}

struct MessageState {
    id: String,
    index: i64,
    text: String,
    refusal: String,
    text_part: bool,
    refusal_part: bool,
    logs: Option<c::Logprobs>,
}

struct ToolState {
    source_id: Option<String>,
    name: String,
    arguments: String,
}

pub struct ChatToResponsesStream {
    source: Option<ChatStreamCollector>,
    target: Option<ResponsesStreamCollector>,
    context: Option<super::super::response::ResponsesResponseContext>,
    flow: IdentityFlow,
    target_policy: TargetIdPolicy,
    limits: StreamLimits,
    budget: Budget,
    sequence: i64,
    response_id: Option<String>,
    model: Option<String>,
    created: Option<i64>,
    base: Option<r::GenerateContentResponseBody>,
    message: Option<MessageState>,
    reasoning_text: String,
    reasoning_details: Vec<crate::wire::openai::chat::ReasoningDetail>,
    tools: BTreeMap<i64, ToolState>,
    client_tools: super::super::client_tools::Bindings,
    next_output: i64,
    finish: Option<c::FinishReason>,
    events: Vec<rs::StreamEvent>,
    pending: Vec<cs::ChatCompletionChunk>,
    report: Report,
    output: BTreeMap<i64, r::ResponseOutputItem>,
    usage_facts: super::super::response::ChatUsageSupplement,
    failed: bool,
    done: bool,
}

impl ChatToResponsesStream {
    pub fn new(
        context: impl Into<ChatToResponsesContext>,
        flow: IdentityFlow,
        limits: StreamLimits,
    ) -> Self {
        Self::new_with_policy(
            context,
            flow,
            limits,
            TargetIdPolicy::new(crate::Dialect::OpenAi),
        )
        .expect("default policy matches target dialect")
    }
    pub fn new_with_policy(
        context: impl Into<ChatToResponsesContext>,
        flow: IdentityFlow,
        limits: StreamLimits,
        policy: TargetIdPolicy,
    ) -> Result<Self, TransformError> {
        if policy.dialect != crate::Dialect::OpenAi {
            return Err(TransformError::shape(
                "stream.policy",
                "target policy dialect mismatch",
            ));
        }

        let mut context = context.into();
        let client_tools = super::super::client_tools::Bindings::new(&context.response.request)?;
        context.response.request = declared(context.response.request);
        context.response.effective_tool_choice = declared(context.response.effective_tool_choice);
        context.response.effective_prompt_cache_options =
            declared(context.response.effective_prompt_cache_options);
        let source = ChatStreamCollector::with_limits(
            IdentityFlow::new(flow.namespace()),
            TargetIdPolicy::new(Dialect::OpenAiChat),
            ChatStreamLimits {
                max_events: limits.max_events,
                max_bytes: limits.max_bytes,
                max_choices: 2,
                max_tool_calls: limits.max_tool_calls,
            },
        );
        let usage_facts = context.response.usage;
        Ok(Self {
            source: Some(source),
            target: Some(ResponsesStreamCollector::new(
                crate::transform::generate::stream::responses::ResponsesStreamLimits {
                    max_events: limits.max_events,
                    max_bytes: limits.max_bytes,
                    max_items: limits.max_items,
                    max_text_bytes: limits.max_bytes,
                    max_json_bytes: limits.max_bytes,
                },
            )),
            context: Some(context.response),
            flow,
            target_policy: policy,
            limits,
            budget: Budget::new(limits),
            sequence: 0,
            response_id: None,
            model: None,
            created: None,
            base: None,
            message: None,
            reasoning_text: String::new(),
            reasoning_details: Vec::new(),
            tools: BTreeMap::new(),
            client_tools,
            next_output: 0,
            finish: None,
            events: Vec::new(),
            pending: Vec::new(),
            report: Report::default(),
            output: BTreeMap::new(),
            usage_facts,
            failed: false,
            done: false,
        })
    }

    pub fn identities(&self) -> &IdentityFlow {
        &self.flow
    }
    pub fn push(
        &mut self,
        chunk: cs::ChatCompletionChunk,
    ) -> Result<Converted<Vec<rs::StreamEvent>>, TransformError> {
        if self.failed || self.done {
            self.failed = true;
            return Err(invalid("event after DONE or failed stream"));
        }
        let result = self.push_inner(declared(chunk));
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn push_inner(
        &mut self,
        chunk: cs::ChatCompletionChunk,
    ) -> Result<Converted<Vec<rs::StreamEvent>>, TransformError> {
        self.budget.input(&chunk)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(chunk.clone())?;
        if chunk.created < 0 {
            return Err(TransformError::missing_metadata("chat.created/model"));
        }
        if self.created.is_some_and(|v| v != chunk.created) {
            return Err(invalid("created changed during stream"));
        }
        self.created = Some(chunk.created);
        if !chunk.id.is_empty() {
            self.response_id = Some(chunk.id.clone());
        }
        if !chunk.model.is_empty() {
            self.model = Some(chunk.model.clone());
        }
        // Bind a late upstream response ID without replacing the emitted alias.
        if self.base.is_some() {
            self.flow
                .resolve_or_allocate(
                    IdentityRole::Response,
                    SourceIdentity::new(Dialect::OpenAiChat, self.response_id.clone(), 0),
                    &self.target_policy,
                )
                .map_err(|e| invalid_owned(e.to_string()))?;
        }
        if self.model.is_none() {
            self.pending.push(chunk);
            return Ok(Converted {
                value: Vec::new(),
                report: std::mem::take(&mut self.report),
            });
        }
        self.start_if_needed()?;
        let mut pending = std::mem::take(&mut self.pending);
        pending.push(chunk);
        for chunk in pending {
            if chunk.choices.len() > 1 || chunk.choices.iter().any(|v| v.index != 0) {
                return Err(TransformError::unsupported(
                    "choices",
                    "Chat→Responses stream requires one choice; fan-out must be a separate invocation",
                ));
            }
            for choice in chunk.choices {
                let choice = declared(choice);
                self.apply_choice(choice)?;
            }
        }
        Ok(Converted {
            value: std::mem::take(&mut self.events),
            report: std::mem::take(&mut self.report),
        })
    }

    fn start_if_needed(&mut self) -> Result<(), TransformError> {
        if self.base.is_some() {
            return Ok(());
        }
        let source_id = self.response_id.clone();
        let created = self
            .created
            .ok_or_else(|| TransformError::missing_metadata("chat.created"))?;
        let model = self
            .model
            .clone()
            .ok_or_else(|| TransformError::missing_metadata("chat.model"))?;
        let id = self
            .flow
            .resolve_or_allocate(
                IdentityRole::Response,
                SourceIdentity::new(Dialect::OpenAiChat, source_id, 0),
                &self.target_policy,
            )
            .map_err(|e| TransformError::invalid_result("identity", e.to_string()))?
            .emitted_id;
        let context = self
            .context
            .take()
            .ok_or_else(|| invalid("missing response context"))?;
        super::common::measure(&context.request, self.limits.max_bytes)?;
        super::common::measure(&context.effective_tool_choice, self.limits.max_bytes)?;
        let mut base = context.into_response(id, created, model)?;
        super::common::measure(&base, self.limits.max_bytes)?;
        base.status = Some(r::ResponseStatus::InProgress);
        base.output.clear();
        base.usage = None;
        base.output_text = None;
        base.completed_at = None;
        base.error = None;
        base.incomplete_details = None;
        self.emit(rs::StreamEvent::Created(rs::ResponseCreated {
            sequence_number: self.sequence,
            response: base.clone(),
            rest: Default::default(),
        }))?;
        self.base = Some(base);
        Ok(())
    }

    fn apply_choice(&mut self, choice: cs::StreamChoice) -> Result<(), TransformError> {
        if let Some(Some(logs)) = choice.logprobs {
            self.message_mut()?;
            crate::transform::generate::stream::chat::logs::append(
                &mut self.message.as_mut().unwrap().logs,
                logs,
            )?;
        }
        let d = choice.delta;
        if let Some(text) =
            crate::wire::openai::chat::reasoning_text(&d.reasoning_content, &d.reasoning)
        {
            self.reasoning_text.push_str(text);
        }
        if let Some(Some(details)) = d.reasoning_details {
            rd::merge(&mut self.reasoning_details, details)?;
        }

        if let Some(Some(role)) = d.role
            && role != cs::DeltaRole::Assistant
        {
            return Err(invalid("Chat stream delta role must be assistant"));
        }
        if let Some(Some(text)) = d.content {
            self.message_mut()?;
            self.message_text(text)?;
        }
        if let Some(Some(text)) = d.refusal {
            self.message_mut()?;
            let state = self.message.as_mut().unwrap();
            state.refusal_part = true;
            state.refusal.push_str(&text);
        }
        if let Some(Some(function)) = d.function_call {
            self.tool_delta(-1, None, function)?;
        }
        if let Some(Some(calls)) = d.tool_calls {
            for call in calls {
                if call.index < 0 {
                    return Err(invalid("negative tool index"));
                }
                self.tool_delta(
                    call.index,
                    call.id.flatten(),
                    call.function
                        .flatten()
                        .unwrap_or_else(|| cs::DeltaFunctionCall::builder().build()),
                )?;
            }
        }
        if let Some(Some(reason)) = choice.finish_reason {
            self.finish = Some(reason);
            self.finish_choice()?;
        }
        Ok(())
    }

    fn emit(&mut self, event: rs::StreamEvent) -> Result<(), TransformError> {
        self.budget.output(&event)?;
        self.target
            .as_mut()
            .ok_or_else(|| invalid("target consumed"))?
            .push(event.clone())?;
        self.sequence = self.sequence.checked_add(1).ok_or_else(limit)?;
        self.events.push(event);
        Ok(())
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

    pub fn finish(mut self) -> Result<StreamEnd<rs::StreamEvent>, TransformError> {
        if self.failed || !self.done {
            return Err(invalid("Chat stream ended without valid SSE DONE"));
        }
        let source = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?;
        self.report.diagnostics.extend(source.report.diagnostics);
        let final_chat = source.value;
        if final_chat.choices.len() != 1 {
            return Err(TransformError::unsupported(
                "choices",
                "one Responses response requires one Chat choice",
            ));
        }
        let usage = final_chat
            .usage
            .map(|u| {
                super::super::response::usage::to_responses(u, self.usage_facts, &mut self.report)
            })
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        let mut response = self
            .base
            .take()
            .ok_or_else(|| invalid("missing response.created"))?;
        response.output = self.current_output();
        response.usage = usage.map(Some);
        response.status = Some(
            if matches!(
                self.finish,
                Some(c::FinishReason::Length | c::FinishReason::ContentFilter)
            ) {
                r::ResponseStatus::Incomplete
            } else {
                r::ResponseStatus::Completed
            },
        );
        response.incomplete_details = match self.finish {
            Some(c::FinishReason::Length) => Some(r::ResponseIncompleteDetails {
                reason: Some(r::ResponseIncompleteReason::MaxOutputTokens),
                rest: Default::default(),
            }),
            Some(c::FinishReason::ContentFilter) => Some(r::ResponseIncompleteDetails {
                reason: Some(r::ResponseIncompleteReason::ContentFilter),
                rest: Default::default(),
            }),
            _ => None,
        };
        response.service_tier = final_chat
            .service_tier
            .map(|v| v.map(super::super::response::tier_to_responses));
        response.moderation = final_chat
            .moderation
            .map(|v| {
                v.map(|v| super::super::response::moderation::to_responses(v, &mut self.report))
                    .transpose()
            })
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        if final_chat.system_fingerprint.is_some() {
            self.report
                .omitted("system_fingerprint", "Responses has no fingerprint field");
        }
        let terminal = if response.status == Some(r::ResponseStatus::Incomplete) {
            rs::StreamEvent::Incomplete(rs::ResponseIncomplete {
                sequence_number: self.sequence,
                response,
                rest: Default::default(),
            })
        } else {
            rs::StreamEvent::Completed(rs::ResponseCompleted {
                sequence_number: self.sequence,
                response,
                rest: Default::default(),
            })
        };
        self.emit(terminal)?;
        self.target
            .take()
            .ok_or_else(|| invalid("target consumed"))?
            .finish()?;
        Ok(StreamEnd {
            chunks: self.events,
            identities: self.flow,
            report: self.report,
        })
    }

    fn current_output(&self) -> Vec<r::ResponseOutputItem> {
        self.output.values().cloned().collect()
    }
}

fn invalid_owned(message: String) -> TransformError {
    TransformError::invalid_result("chat_responses.stream", message)
}
