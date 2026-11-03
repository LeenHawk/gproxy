use super::*;
use crate::stream::{BlockId, Event};
use gproxy_protocol::openai::chat::stream as s;
impl StreamState {
    pub(super) fn apply_choice(
        &mut self,
        choice: s::StreamChoice,
        out: &mut Vec<Event>,
    ) -> Result<()> {
        let delta = choice.delta;
        let extensions: StreamExtensions =
            serde_json::from_value(Value::Object(delta.rest)).map_err(Error::invalid)?;
        if let Some(Some(text)) = delta.content {
            self.message
                .content
                .get_or_insert_with(String::new)
                .push_str(&text);
            out.push(Event::TextDelta {
                id: BlockId { item: 1, part: 0 },
                delta: text,
            });
        }
        if let Some(Some(text)) = delta.refusal {
            self.message
                .refusal
                .get_or_insert_with(String::new)
                .push_str(&text);
            out.push(Event::TextDelta {
                id: BlockId { item: 2, part: 0 },
                delta: text,
            });
        }
        if let Some(Some(text)) = &delta.reasoning_content {
            out.push(Event::ReasoningDelta {
                id: BlockId { item: 0, part: 0 },
                delta: text.clone(),
            });
        }
        append_nullable(&mut self.message.reasoning_content, delta.reasoning_content);
        if let Some(Some(text)) = &delta.reasoning {
            out.push(Event::ReasoningDelta {
                id: BlockId { item: 0, part: 0 },
                delta: text.clone(),
            });
        }
        append_nullable(&mut self.message.reasoning, delta.reasoning);
        if let Some(Some(details)) = delta.reasoning_details {
            self.reasoning_details_delta(details, out)?;
        }
        if let Some(annotations) = extensions.annotations {
            for annotation in &annotations {
                out.push(Event::Citation {
                    id: BlockId { item: 1, part: 0 },
                    citation: citations::parse_chat(annotation.clone())?,
                });
            }
            self.message
                .annotations
                .get_or_insert_with(Vec::new)
                .extend(annotations);
        }
        if let Some(audio) = extensions.audio {
            self.audio_delta(audio, out)?;
        }
        if let Some(Some(function)) = delta.function_call {
            self.function_delta(function, out)?;
        }
        if let Some(Some(calls)) = delta.tool_calls {
            self.tool_calls_delta(calls, out)?;
        }
        if let Some(Some(reason)) = choice.finish_reason {
            self.reason = Some(reason);
        }
        self.message.rest.extend(extensions.rest);

        Ok(())
    }
    fn reasoning_details_delta(
        &mut self,
        details: Vec<gproxy_protocol::openai::chat::reasoning::ReasoningDetail>,
        out: &mut Vec<Event>,
    ) -> Result<()> {
        let target = self
            .message
            .reasoning_details
            .get_or_insert_with(|| Some(Vec::new()))
            .get_or_insert_with(Vec::new);
        let plain_reasoning = gproxy_protocol::openai::chat::reasoning_text(
            &self.message.reasoning_content,
            &self.message.reasoning,
        )
        .is_some_and(|s| !s.is_empty());
        for detail in details {
            if !plain_reasoning {
                use gproxy_protocol::openai::chat::reasoning::ReasoningDetailKind as K;
                let text = match detail.type_ {
                    K::Text => detail.text.as_ref().and_then(Option::as_ref),
                    K::Summary => detail.summary.as_ref(),
                    K::Encrypted => None,
                };
                if let Some(text) = text {
                    out.push(Event::ReasoningDelta {
                        id: BlockId { item: 0, part: 0 },
                        delta: text.clone(),
                    });
                }
            }
            if let Some(index) = detail.index
                && let Some(current) = target
                    .iter_mut()
                    .find(|current| current.index == Some(index) && current.type_ == detail.type_)
            {
                append_nullable(&mut current.text, detail.text);
                append_nullable(&mut current.signature, detail.signature);
                if let Some(text) = detail.summary {
                    current
                        .summary
                        .get_or_insert_with(String::new)
                        .push_str(&text);
                }
                if let Some(data) = detail.data {
                    current.data.get_or_insert_with(String::new).push_str(&data);
                }
                if detail.id.is_some() {
                    current.id = detail.id;
                }
                if detail.format.is_some() {
                    current.format = detail.format;
                }
                current.rest.extend(detail.rest);
            } else {
                target.push(detail);
            }
        }

        Ok(())
    }
    fn audio_delta(&mut self, audio: AudioDelta, out: &mut Vec<Event>) -> Result<()> {
        self.audio_seen = true;
        if let Some(id) = audio.id {
            self.audio_id = Some(id);
        }
        if let Some(expiry) = audio.expires_at {
            self.expires = Some(expiry);
        }
        if let Some(data) = audio.data {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(Error::invalid)?;
            self.audio.extend_from_slice(&bytes);
            out.push(Event::AudioDelta {
                id: BlockId { item: 3, part: 0 },
                bytes,
                format: self
                    .format
                    .clone()
                    .ok_or_else(|| Error::invalid("audio stream requires its requested format"))?,
            });
        }
        if let Some(text) = audio.transcript {
            self.transcript.push_str(&text);
            out.push(Event::TranscriptDelta {
                id: BlockId { item: 3, part: 0 },
                delta: text,
            });
        }
        self.audio_rest.extend(audio.rest);

        Ok(())
    }
    fn function_delta(
        &mut self,
        function: s::DeltaFunctionCall,
        out: &mut Vec<Event>,
    ) -> Result<()> {
        let first = self.message.function_call.is_none();
        let value = self
            .message
            .function_call
            .get_or_insert_with(|| c::FunctionCall::builder(String::new(), String::new()).build());
        if let Some(Some(name)) = function.name {
            value.name.push_str(&name);
        }
        let id = BlockId { item: 4, part: 0 };
        if first {
            out.push(Event::ToolCallStart {
                id,
                call_id: "legacy_function".into(),
                name: value.name.clone(),
                provider_executed: false,
            });
        }
        if let Some(Some(arguments)) = function.arguments {
            value.arguments.push_str(&arguments);
            out.push(Event::ToolCallArgsDelta {
                id,
                delta: arguments,
            });
        }
        value.rest.extend(function.rest);

        Ok(())
    }
    fn tool_calls_delta(
        &mut self,
        calls: Vec<s::DeltaToolCall>,
        out: &mut Vec<Event>,
    ) -> Result<()> {
        for delta in calls {
            let index: u32 = delta.index.try_into().map_err(Error::invalid)?;
            let id = BlockId {
                item: 4 + index,
                part: 0,
            };
            let value = self.calls.entry(index).or_insert_with(|| {
                c::ChatToolCall::builder(
                    String::new(),
                    c::FunctionCall::builder(String::new(), String::new()).build(),
                    c::ChatToolCallType::Function,
                )
                .build()
            });
            // id and name arrive whole on the call's first delta; OpenAI-compatible
            // upstreams may repeat them on later deltas, so they are set once.
            if let Some(Some(call_id)) = delta.id
                && value.id.is_empty()
            {
                value.id = call_id;
            }
            if let Some(Some(function)) = delta.function {
                let named = !value.function.name.is_empty();
                if let Some(Some(name)) = function.name
                    && !named
                {
                    value.function.name = name;
                }
                if !named && !value.function.name.is_empty() {
                    out.push(Event::ToolCallStart {
                        id,
                        call_id: value.id.clone(),
                        name: value.function.name.clone(),
                        provider_executed: false,
                    });
                }
                if let Some(Some(arguments)) = function.arguments {
                    value.function.arguments.push_str(&arguments);
                    out.push(Event::ToolCallArgsDelta {
                        id,
                        delta: arguments,
                    });
                }
                value.function.rest.extend(function.rest);
            }
            value.type_ = c::ChatToolCallType::Function;
            value.rest.extend(delta.rest);
        }

        Ok(())
    }
}
