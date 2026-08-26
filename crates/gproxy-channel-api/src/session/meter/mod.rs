mod dedupe;
mod model;
mod normalize;

use gproxy_protocol::openai::realtime::{
    CreateRealtimeCallRequest, KnownRealtimeServerEvent, RealtimeServerEvent, RealtimeSession,
};

use super::{SessionObservation, SessionUsage, SessionUsageKind};
use crate::WsFrame;

pub struct RealtimeMeter {
    primary_model: String,
    transcription_model: Option<String>,
    dedupe: dedupe::Dedupe,
    ready: bool,
}

impl RealtimeMeter {
    pub fn new(request: &[u8], primary_model: &str) -> Self {
        let transcription_model = serde_json::from_slice::<CreateRealtimeCallRequest>(request)
            .ok()
            .and_then(|request| model::transcription(&request.session));
        Self {
            primary_model: primary_model.into(),
            transcription_model,
            dedupe: dedupe::Dedupe::default(),
            ready: false,
        }
    }

    pub fn observe(&mut self, frame: &WsFrame) -> SessionObservation {
        let text = match frame {
            WsFrame::Text(text) => text,
            WsFrame::Binary(_) => return compromised("sideband sent a binary event", false),
            WsFrame::Close(_) => return SessionObservation::None,
        };
        let event: RealtimeServerEvent = match serde_json::from_str(text) {
            Ok(event) => event,
            Err(error) => return compromised(format!("event JSON: {error}"), false),
        };
        match event {
            RealtimeServerEvent::Known(event) => self.known(*event),
            RealtimeServerEvent::Unknown(event) => match event.type_.as_deref() {
                Some("response.done")
                | Some("conversation.item.input_audio_transcription.completed") => {
                    compromised("usage event has an invalid payload", false)
                }
                Some("session.created" | "session.updated") => {
                    compromised("session state event has an invalid payload", true)
                }
                _ => SessionObservation::None,
            },
        }
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn primary_model(&self) -> &str {
        &self.primary_model
    }

    pub fn require_resync(&mut self) {
        self.ready = false;
    }

    fn known(&mut self, event: KnownRealtimeServerEvent) -> SessionObservation {
        match event {
            KnownRealtimeServerEvent::SessionCreated(event)
            | KnownRealtimeServerEvent::SessionUpdated(event) => {
                self.update_session(&event.session);
                SessionObservation::None
            }
            KnownRealtimeServerEvent::ResponseDone(event) => {
                if event
                    .response
                    .id
                    .as_ref()
                    .is_some_and(|id| self.dedupe.response_seen(id))
                {
                    return SessionObservation::None;
                }
                let Some(usage) = event.response.usage.as_ref() else {
                    return compromised("response.done omitted usage", false);
                };
                match normalize::realtime(usage) {
                    Ok(usage) => {
                        if let Some(id) = event.response.id.as_ref() {
                            self.dedupe.record_response(id);
                        }
                        SessionObservation::Usage(SessionUsage {
                            kind: SessionUsageKind::Primary,
                            model: self.primary_model.clone(),
                            usage,
                        })
                    }
                    Err(error) => compromised(error.to_string(), false),
                }
            }
            KnownRealtimeServerEvent::InputAudioTranscriptionCompleted(event) => {
                let identity = (event.item_id.clone(), event.content_index);
                if self.dedupe.transcription_seen(&identity) {
                    return SessionObservation::None;
                }
                let Some(usage) = event.usage.as_ref() else {
                    return compromised("transcription completed event omitted usage", false);
                };
                let Some(model) = self.transcription_model.clone() else {
                    return compromised("transcription usage has no server-reported model", true);
                };
                match normalize::audio(usage) {
                    Ok(usage) => {
                        self.dedupe.record_transcription(identity);
                        SessionObservation::Usage(SessionUsage {
                            kind: SessionUsageKind::Transcription,
                            model,
                            usage,
                        })
                    }
                    Err(error) => compromised(error.to_string(), false),
                }
            }
            KnownRealtimeServerEvent::Error(_)
            | KnownRealtimeServerEvent::ConversationItemAdded(_)
            | KnownRealtimeServerEvent::ConversationItemCreated(_)
            | KnownRealtimeServerEvent::ConversationItemDone(_)
            | KnownRealtimeServerEvent::ConversationItemRetrieved(_)
            | KnownRealtimeServerEvent::ConversationItemTruncated(_)
            | KnownRealtimeServerEvent::ConversationItemDeleted(_)
            | KnownRealtimeServerEvent::InputAudioTranscriptionDelta(_)
            | KnownRealtimeServerEvent::InputAudioTranscriptionFailed(_)
            | KnownRealtimeServerEvent::InputAudioTranscriptionSegment(_)
            | KnownRealtimeServerEvent::InputAudioBufferCommitted(_)
            | KnownRealtimeServerEvent::InputAudioBufferCleared(_)
            | KnownRealtimeServerEvent::InputAudioBufferSpeechStarted(_)
            | KnownRealtimeServerEvent::InputAudioBufferSpeechStopped(_)
            | KnownRealtimeServerEvent::InputAudioBufferTimeoutTriggered(_)
            | KnownRealtimeServerEvent::OutputAudioBufferStarted(_)
            | KnownRealtimeServerEvent::OutputAudioBufferStopped(_)
            | KnownRealtimeServerEvent::OutputAudioBufferCleared(_)
            | KnownRealtimeServerEvent::RateLimitsUpdated(_)
            | KnownRealtimeServerEvent::ResponseCreated(_)
            | KnownRealtimeServerEvent::ResponseOutputItemAdded(_)
            | KnownRealtimeServerEvent::ResponseOutputItemDone(_)
            | KnownRealtimeServerEvent::ResponseContentPartAdded(_)
            | KnownRealtimeServerEvent::ResponseContentPartDone(_)
            | KnownRealtimeServerEvent::OutputTextDelta(_)
            | KnownRealtimeServerEvent::OutputTextDone(_)
            | KnownRealtimeServerEvent::OutputAudioTranscriptDelta(_)
            | KnownRealtimeServerEvent::OutputAudioTranscriptDone(_)
            | KnownRealtimeServerEvent::OutputAudioDelta(_)
            | KnownRealtimeServerEvent::OutputAudioDone(_)
            | KnownRealtimeServerEvent::FunctionCallArgumentsDelta(_)
            | KnownRealtimeServerEvent::FunctionCallArgumentsDone(_)
            | KnownRealtimeServerEvent::McpCallArgumentsDelta(_)
            | KnownRealtimeServerEvent::McpCallArgumentsDone(_)
            | KnownRealtimeServerEvent::McpCallInProgress(_)
            | KnownRealtimeServerEvent::McpCallCompleted(_)
            | KnownRealtimeServerEvent::McpCallFailed(_)
            | KnownRealtimeServerEvent::McpListToolsInProgress(_)
            | KnownRealtimeServerEvent::McpListToolsCompleted(_)
            | KnownRealtimeServerEvent::McpListToolsFailed(_) => SessionObservation::None,
        }
    }

    fn update_session(&mut self, session: &RealtimeSession) {
        if let Some(model) = session.model.as_ref().and_then(model::name) {
            self.primary_model = model;
        }
        self.transcription_model = model::transcription(session);
        self.ready = true;
    }
}

fn compromised(message: impl Into<String>, resync: bool) -> SessionObservation {
    SessionObservation::Compromised {
        reason: format!("Realtime sideband: {}", message.into()),
        resync,
    }
}

#[cfg(test)]
mod tests;
