use super::session::{Driver, State};
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, lock::Mutex};
use protocol::{
    connection::{WsClose, WsFrame},
    gemini::live as l,
    openai::realtime as o,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct RealtimeSender {
    pub(crate) state: Arc<Mutex<State>>,
    pub(crate) queue: Arc<queue::Queue>,
}
impl RealtimeSender {
    fn local(&self, state: &State, kind: RealtimeEventKind) {
        self.queue
            .push(RealtimeEvent::local(state.vendor.clone(), kind));
    }
    pub async fn append_audio(&self, bytes: &[u8]) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        let audio = STANDARD.encode(bytes);
        match state.dialect {
            Driver::Openai => {
                state
                    .send(
                        o::InputAudioBufferAppendEvent::builder(
                            audio,
                            o::InputAudioBufferAppendEventType::InputAudioBufferAppend,
                        )
                        .build(),
                    )
                    .await
            }
            Driver::Gemini => {
                if state.manual_input && !state.input_active {
                    state
                        .send(gemini::realtime(
                            l::BidiGenerateContentRealtimeInput::builder()
                                .activity_start(l::LiveActivityStart::builder().build())
                                .build(),
                        ))
                        .await?;
                    state.input_active = true;
                }
                state
                    .send(gemini::realtime(
                        l::BidiGenerateContentRealtimeInput::builder()
                            .audio(
                                l::LiveBlob::builder()
                                    .mime_type("audio/pcm;rate=16000")
                                    .data(audio)
                                    .build(),
                            )
                            .build(),
                    ))
                    .await
            }
        }
    }
    /// Commit input without an extra OpenAI response.create. Manual OpenAI
    /// callers explicitly call create_response after committing.
    pub async fn commit(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        match state.dialect {
            Driver::Openai => {
                state
                    .send(
                        o::InputAudioBufferCommitEvent::builder(
                            o::InputAudioBufferCommitEventType::InputAudioBufferCommit,
                        )
                        .build(),
                    )
                    .await
            }
            Driver::Gemini => {
                let mut input = l::BidiGenerateContentRealtimeInput::builder().build();
                if state.manual_input {
                    input.activity_end = Some(l::LiveActivityEnd::builder().build());
                } else {
                    input.audio_stream_end = Some(true);
                }
                state.send(gemini::realtime(input)).await?;
                state.input_active = false;
                Ok(())
            }
        }
    }
    pub async fn clear(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        match state.dialect {
            Driver::Openai => {
                state
                    .send(
                        o::InputAudioBufferClearEvent::builder(
                            o::InputAudioBufferClearEventType::InputAudioBufferClear,
                        )
                        .build(),
                    )
                    .await
            }
            Driver::Gemini => Err(Error::unsupported(
                "Live has no input-buffer clear operation",
            )),
        }
    }
    pub async fn send_text(&self, text: &str) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        let item = RealtimeItem::new(state.next_id("text"), 0);
        match state.dialect {
            Driver::Openai => {
                state
                    .send(openai::text(Some(&item.id), &Role::User, text)?)
                    .await?;
                if state.auto_response {
                    create(&mut state).await?;
                }
            }
            Driver::Gemini => {
                let auto = state.auto_response;
                state.send(gemini::text(text, auto)).await?;
            }
        }
        self.local(
            &state,
            RealtimeEventKind::InputText {
                item,
                text: text.to_owned(),
            },
        );
        Ok(())
    }
    pub async fn send_tool_result(&self, result: &ToolResult) -> Result<()> {
        let auto = self.state.lock().await.auto_response;
        self.send_tool_result_with_response(result, auto).await
    }
    pub async fn send_tool_result_with_response(
        &self,
        result: &ToolResult,
        request_response: bool,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        match state.dialect {
            Driver::Openai => {
                state.send(openai::tool_result(result)?).await?;
                if request_response {
                    create(&mut state).await?;
                }
            }
            Driver::Gemini => {
                let mut native = gemini::tool_result(result)?;
                if let Some(native_id) = state.calls.get(&result.id)
                    && let Some(response) = native
                        .tool_response
                        .as_mut()
                        .and_then(|t| t.function_responses.as_mut())
                        .and_then(|r| r.first_mut())
                {
                    response.id = native_id.clone();
                }
                state.send(native).await?;
                if !request_response {
                    self.local(&state, RealtimeEventKind::Warning { warning: Warning::new(WarningKind::Unsupported, "auto_response", "Live resumes generation on tool responses; response creation cannot be deferred") });
                }
            }
        }
        self.local(
            &state,
            RealtimeEventKind::ToolResultSent {
                result: result.clone(),
            },
        );
        Ok(())
    }
    pub async fn create_response(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        create(&mut state).await
    }
    pub async fn cancel(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        cancel(&mut state).await
    }
    pub async fn interrupt(&self, played_ms: u64) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        let item = state
            .audio_item
            .clone()
            .ok_or_else(|| Error::invalid("there is no observed output audio item to interrupt"))?;
        match state.dialect {
            Driver::Openai => {
                let ms = i64::try_from(played_ms).map_err(Error::invalid)?;
                if state.responding && state.response_audio_item.as_ref() == Some(&item) {
                    cancel(&mut state).await?;
                }
                state
                    .send(
                        o::ConversationItemTruncateEvent::builder(
                            ms,
                            item.content_index,
                            item.id.clone(),
                            o::ConversationItemTruncateEventType::ConversationItemTruncate,
                        )
                        .build(),
                    )
                    .await?;
            }
            Driver::Gemini => {
                if item == state.output_item {
                    state.turn_interrupted = true;
                }
            }
        }
        state.interrupted.insert(item.clone());
        self.local(&state, RealtimeEventKind::ClearLocalPlayback);
        self.local(
            &state,
            RealtimeEventKind::Interrupted {
                item,
                played_ms: Some(played_ms),
            },
        );
        Ok(())
    }
    pub async fn update_session(&self, config: &RealtimeConfig) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        match state.dialect {
            Driver::Gemini => Err(Error::unsupported(
                "Gemini Live setup cannot be changed after connecting",
            )),
            Driver::Openai => {
                if config.model != state.model {
                    return Err(Error::invalid(
                        "session updates cannot change the routed model",
                    ));
                }
                if config.history.is_some() {
                    return Err(Error::invalid(
                        "history can only seed a new Realtime session",
                    ));
                }
                let rendered = openai::render(config)?;
                state.send(rendered.request).await?;
                state.auto_response = config.auto_response;
                for warning in rendered.warnings {
                    self.local(&state, RealtimeEventKind::Warning { warning });
                }
                Ok(())
            }
        }
    }
    pub async fn send_native(&self, message: NativeRealtimeMessage<'_>) -> Result<()> {
        let mut state = self.state.lock().await;
        state.check_ready()?;
        match (state.dialect, message) {
            (Driver::Openai, NativeRealtimeMessage::Openai(value)) => state.send(value).await,
            (Driver::Gemini, NativeRealtimeMessage::Gemini(value)) => {
                if value.setup.is_some() {
                    return Err(Error::unsupported(
                        "Live setup can only be sent as the first frame",
                    ));
                }
                state.send(value).await
            }
            (Driver::Gemini, NativeRealtimeMessage::Raw(value)) if value.get("setup").is_some() => {
                Err(Error::unsupported(
                    "Live setup can only be sent as the first frame",
                ))
            }
            (_, NativeRealtimeMessage::Raw(value)) => state.send(value).await,
            _ => Err(Error::invalid(
                "native message dialect does not match the session",
            )),
        }
    }
    pub async fn close(&self, code: u16, reason: impl Into<String>) -> Result<()> {
        let mut state = self.state.lock().await;
        if state.closed {
            return Ok(());
        }
        state
            .outgoing
            .send(WsFrame::Close(Some(WsClose {
                code,
                reason: reason.into(),
            })))
            .await
            .map_err(|e| Error::network_cause(e.as_ref()))?;
        state.closed = true;
        Ok(())
    }
}
async fn create(state: &mut State) -> Result<()> {
    match state.dialect {
        Driver::Openai => {
            state
                .send(
                    o::ResponseCreateEvent::builder(o::ResponseCreateEventType::ResponseCreate)
                        .build(),
                )
                .await
        }
        Driver::Gemini => {
            state
                .send(
                    l::LiveClientMessage::builder()
                        .client_content(
                            l::BidiGenerateContentClientContent::builder()
                                .turn_complete(true)
                                .build(),
                        )
                        .build(),
                )
                .await
        }
    }
}
async fn cancel(state: &mut State) -> Result<()> {
    match state.dialect {
        Driver::Openai => {
            let mut event =
                o::ResponseCancelEvent::builder(o::ResponseCancelEventType::ResponseCancel).build();
            event.response_id = state.response_id.clone().map(Some);
            state.send(event).await
        }
        Driver::Gemini => Err(Error::unsupported(
            "Live has no response-cancel operation; interrupt clears local playback",
        )),
    }
}
