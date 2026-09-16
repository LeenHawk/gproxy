use crate::{
    transform::{
        TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::openai::chat::{content as c, response as r, stream as s},
};
use std::collections::BTreeMap;

#[derive(Default)]
struct FunctionAccum {
    name: String,
    args: String,
    id: Option<String>,
}

impl FunctionAccum {
    fn push(&mut self, function: s::DeltaFunctionCall) {
        if let Some(Some(v)) = function.name {
            self.name.push_str(&v);
        }
        if let Some(Some(v)) = function.arguments {
            self.args.push_str(&v);
        }
    }
    fn finish(self) -> Result<c::FunctionCall, TransformError> {
        if self.name.is_empty() {
            return Err(TransformError::missing_metadata("tool.function.name"));
        }
        Ok(c::FunctionCall::builder(self.args, self.name).build())
    }
}

#[derive(Default)]
pub(super) struct ChoiceAccum {
    role: Option<s::DeltaRole>,
    content: Option<String>,
    refusal: Option<String>,
    legacy: Option<FunctionAccum>,
    tools: BTreeMap<i64, FunctionAccum>,
    finish: Option<r::FinishReason>,
    logs: Option<r::Logprobs>,
}

impl ChoiceAccum {
    pub(super) fn tool_count(&self) -> usize {
        self.tools.len() + usize::from(self.legacy.is_some())
    }
    pub(super) fn is_finished(&self) -> bool {
        self.finish.is_some()
    }
    pub(super) fn push(&mut self, choice: s::StreamChoice) -> Result<(), TransformError> {
        if self.finish.is_some() {
            return Err(TransformError::invalid_result(
                "choice",
                "delta or duplicate finish after choice terminal",
            ));
        }
        let d = choice.delta;
        if let Some(Some(role)) = d.role {
            if role != s::DeltaRole::Assistant {
                return Err(TransformError::invalid_result(
                    "delta.role",
                    "Chat response role must be assistant",
                ));
            }
            self.role = Some(role);
        }
        if let Some(Some(text)) = d.content {
            self.content.get_or_insert_with(String::new).push_str(&text);
        }
        if let Some(Some(text)) = d.refusal {
            self.refusal.get_or_insert_with(String::new).push_str(&text);
        }
        if let Some(Some(call)) = d.function_call {
            self.legacy.get_or_insert_with(Default::default).push(call);
        }
        if let Some(Some(calls)) = d.tool_calls {
            for call in calls {
                if call.index < 0 {
                    return Err(TransformError::invalid_result(
                        "tool.index",
                        "negative tool index",
                    ));
                }
                let state = self.tools.entry(call.index).or_default();
                if let Some(Some(id)) = call.id {
                    if id.is_empty() || state.id.as_ref().is_some_and(|v| v != &id) {
                        return Err(TransformError::invalid_result(
                            "tool.id",
                            "empty or conflicting tool ID",
                        ));
                    }
                    state.id = Some(id);
                }
                if let Some(Some(function)) = call.function {
                    state.push(function);
                }
            }
        }
        if self.legacy.is_some() && !self.tools.is_empty() {
            return Err(TransformError::invalid_result(
                "tool_calls",
                "legacy and modern tool deltas conflict",
            ));
        }
        if let Some(Some(logs)) = choice.logprobs {
            super::logs::append(&mut self.logs, logs)?;
        }
        if let Some(Some(finish)) = choice.finish_reason {
            self.finish = Some(finish);
        }
        Ok(())
    }
    pub(super) fn finish(
        self,
        index: i64,
        ordinal: &mut u64,
        flow: &mut IdentityFlow,
        policy: &TargetIdPolicy,
    ) -> Result<r::Choice, TransformError> {
        let finish = self
            .finish
            .ok_or_else(|| TransformError::missing_metadata("finish_reason"))?;
        if (finish == r::FinishReason::ToolCalls && self.tools.is_empty())
            || (finish == r::FinishReason::FunctionCall && self.legacy.is_none())
            || (finish == r::FinishReason::Stop
                && (!self.tools.is_empty() || self.legacy.is_some()))
        {
            return Err(TransformError::invalid_result(
                "finish_reason",
                "tool payload conflicts with finish reason",
            ));
        }
        let mut tools = Vec::new();
        for (expected, (tool_index, state)) in self.tools.into_iter().enumerate() {
            if tool_index != expected as i64 {
                return Err(TransformError::invalid_result(
                    "tool.index",
                    "non-contiguous tool indexes",
                ));
            }
            let id = flow
                .resolve_or_allocate(
                    IdentityRole::ToolCall,
                    SourceIdentity::new(crate::Dialect::OpenAiChat, state.id.clone(), *ordinal),
                    policy,
                )
                .map_err(|e| TransformError::invalid_result("identity", e.to_string()))?
                .emitted_id;
            *ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| super::limit("tool_calls"))?;
            tools.push(c::MessageToolCall::Function(
                c::ChatToolCall::builder(id, state.finish()?, c::ChatToolCallType::Function)
                    .build(),
            ));
        }
        let mut message =
            r::ResponseMessage::builder(self.content, self.refusal, r::ResponseRole::Assistant)
                .build();
        message.function_call = self
            .legacy
            .map(FunctionAccum::finish)
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        if !tools.is_empty() {
            message.tool_calls = Some(tools);
        }
        Ok(r::Choice::builder(finish, index, self.logs, message).build())
    }
}
