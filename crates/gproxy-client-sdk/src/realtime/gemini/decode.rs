use super::*;
use crate::realtime::session::State;
use RealtimeEventKind as K;
use base64::{Engine, engine::general_purpose::STANDARD};

fn observed(value: Option<i64>) -> Result<Option<u64>> {
    value.map(u64::try_from).transpose().map_err(Error::invalid)
}
fn start(state: &mut State, events: &mut Vec<K>) {
    if !state.responding {
        state.responding = true;
        events.push(K::ResponseStarted { response_id: None });
    }
}
fn call(state: &mut State, native: g::FunctionCall, events: &mut Vec<K>) -> Result<()> {
    state.had_tool_calls = true;
    let id = native.id.clone().unwrap_or_else(|| state.next_id("tool"));
    state.calls.insert(id.clone(), native.id);
    events.push(K::ToolCall {
        item: RealtimeItem::new(id.clone(), 0),
        call: ToolCall {
            id,
            name: native.name,
            arguments: json(native.args.unwrap_or_default())?.to_string(),
            provider_executed: false,
        },
    });
    Ok(())
}
/// Gemini messages can carry several fields together; never deserialize an
/// untagged event enum by trial order or discard the remaining fields.
pub(in crate::realtime) fn decode(state: &mut State, raw: Value) -> Result<Vec<K>> {
    if raw.get("error").is_some() {
        return Ok(vec![K::Error {
            error: Error::from_event(raw.to_string()),
            fatal: true,
        }]);
    }
    let known = [
        "setupComplete",
        "setup_complete",
        "serverContent",
        "server_content",
        "toolCall",
        "tool_call",
        "toolCallCancellation",
        "tool_call_cancellation",
        "goAway",
        "go_away",
        "sessionResumptionUpdate",
        "session_resumption_update",
        "usageMetadata",
        "usage_metadata",
        "voiceActivity",
        "voice_activity",
        "voiceActivityDetectionSignal",
        "voice_activity_detection_signal",
    ];
    if !known.iter().any(|key| raw.get(key).is_some()) {
        return Ok(vec![K::Unknown(raw)]);
    }
    let message: l::LiveServerMessage =
        serde_json::from_value(raw.clone()).map_err(Error::invalid)?;
    let mut events = Vec::new();
    if message.setup_complete.is_some() {
        if state.ready {
            events.push(K::Updated);
        } else {
            state.ready = true;
            events.push(K::Ready);
        }
    }
    if let Some(u) = message.usage_metadata {
        macro_rules! observe {
            ($target:expr, $value:expr) => {
                if let Some(value) = observed($value)? {
                    $target = Some(value);
                }
            };
        }
        observe!(state.usage.tokens.input_tokens, u.prompt_token_count);
        observe!(state.usage.tokens.output_tokens, u.response_token_count);
        observe!(state.usage.tokens.reasoning_tokens, u.thoughts_token_count);
        observe!(
            state.usage.tokens.cache_read_tokens,
            u.cached_content_token_count
        );
        observe!(state.usage.total_tokens, u.total_token_count);
    }
    if let Some(v) = message.voice_activity {
        match v.type_ {
            Some(l::VoiceActivityType::ActivityStart) => {
                input_boundary(state, &mut events);
                events.push(K::SpeechStarted {
                    item: state.input_item.clone(),
                    audio_ms: offset(v.audio_offset)?,
                });
            }
            Some(l::VoiceActivityType::ActivityEnd) => events.push(K::SpeechStopped {
                item: state.input_item.clone(),
                audio_ms: offset(v.audio_offset)?,
            }),
            _ => {}
        }
    }
    if let Some(v) = message.voice_activity_detection_signal {
        match v.vad_signal_type {
            Some(l::VadSignalType::VadSignalTypeSos) => {
                input_boundary(state, &mut events);
                events.push(K::SpeechStarted {
                    item: state.input_item.clone(),
                    audio_ms: None,
                });
            }
            Some(l::VadSignalType::VadSignalTypeEos) => events.push(K::SpeechStopped {
                item: state.input_item.clone(),
                audio_ms: None,
            }),
            _ => {}
        }
    }
    if let Some(content) = message.server_content {
        if let Some(preview) = content.interim_input_transcription
            && let Some(text) = preview.text
        {
            events.push(K::InputTranscriptPreview {
                item: state.input_item.clone(),
                text,
            });
        }
        if let Some(t) = content.input_transcription {
            if state.input_done {
                input_boundary(state, &mut events);
            }
            if state.input_text.is_empty() {
                events.push(K::InputCommitted {
                    item: state.input_item.clone(),
                });
            }
            if let Some(delta) = t.text {
                state.input_text.push_str(&delta);
                events.push(K::InputTranscriptDelta {
                    item: state.input_item.clone(),
                    delta,
                });
            }
            if t.finished == Some(true) {
                events.push(K::InputTranscriptDone {
                    item: state.input_item.clone(),
                    text: std::mem::take(&mut state.input_text),
                });
                state.input_done = true;
            }
        }
        if let Some(turn) = content.model_turn {
            start(state, &mut events);
            for part in turn.parts.into_iter().flatten() {
                if let Some(text) = part.text {
                    if part.thought == Some(true) {
                        events.push(K::Warning {
                            warning: Warning::new(
                                WarningKind::Unsupported,
                                "model_turn.thought",
                                "Live reasoning is available in native metadata only",
                            ),
                        });
                    } else {
                        state.output_text.push_str(&text);
                        events.push(K::TextDelta {
                            item: state.output_item.clone(),
                            delta: text,
                        });
                    }
                }
                if let Some(audio) = part.inline_data {
                    state.audio_item = Some(state.output_item.clone());
                    if !state.interrupted.contains(&state.output_item) {
                        let bytes =
                            STANDARD
                                .decode(audio.data.ok_or_else(|| {
                                    Error::invalid("Live audio has no inline data")
                                })?)
                                .map_err(Error::invalid)?;
                        let mime = audio
                            .mime_type
                            .as_deref()
                            .ok_or_else(|| Error::invalid("Live audio has no MIME type"))?;
                        if !matches!(
                            mime,
                            "audio/pcm;rate=24000" | "audio/L16;rate=24000" | "audio/pcm"
                        ) {
                            return Err(Error::invalid(
                                "Live returned an unsupported audio format",
                            ));
                        }
                        events.push(K::AudioDelta {
                            item: state.output_item.clone(),
                            bytes,
                            format: AudioFormat::pcm(24000, 1),
                        });
                    }
                }
                if let Some(c) = part.function_call {
                    call(state, c, &mut events)?;
                }
            }
        }
        if let Some(t) = content.output_transcription {
            start(state, &mut events);
            state.audio_item = Some(state.output_item.clone());
            if let Some(delta) = t.text {
                state.output_transcript.push_str(&delta);
                state.output_done = false;
                events.push(K::OutputTranscriptDelta {
                    item: state.output_item.clone(),
                    delta,
                });
            }
            if t.finished == Some(true) {
                events.push(K::OutputTranscriptDone {
                    item: state.output_item.clone(),
                    text: state.output_transcript.clone(),
                });
                state.output_done = true;
            }
        }
        if content.interrupted == Some(true) {
            state.turn_interrupted = true;
            state.interrupted.insert(state.output_item.clone());
            events.push(K::ClearLocalPlayback);
            events.push(K::Interrupted {
                item: state.output_item.clone(),
                played_ms: None,
            });
        }
        if content.turn_complete == Some(true) {
            if !state.output_transcript.is_empty() && !state.output_done {
                events.push(K::OutputTranscriptDone {
                    item: state.output_item.clone(),
                    text: std::mem::take(&mut state.output_transcript),
                });
            }
            state.output_transcript.clear();
            state.output_done = false;
            if !state.output_text.is_empty() {
                events.push(K::TextDone {
                    item: state.output_item.clone(),
                    text: std::mem::take(&mut state.output_text),
                });
            }
            let finish = if state.turn_interrupted {
                FinishReasonKind::Interrupted
            } else {
                match content.turn_complete_reason {
                    None | Some(l::TurnCompleteReason::TurnCompleteReasonUnspecified)
                        if state.had_tool_calls =>
                    {
                        FinishReasonKind::ToolCalls
                    }
                    None | Some(l::TurnCompleteReason::TurnCompleteReasonUnspecified) => {
                        FinishReasonKind::Stop
                    }
                    Some(l::TurnCompleteReason::MalformedFunctionCall) => FinishReasonKind::Error,
                    Some(l::TurnCompleteReason::NeedMoreInput) => FinishReasonKind::Other,
                    Some(_) => FinishReasonKind::ContentBlocked,
                }
            };
            events.push(K::ResponseDone {
                response_id: state.response_id.take(),
                finish_reason: FinishReason::new(
                    finish,
                    content
                        .turn_complete_reason
                        .map(|r| json(r).map(|v| v.as_str().map(str::to_owned)))
                        .transpose()?
                        .flatten(),
                ),
                usage: std::mem::take(&mut state.usage),
            });
            state.responding = false;
            state.had_tool_calls = false;
            state.turn_interrupted = false;
            state.output_item = RealtimeItem::new(state.next_id("output"), 0);
        }
    }
    if let Some(c) = message.tool_call {
        for native in c.function_calls.into_iter().flatten() {
            start(state, &mut events);
            call(state, native, &mut events)?;
        }
    }
    if let Some(c) = message.tool_call_cancellation {
        events.push(K::ToolCallCancelled {
            ids: c.ids.unwrap_or_default(),
        });
    }
    if let Some(away) = message.go_away {
        events.push(K::GoAway {
            time_left: away.time_left,
        });
    }
    if let Some(resumption) = message.session_resumption_update {
        events.push(K::Resumption {
            handle: resumption.new_handle,
            resumable: resumption.resumable,
        });
    }
    if events.is_empty() {
        events.push(K::Unknown(raw));
    }
    Ok(events)
}
fn input_boundary(state: &mut State, events: &mut Vec<K>) {
    if !state.input_text.is_empty() {
        events.push(K::InputTranscriptDone {
            item: state.input_item.clone(),
            text: std::mem::take(&mut state.input_text),
        });
        state.input_done = true;
    }
    if state.input_done {
        state.input_item = RealtimeItem::new(state.next_id("input"), 0);
        state.input_done = false;
    }
}
fn offset(value: Option<String>) -> Result<Option<u64>> {
    value
        .map(|value| {
            let seconds = value
                .strip_suffix('s')
                .ok_or_else(|| Error::invalid("invalid Live audio offset"))?
                .parse::<f64>()
                .map_err(Error::invalid)?;
            if !seconds.is_finite() || seconds < 0.0 || seconds * 1000.0 > u64::MAX as f64 {
                return Err(Error::invalid("invalid Live audio offset"));
            }
            Ok((seconds * 1000.0).round() as u64)
        })
        .transpose()
}
