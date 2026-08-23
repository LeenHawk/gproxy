use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::SseFrame;

use super::claude_to_openai::State;

impl State {
    pub(super) fn chat_text(
        &self,
        text: String,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        self.chat_chunk(
            openai::ChatDelta {
                content: Some(text),
                ..empty_delta(rest)
            },
            None,
            None,
        )
    }

    pub(super) fn chat_reasoning(
        &self,
        text: String,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        self.chat_chunk(
            openai::ChatDelta {
                reasoning_content: Some(text),
                ..empty_delta(rest)
            },
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
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        self.chat_chunk(
            openai::ChatDelta {
                tool_calls: Some(vec![openai::ChatToolCallDelta {
                    index,
                    id: Some(id),
                    type_: Some(openai::ChatToolCallType::Function),
                    function: Some(openai::FunctionCallDelta {
                        arguments: Some(arguments),
                        name: Some(name),
                        rest,
                    }),
                    custom: None,
                    rest: Default::default(),
                }]),
                ..empty_delta(Default::default())
            },
            None,
            None,
        )
    }

    pub(super) fn chat_tool_delta(
        &self,
        index: u32,
        arguments: String,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        self.chat_chunk(
            openai::ChatDelta {
                tool_calls: Some(vec![openai::ChatToolCallDelta {
                    index,
                    id: None,
                    type_: None,
                    function: Some(openai::FunctionCallDelta {
                        arguments: Some(arguments),
                        name: None,
                        rest,
                    }),
                    custom: None,
                    rest: Default::default(),
                }]),
                ..empty_delta(Default::default())
            },
            None,
            None,
        )
    }

    pub(super) fn chat_chunk(
        &self,
        delta: openai::ChatDelta,
        finish_reason: Option<openai::ChatFinishReason>,
        usage: Option<openai::CompletionUsage>,
    ) -> Result<Bytes, TransformError> {
        SseFrame::typed(
            None,
            &openai::ChatCompletionChunk {
                id: self.id.clone().expect("started message has an id"),
                choices: vec![openai::ChatChunkChoice {
                    index: 0,
                    delta,
                    finish_reason,
                    logprobs: None,
                    rest: Default::default(),
                }],
                created: None,
                model: self.model.clone().expect("started message has a model"),
                object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
                service_tier: None,
                system_fingerprint: None,
                usage,
                rest: Default::default(),
            },
        )
    }
}

pub(super) fn empty_delta(rest: openai::Rest) -> openai::ChatDelta {
    openai::ChatDelta {
        role: None,
        content: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: None,
        function_call: None,
        obfuscation: None,
        rest,
    }
}
