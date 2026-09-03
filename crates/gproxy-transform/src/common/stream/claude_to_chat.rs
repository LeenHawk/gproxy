use gproxy_protocol::openai;

use crate::TransformError;

use super::claude_to_openai::{OutputEvent, State};

impl State {
    pub(super) fn chat_text(&self, text: String) -> Result<OutputEvent, TransformError> {
        self.chat_chunk(
            crate::wire!(openai::ChatDelta {
                content: Some(text),
                ..empty_delta()
            }),
            None,
            None,
        )
    }

    pub(super) fn chat_reasoning(&self, text: String) -> Result<OutputEvent, TransformError> {
        self.chat_chunk(
            crate::wire!(openai::ChatDelta {
                reasoning_content: Some(text),
                ..empty_delta()
            }),
            None,
            None,
        )
    }

    pub(super) fn chat_tool_start(
        &self,
        index: u32,
        id: String,
        name: String,
        arguments: String,
    ) -> Result<OutputEvent, TransformError> {
        self.chat_chunk(
            crate::wire!(openai::ChatDelta {
                tool_calls: Some(vec![crate::wire!(openai::ChatToolCallDelta {
                    index,
                    id: Some(id),
                    type_: Some(openai::ChatToolCallType::Function),
                    function: Some(openai::FunctionCallDelta {
                        arguments: Some(arguments),
                        name: Some(name),
                        rest: Default::default(),
                    }),
                    custom: None,
                    rest: Default::default(),
                })]),
                ..empty_delta()
            }),
            None,
            None,
        )
    }

    pub(super) fn chat_tool_delta(
        &self,
        index: u32,
        arguments: String,
    ) -> Result<OutputEvent, TransformError> {
        self.chat_chunk(
            crate::wire!(openai::ChatDelta {
                tool_calls: Some(vec![crate::wire!(openai::ChatToolCallDelta {
                    index,
                    id: None,
                    type_: None,
                    function: Some(openai::FunctionCallDelta {
                        arguments: Some(arguments),
                        name: None,
                        rest: Default::default(),
                    }),
                    custom: None,
                    rest: Default::default(),
                })]),
                ..empty_delta()
            }),
            None,
            None,
        )
    }

    pub(super) fn chat_chunk(
        &self,
        delta: openai::ChatDelta,
        finish_reason: Option<openai::ChatFinishReason>,
        usage: Option<openai::CompletionUsage>,
    ) -> Result<OutputEvent, TransformError> {
        Ok(OutputEvent::Chat(crate::wire!(
            openai::ChatCompletionChunk {
                id: self.id.clone().expect("started message has an id"),
                choices: vec![crate::wire!(openai::ChatChunkChoice {
                    index: 0,
                    delta,
                    finish_reason,
                    logprobs: None,
                    rest: Default::default(),
                })],
                created: None,
                model: self.model.clone().expect("started message has a model"),
                object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
                service_tier: None,
                system_fingerprint: None,
                usage,
                rest: Default::default(),
            }
        )))
    }
}

pub(super) fn empty_delta() -> openai::ChatDelta {
    crate::wire!(openai::ChatDelta {
        role: None,
        content: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: None,
        function_call: None,
        obfuscation: None,
        rest: Default::default(),
    })
}
