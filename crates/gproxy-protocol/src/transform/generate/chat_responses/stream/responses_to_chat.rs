mod content;
use super::common::{Budget, StreamEnd, StreamLimits, declared, invalid, unsupported};
use crate::{
    Dialect,
    transform::generate::stream::{
        chat::{ChatStreamCollector, ChatStreamLimits},
        responses::{ResponsesStreamCollector, ResponsesStreamLimits},
    },
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::openai::{
        chat::{response as c, stream as cs},
        responses::{input as i, response as r, stream as rs},
    },
};
use std::collections::BTreeMap;

pub(super) struct TextPart {
    pub pending: String,
    pub refusal: bool,
    pub done: bool,
    pub emitted: bool,
}

pub(super) enum ItemKind {
    Excluded,
    Message {
        parts: BTreeMap<i64, TextPart>,
        cursor: i64,
        done: bool,
    },
    Function {
        call_id: String,
        name: String,
        tool_index: i64,
        sent: bool,
    },
    Reasoning,
}

pub(super) struct ItemState {
    pub id: Option<String>,
    pub kind: ItemKind,
}

/// Each push returns only newly emitted target chunks. Source item/part order is
/// retained when concurrent Responses messages feed Chat's single content field.
pub struct ResponsesToChatStream {
    pub(super) source: Option<ResponsesStreamCollector>,
    pub(super) target: Option<ChatStreamCollector>,
    pub(super) flow: IdentityFlow,
    pub(super) policy: TargetIdPolicy,
    pub(super) budget: Budget,
    pub(super) response_id: Option<String>,
    pub(super) model: Option<String>,
    pub(super) created: Option<i64>,
    pub(super) items: BTreeMap<i64, ItemState>,
    pub(super) content_cursor: i64,
    pub(super) tools: usize,
    pub(super) report: Report,
    pub(super) expected: Option<c::GenerateContentResponseBody>,
    pub(super) terminal: bool,
    pub(super) failed: bool,
}

