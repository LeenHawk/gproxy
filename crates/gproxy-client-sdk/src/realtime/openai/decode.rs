use super::*;
use crate::realtime::session::State;
use base64::{Engine, engine::general_purpose::STANDARD};
use o::RealtimeServerEvent as E;

fn count(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(Error::invalid)
}
fn optional(value: Option<Option<i64>>) -> Result<Option<u64>> {
    value.flatten().map(count).transpose()
}
pub(in crate::realtime) fn decode(state: &mut State, raw: Value) -> Result<Vec<RealtimeEventKind>> {
    use RealtimeEventKind as K;
    let Some(event) = dispatch::dispatch(raw.clone())? else {
        return Ok(vec![K::Unknown(raw)]);
    };
    let kind = match event {
        E::SessionUpdatedEvent(e) => {
            if let o::SessionUpdatedEventSession::RealtimeSessionCreateRequest(session) = e.session
                && let Some(format) = session
                    .audio
                    .and_then(|a| a)
                    .and_then(|a| a.output)
                    .and_then(|o| o)
                    .and_then(|o| o.format)
                    .and_then(|f| f)
            {
                state.output_format = format_from_wire(&format);
            }
            if state.ready {
                K::Updated
            } else {
                state.ready = true;
                K::Ready
            }
        }
        E::InputAudioBufferSpeechStartedEvent(e) => K::SpeechStarted {
            item: RealtimeItem::new(e.item_id, 0),
            audio_ms: Some(count(e.audio_start_ms)?),
        },
        E::InputAudioBufferSpeechStoppedEvent(e) => K::SpeechStopped {
            item: RealtimeItem::new(e.item_id, 0),
            audio_ms: Some(count(e.audio_end_ms)?),
        },
        E::InputAudioBufferCommittedEvent(e) => K::InputCommitted {
            item: RealtimeItem::new(e.item_id, 0),
        },
        E::InputAudioBufferClearedEvent(_) => K::InputCleared,
        E::ConversationItemInputAudioTranscriptionDeltaEvent(e) => match e.delta.flatten() {
            Some(delta) => K::InputTranscriptDelta {
                item: RealtimeItem::new(e.item_id, e.content_index.flatten().unwrap_or(0)),
                delta,
            },
            None => K::Unknown(raw),
        },
        E::ConversationItemInputAudioTranscriptionCompletedEvent(e) => K::InputTranscriptDone {
            item: RealtimeItem::new(e.item_id, e.content_index),
            text: e.transcript,
        },
        E::ConversationItemInputAudioTranscriptionFailedEvent(e) => K::InputTranscriptionFailed {
            item: RealtimeItem::new(e.item_id, e.content_index),
            error: Error::from_event(json(e.error)?.to_string()),
        },
        E::ResponseCreatedEvent(e) => {
            state.response_id = e.response.id.flatten();
            state.responding = true;
            state.response_audio_item = None;
            state.had_tool_calls = false;
            K::ResponseStarted {
                response_id: state.response_id.clone(),
            }
        }
        E::ResponseAudioDeltaEvent(e) => {
            let item = RealtimeItem::new(e.item_id, e.content_index);
            state.audio_item = Some(item.clone());
            state.response_audio_item = Some(item.clone());
            if state.interrupted.contains(&item) {
                return Ok(Vec::new());
            }
            K::AudioDelta {
                item,
                bytes: STANDARD.decode(e.delta).map_err(Error::invalid)?,
                format: state.output_format.clone(),
            }
        }
        E::ResponseAudioTranscriptDeltaEvent(e) => {
            let item = RealtimeItem::new(e.item_id, e.content_index);
            state.audio_item = Some(item.clone());
            state.response_audio_item = Some(item.clone());
            K::OutputTranscriptDelta {
                item,
                delta: e.delta,
            }
        }
        E::ResponseAudioTranscriptDoneEvent(e) => K::OutputTranscriptDone {
            item: RealtimeItem::new(e.item_id, e.content_index),
            text: e.transcript,
        },
        E::ResponseTextDeltaEvent(e) => K::TextDelta {
            item: RealtimeItem::new(e.item_id, e.content_index),
            delta: e.delta,
        },
        E::ResponseTextDoneEvent(e) => K::TextDone {
            item: RealtimeItem::new(e.item_id, e.content_index),
            text: e.text,
        },
        E::ResponseFunctionCallArgumentsDoneEvent(e) => {
            state.had_tool_calls = true;
            K::ToolCall {
                item: RealtimeItem::new(e.item_id, 0),
                call: ToolCall {
                    id: e.call_id,
                    name: e.name,
                    arguments: e.arguments,
                    provider_executed: false,
                },
            }
        }
        E::ConversationItemTruncatedEvent(e) => {
            let item = RealtimeItem::new(e.item_id, e.content_index);
            state.interrupted.insert(item.clone());
            return Ok(vec![
                K::ClearLocalPlayback,
                K::Interrupted {
                    item,
                    played_ms: Some(count(e.audio_end_ms)?),
                },
            ]);
        }
        E::ResponseDoneEvent(e) => {
            state.responding = false;
            let response_id = e.response.id.flatten();
            let interrupted = state
                .response_audio_item
                .as_ref()
                .is_some_and(|item| state.interrupted.contains(item));
            let reason = e
                .response
                .status_details
                .as_ref()
                .and_then(|s| s.as_ref())
                .and_then(|s| s.reason.as_ref())
                .and_then(|r| *r);
            let finish = if interrupted {
                FinishReasonKind::Interrupted
            } else {
                match (e.response.status.as_ref().and_then(|s| *s), reason) {
                    (_, Some(o::RealtimeResponseStatusReason::ContentFilter)) => {
                        FinishReasonKind::ContentBlocked
                    }
                    (_, Some(o::RealtimeResponseStatusReason::MaxOutputTokens)) => {
                        FinishReasonKind::Length
                    }
                    (Some(o::RealtimeResponseStatusValue::Cancelled), _) => {
                        FinishReasonKind::Interrupted
                    }
                    (Some(o::RealtimeResponseStatusValue::Failed), _) => FinishReasonKind::Error,
                    (Some(o::RealtimeResponseStatusValue::Completed), _)
                        if state.had_tool_calls =>
                    {
                        FinishReasonKind::ToolCalls
                    }
                    (Some(o::RealtimeResponseStatusValue::Completed), _) => FinishReasonKind::Stop,
                    _ => FinishReasonKind::Other,
                }
            };
            let native_reason = e
                .response
                .status
                .as_ref()
                .and_then(|s| *s)
                .map(|s| {
                    json(s)
                        .and_then(|v| serde_json::from_value::<String>(v).map_err(Error::invalid))
                })
                .transpose()?;
            let mut usage = RealtimeUsage::default();
            if let Some(Some(u)) = e.response.usage {
                usage.tokens.input_tokens = optional(u.input_tokens)?;
                usage.tokens.output_tokens = optional(u.output_tokens)?;
                usage.total_tokens = optional(u.total_tokens)?;
                if let Some(Some(details)) = u.input_token_details {
                    usage.tokens.cache_read_tokens = optional(details.cached_tokens)?;
                }
            }
            K::ResponseDone {
                response_id,
                finish_reason: FinishReason::new(finish, native_reason),
                usage,
            }
        }
        E::RealtimeErrorEvent(_) => K::Error {
            error: Error::from_event(raw.to_string()),
            fatal: false,
        },
        E::RateLimitsUpdatedEvent(e) => K::RateLimits {
            limits: json(e.rate_limits)?,
        },
        _ => K::Unknown(raw),
    };
    Ok(vec![kind])
}