impl ResponsesToChatStream {
    pub fn new(flow: IdentityFlow, limits: StreamLimits) -> Self {
        Self::new_with_policy(
            flow,
            limits,
            TargetIdPolicy::new(crate::Dialect::OpenAiChat),
        )
        .expect("default policy matches target dialect")
    }
    pub fn new_with_policy(
        flow: IdentityFlow,
        limits: StreamLimits,
        policy: TargetIdPolicy,
    ) -> Result<Self, TransformError> {
        if policy.dialect != crate::Dialect::OpenAiChat {
            return Err(TransformError::shape(
                "stream.policy",
                "target policy dialect mismatch",
            ));
        }

        // These IDs were already allocated under the selected policy. The
        // native target collector must preserve them while checking syntax;
        // preserve_source_ids=false must not trigger a second allocation.
        let mut collector_policy = policy.clone();
        collector_policy.preserve_source_ids = true;
        Ok(Self {
            source: Some(ResponsesStreamCollector::new(ResponsesStreamLimits {
                max_bytes: limits.max_bytes,

                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            })),
            target: Some(ChatStreamCollector::with_limits(
                IdentityFlow::new(flow.namespace()),
                collector_policy,
                ChatStreamLimits {
                    max_bytes: limits.max_bytes,
                },
            )),
            flow,
            policy,
            budget: Budget::new(limits),
            response_id: None,
            model: None,
            created: None,
            items: BTreeMap::new(),
            content_cursor: 0,
            tools: 0,
            report: Report::default(),
            expected: None,
            terminal: false,
            failed: false,
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
        event: rs::StreamEvent,
    ) -> Result<Converted<Vec<cs::ChatCompletionChunk>>, TransformError> {
        if self.failed || self.terminal {
            self.failed = true;
            return Err(invalid("event after terminal or failed stream"));
        }
        let result = self.push_inner(declared(event));
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_inner(
        &mut self,
        event: rs::StreamEvent,
    ) -> Result<Converted<Vec<cs::ChatCompletionChunk>>, TransformError> {
        let event = crate::transform::generate::multi_agent::attribute_event(event)?;
        self.budget.input(&event)?;
        self.source
            .as_mut()
            .ok_or_else(|| invalid("source consumed"))?
            .push(event.clone())?;
        let mut out = Vec::new();
        match event {
            rs::StreamEvent::Created(v) => {
                let id = self
                    .flow
                    .resolve_or_allocate(
                        IdentityRole::Response,
                        SourceIdentity::new(Dialect::OpenAi, Some(v.response.id), 0),
                        &self.policy,
                    )
                    .map_err(|e| {
                        TransformError::invalid_result("identity.response", e.to_string())
                    })?
                    .emitted_id;
                self.response_id = Some(id);
                self.model = Some(v.response.model);
                self.created = Some(v.response.created_at);
                self.emit_delta(
                    cs::Delta::builder().role(cs::DeltaRole::Assistant).build(),
                    &mut out,
                )?;
                for (index, item) in v.response.output.into_iter().enumerate() {
                    self.add_item(index as i64, item, &mut out)?;
                }
            }
            rs::StreamEvent::OutputItemAdded(v) => {
                self.add_item(v.output_index, v.item, &mut out)?
            }
            rs::StreamEvent::ContentPartAdded(v) => {
                self.add_part(v.output_index, v.content_index, v.part)?
            }
            rs::StreamEvent::OutputTextDelta(v) => {
                self.append_text(v.output_index, v.content_index, &v.item_id, v.delta, false)?
            }
            rs::StreamEvent::RefusalDelta(v) => {
                self.append_text(v.output_index, v.content_index, &v.item_id, v.delta, true)?
            }
            rs::StreamEvent::ContentPartDone(v) => {
                if let Some(ItemState {
                    kind: ItemKind::Message { parts, .. },
                    ..
                }) = self.items.get_mut(&v.output_index)
                {
                    parts
                        .get_mut(&v.content_index)
                        .ok_or_else(|| invalid("part done without part"))?
                        .done = true;
                }
            }
            rs::StreamEvent::OutputItemDone(v) => {
                self.bind_item(v.output_index, item_id(&v.item))?;
                if let Some(ItemState {
                    kind: ItemKind::Message { parts, done, .. },
                    ..
                }) = self.items.get_mut(&v.output_index)
                {
                    *done = true;
                    for p in parts.values_mut() {
                        p.done = true;
                    }
                }
            }
            rs::StreamEvent::FunctionCallArgumentsDelta(v) => {
                self.bind_item(v.output_index, Some(&v.item_id))?;
                self.emit_tool(v.output_index, v.delta, &mut out)?;
            }
            rs::StreamEvent::FunctionCallArgumentsDone(_) => {}
            rs::StreamEvent::Completed(_) | rs::StreamEvent::Incomplete(_) => {
                self.finish_source(&mut out)?;
            }
            rs::StreamEvent::Queued(_)
            | rs::StreamEvent::InProgress(_)
            | rs::StreamEvent::OutputTextDone(_)
            | rs::StreamEvent::RefusalDone(_) => {}
            rs::StreamEvent::OutputTextAnnotationAdded(_) => self.report.omitted(
                "response.annotations",
                "Chat stream Delta has no annotation field",
            ),
            rs::StreamEvent::ReasoningTextDelta(_)
            | rs::StreamEvent::ReasoningSummaryTextDelta(_) => {}
            rs::StreamEvent::ReasoningTextDone(_)
            | rs::StreamEvent::ReasoningSummaryTextDone(_)
            | rs::StreamEvent::ReasoningSummaryPartAdded(_)
            | rs::StreamEvent::ReasoningSummaryPartDone(_) => {}
            rs::StreamEvent::Failed(_) | rs::StreamEvent::Error(_) => {
                return Err(invalid("Responses stream failed"));
            }
            rs::StreamEvent::CustomToolInputDelta(_) | rs::StreamEvent::CustomToolInputDone(_) => {
                return Err(unsupported(
                    "custom_tool",
                    "Chat streaming tool schema supports only function calls; buffered Chat has a separate custom-tool shape",
                ));
            }
            rs::StreamEvent::AudioDelta(_)
            | rs::StreamEvent::AudioDone(_)
            | rs::StreamEvent::AudioTranscriptDelta(_)
            | rs::StreamEvent::AudioTranscriptDone(_) => {
                return Err(unsupported(
                    "audio",
                    "this native stream wire has no canonical output audio item",
                ));
            }
            _ => {
                return Err(unsupported(
                    "response.execution",
                    "native execution events require an invocation adapter",
                ));
            }
        }
        if !self.terminal {
            self.flush_text(&mut out)?;
        }
        Ok(Converted {
            value: out,
            report: std::mem::take(&mut self.report),
        })
    }
    fn finish_source(
        &mut self,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        let actual = self
            .source
            .take()
            .ok_or_else(|| invalid("source consumed"))?
            .finish()?;
        self.report.diagnostics.extend(actual.report.diagnostics);
        let mapped =
            super::super::responses_to_chat_response(actual.value, &mut self.flow, &self.policy)?;
        self.report.diagnostics.extend(mapped.report.diagnostics);
        for item in self.items.values_mut() {
            if let ItemKind::Message { parts, done, .. } = &mut item.kind {
                *done = true;
                for p in parts.values_mut() {
                    p.done = true;
                }
            }
        }
        self.flush_text(out)?;
        let mut expected = mapped.value;
        let message = &expected.choices[0].message;
        if message.reasoning_details.is_some() || message.reasoning_content.is_some() {
            let mut delta = cs::Delta::builder().build();
            delta.reasoning_details = message.reasoning_details.clone();
            delta.reasoning_content = message.reasoning_content.clone();
            self.emit_delta(delta, out)?;
        }
        if expected.choices[0].message.annotations.take().is_some() {
            self.report.omitted(
                "message.annotations",
                "Chat streaming Delta has no annotation field",
            );
        }
        let logs = expected.choices[0]
            .logprobs
            .clone()
            .map(crate::transform::generate::stream::chat::logs::synthesize)
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        let mut choice = cs::StreamChoice::builder(0, cs::Delta::builder().build())
            .finish_reason(expected.choices[0].finish_reason)
            .build();
        choice.logprobs = logs.map(Some);
        let mut terminal = self.chunk(vec![choice])?;
        terminal.service_tier = expected.service_tier;
        terminal.moderation = expected.moderation.clone();
        self.emit(terminal, out)?;
        if let Some(usage) = expected.usage.clone() {
            let usage = crate::transform::generate::stream::chat::usage::synthesize(usage)?;
            let mut chunk = self.chunk(Vec::new())?;
            chunk.usage = Some(Some(usage));
            self.emit(chunk, out)?;
        }
        self.expected = Some(expected);
        self.terminal = true;
        Ok(())
    }
    fn emit_delta(
        &mut self,
        delta: cs::Delta,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        let chunk = self.chunk(vec![cs::StreamChoice::builder(0, delta).build()])?;
        self.emit(chunk, out)
    }
    fn chunk(
        &self,
        choices: Vec<cs::StreamChoice>,
    ) -> Result<cs::ChatCompletionChunk, TransformError> {
        Ok(cs::ChatCompletionChunk::builder(
            self.response_id
                .clone()
                .ok_or_else(|| TransformError::missing_metadata("response.id"))?,
            choices,
            self.created
                .ok_or_else(|| TransformError::missing_metadata("response.created_at"))?,
            self.model
                .clone()
                .ok_or_else(|| TransformError::missing_metadata("response.model"))?,
            cs::ChunkObject::ChatCompletionChunk,
        )
        .build())
    }
    fn emit(
        &mut self,
        chunk: cs::ChatCompletionChunk,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        self.budget.output(&chunk)?;
        self.target
            .as_mut()
            .ok_or_else(|| invalid("target consumed"))?
            .push(chunk.clone())?;
        out.push(chunk);
        Ok(())
    }
    pub fn finish(mut self) -> Result<StreamEnd<cs::ChatCompletionChunk>, TransformError> {
        if self.failed || !self.terminal {
            return Err(invalid("Responses EOF without terminal"));
        }
        let mut target = self
            .target
            .take()
            .ok_or_else(|| invalid("target consumed"))?;
        target.push_done()?;
        let actual = target.finish()?;
        if Some(&actual.value) != self.expected.as_ref() {
            return Err(invalid(
                "emitted Chat content/identity/metadata differs from native terminal response",
            ));
        }
        self.report.diagnostics.extend(actual.report.diagnostics);
        Ok(StreamEnd {
            chunks: Vec::new(),
            identities: self.flow,
            report: self.report,
        })
    }
}

fn item_id(item: &r::ResponseOutputItem) -> Option<&str> {
    match item {
        r::ResponseOutputItem::Message(v) => Some(&v.id),
        r::ResponseOutputItem::FunctionCall(v) => v.id.as_deref(),
        r::ResponseOutputItem::Reasoning(v) => Some(&v.id),
        _ => None,
    }
}
