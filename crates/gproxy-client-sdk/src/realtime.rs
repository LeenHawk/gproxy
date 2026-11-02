//! OpenAI Realtime over the existing duplex transport. `OpenaiSessionCodec`
//! also works with a host-owned WebRTC data channel; peer/media tracks belong
//! to the host. Gemini Live is not implemented yet.
use crate::*;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    capability::UpstreamConnection,
    connection::{WebSocket, WsFrame},
    openai::realtime as r,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

pub struct OpenaiRealtimeOptions {
    pub model: String,
    pub session: r::RealtimeSessionCreateRequest,
}
impl OpenaiRealtimeOptions {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            session: r::RealtimeSessionCreateRequest::builder(
                r::RealtimeSessionCreateRequestType::Realtime,
            )
            .build(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Audio { bytes: Vec<u8> },
    CommitAudio,
    Text { text: String },
    ToolResult(ToolResult),
    Respond,
    Cancel,
}
#[derive(Debug, Clone, PartialEq)]
pub enum RealtimeEvent {
    TextDelta {
        item_id: String,
        part: u32,
        delta: String,
    },
    AudioDelta {
        item_id: String,
        part: u32,
        bytes: Vec<u8>,
        format: AudioFormat,
    },
    TranscriptDelta {
        item_id: String,
        part: u32,
        delta: String,
    },
    ToolCallArgsDelta {
        item_id: String,
        delta: String,
    },
    Message {
        item_id: String,
        message: Message,
    },
    Native(Box<r::RealtimeServerEvent>),
}

pub struct RealtimeSession {
    socket: WebSocket,
    pub codec: OpenaiSessionCodec,
}
impl RealtimeSession {
    /// Split to send microphone chunks while a receive is pending. Both halves
    /// share the ledger; dropping both releases the duplex transport.
    pub fn split(self) -> (RealtimeSender, RealtimeReceiver) {
        let codec = Arc::new(Mutex::new(self.codec));
        let outgoing = Arc::new(futures_util::lock::Mutex::new(self.socket.outgoing));
        (
            RealtimeSender {
                outgoing: outgoing.clone(),
                codec: codec.clone(),
            },
            RealtimeReceiver {
                incoming: self.socket.incoming,
                outgoing,
                codec,
            },
        )
    }
    pub async fn send(&mut self, command: Command) -> Result<()> {
        let event = self.codec.command(command)?;
        self.socket
            .outgoing
            .send(WsFrame::Text(
                serde_json::to_string(&event).map_err(Error::invalid)?,
            ))
            .await
            .map_err(Error::network)
    }
    pub async fn receive(&mut self) -> Result<Option<RealtimeEvent>> {
        while let Some(frame) = self.socket.incoming.next().await {
            let data = match frame.map_err(Error::network)? {
                WsFrame::Text(text) => text,
                WsFrame::Binary(bytes) => {
                    String::from_utf8(bytes.to_vec()).map_err(Error::invalid)?
                }
                WsFrame::Ping(bytes) => {
                    self.socket
                        .outgoing
                        .send(WsFrame::Pong(bytes))
                        .await
                        .map_err(Error::network)?;
                    continue;
                }
                WsFrame::Pong(_) => continue,
                WsFrame::Close(_) => return Ok(None),
            };
            return self.codec.receive(&data).map(Some);
        }
        Ok(None)
    }
    pub fn conversation(&self) -> Conversation {
        self.codec.conversation()
    }
    pub async fn close(mut self) -> Result<Conversation> {
        self.socket.outgoing.close().await.map_err(Error::network)?;
        Ok(self.codec.conversation())
    }
}
pub struct RealtimeSender {
    outgoing: Arc<futures_util::lock::Mutex<protocol::connection::WsSender>>,
    codec: Arc<Mutex<OpenaiSessionCodec>>,
}
impl RealtimeSender {
    pub async fn send(&mut self, command: Command) -> Result<()> {
        let event = self
            .codec
            .lock()
            .map_err(Error::invalid)?
            .command(command)?;
        let text = serde_json::to_string(&event).map_err(Error::invalid)?;
        self.outgoing
            .lock()
            .await
            .send(WsFrame::Text(text))
            .await
            .map_err(Error::network)
    }
    pub fn conversation(&self) -> Result<Conversation> {
        Ok(self.codec.lock().map_err(Error::invalid)?.conversation())
    }
    pub async fn close(&mut self) -> Result<()> {
        self.outgoing
            .lock()
            .await
            .close()
            .await
            .map_err(Error::network)
    }
}
pub struct RealtimeReceiver {
    incoming: protocol::connection::WsReceiver,
    outgoing: Arc<futures_util::lock::Mutex<protocol::connection::WsSender>>,
    codec: Arc<Mutex<OpenaiSessionCodec>>,
}
impl RealtimeReceiver {
    pub async fn receive(&mut self) -> Result<Option<RealtimeEvent>> {
        while let Some(frame) = self.incoming.next().await {
            let data = match frame.map_err(Error::network)? {
                WsFrame::Text(text) => text,
                WsFrame::Binary(bytes) => {
                    String::from_utf8(bytes.to_vec()).map_err(Error::invalid)?
                }
                WsFrame::Ping(bytes) => {
                    self.outgoing
                        .lock()
                        .await
                        .send(WsFrame::Pong(bytes))
                        .await
                        .map_err(Error::network)?;
                    continue;
                }
                WsFrame::Pong(_) => continue,
                WsFrame::Close(_) => return Ok(None),
            };
            return self
                .codec
                .lock()
                .map_err(Error::invalid)?
                .receive(&data)
                .map(Some);
        }
        Ok(None)
    }
    pub fn conversation(&self) -> Result<Conversation> {
        Ok(self.codec.lock().map_err(Error::invalid)?.conversation())
    }
}
impl<B: Backend> Client<B> {
    pub async fn connect_realtime(
        &self,
        options: OpenaiRealtimeOptions,
    ) -> Result<RealtimeSession> {
        let codec = OpenaiSessionCodec::new(&options.session);
        let key = protocol::OperationKey {
            operation: protocol::Operation::ConnectRealtime,
            dialect: protocol::Dialect::OpenAi,
        };
        let request = protocol::WireRequest {
            method: http::Method::GET,
            path: "/v1/realtime".into(),
            query: Some(format!("model={}", crate::path_segment(&options.model))),
            headers: http::HeaderMap::new(),
            body: (),
        };
        let socket = match self.backend.connect(key, &options.model, request).await? {
            UpstreamConnection::Connected { socket, .. } => socket,
            UpstreamConnection::Rejected(response) => {
                let bytes = protocol::codec::read_http_body(response.body, self.limits).await?;
                return Err(Error::from_response(
                    response.status.as_u16(),
                    &response.headers,
                    String::from_utf8_lossy(&bytes).into_owned(),
                ));
            }
        };
        let mut session = RealtimeSession { socket, codec };
        let mut settings = options.session;
        settings.model = None;
        session
            .send_native(&r::RealtimeClientEvent::SessionUpdateEvent(
                r::SessionUpdateEvent::builder(
                    r::SessionUpdateEventSession::RealtimeSessionCreateRequest(settings),
                    r::SessionUpdateEventType::SessionUpdate,
                )
                .build(),
            ))
            .await?;
        Ok(session)
    }
    /// Return the SDP answer and call Location. The host owns its RTCPeerConnection
    /// and can run `OpenaiSessionCodec` on the negotiated data channel.
    pub async fn create_realtime_call(
        &self,
        options: OpenaiRealtimeOptions,
        sdp: String,
    ) -> Result<RealtimeCall> {
        let mut session = options.session;
        session.model = Some(Some(options.model.clone()));
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/sdp"),
        );
        let part = protocol::connection::MultipartPart {
            headers,
            body: protocol::HttpBody::Bytes(sdp.into()),
        };
        let form = r::CreateRealtimeCallMultipartForm::builder(part)
            .session(session)
            .build();
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(Error::invalid)?;
        let boundary = format!("gproxy_sdk_{:032x}", u128::from_le_bytes(random));
        let mut sdp = form.sdp;
        sdp.headers.insert(
            http::header::CONTENT_DISPOSITION,
            http::HeaderValue::from_static("form-data; name=\"sdp\""),
        );
        let mut parts = vec![sdp];
        if let Some(session) = form.session {
            let mut headers = http::HeaderMap::new();
            headers.insert(
                http::header::CONTENT_DISPOSITION,
                http::HeaderValue::from_static("form-data; name=\"session\""),
            );
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
            parts.push(protocol::connection::MultipartPart {
                headers,
                body: protocol::HttpBody::Bytes(
                    serde_json::to_vec(&session).map_err(Error::invalid)?.into(),
                ),
            });
        }
        let encoder = protocol::codec::MultipartEncoder::new(boundary.clone(), parts, self.limits)?;
        let stream = futures_util::stream::try_unfold(encoder, |mut encoder| async move {
            let next = encoder
                .next_chunk()
                .await
                .map_err(|e| Box::new(e) as protocol::connection::TransportError)?;
            Ok(next.map(|chunk| (chunk, encoder)))
        });
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
                .map_err(Error::invalid)?,
        );
        let request = protocol::WireRequest {
            method: http::Method::POST,
            path: "/v1/realtime/calls".into(),
            query: None,
            headers,
            body: protocol::HttpBody::Stream(Box::pin(stream)),
        };
        let response = self
            .send(
                protocol::OperationKey {
                    operation: protocol::Operation::CreateRealtimeCall,
                    dialect: protocol::Dialect::OpenAi,
                },
                Some(&options.model),
                request,
            )
            .await?;
        let location = response
            .headers
            .get(http::header::LOCATION)
            .map(|v| v.to_str().map(str::to_owned))
            .transpose()
            .map_err(Error::invalid)?;
        let bytes = protocol::codec::read_http_body(response.body, self.limits).await?;
        Ok(RealtimeCall {
            sdp: String::from_utf8(bytes.to_vec()).map_err(Error::invalid)?,
            location,
        })
    }
}
impl RealtimeSession {
    async fn send_native(&mut self, event: &r::RealtimeClientEvent) -> Result<()> {
        self.socket
            .outgoing
            .send(WsFrame::Text(
                serde_json::to_string(event).map_err(Error::invalid)?,
            ))
            .await
            .map_err(Error::network)
    }
}
pub struct RealtimeCall {
    pub sdp: String,
    pub location: Option<String>,
}

/// Typed event conversion and opt-in ledger collection, usable without a socket.
/// Completed native items replace earlier versions with the same item ID.
pub struct OpenaiSessionCodec {
    input_format: AudioFormat,
    output_format: AudioFormat,
    input_audio: Vec<u8>,
    audio: BTreeMap<(String, u32), Vec<u8>>,
    messages: Vec<(String, Message)>,
    next_id: u64,
    pending_audio: VecDeque<String>,
    transcripts: BTreeMap<(String, u32), String>,
}
impl OpenaiSessionCodec {
    pub fn new(session: &r::RealtimeSessionCreateRequest) -> Self {
        let audio = session.audio.as_ref().and_then(Option::as_ref);
        let input = audio
            .and_then(|v| v.input.as_ref())
            .and_then(Option::as_ref)
            .and_then(|v| v.format.as_ref())
            .and_then(Option::as_ref);
        let output = audio
            .and_then(|v| v.output.as_ref())
            .and_then(Option::as_ref)
            .and_then(|v| v.format.as_ref())
            .and_then(Option::as_ref);
        Self {
            input_format: format(input),
            output_format: format(output),
            input_audio: Vec::new(),
            audio: BTreeMap::new(),
            messages: Vec::new(),
            next_id: 0,
            pending_audio: VecDeque::new(),
            transcripts: BTreeMap::new(),
        }
    }
    /// WebRTC hosts feed decoded audio from their media track here when the
    /// data channel does not include response.output_audio.delta payloads.
    pub fn record_output_audio(&mut self, item_id: impl Into<String>, part: u32, bytes: &[u8]) {
        self.audio
            .entry((item_id.into(), part))
            .or_default()
            .extend_from_slice(bytes);
    }
    fn existing_audio(&self, id: &str, part: usize) -> Vec<u8> {
        self.messages
            .iter()
            .find(|(key, _)| key == id)
            .and_then(|(_, m)| m.content.get(part))
            .and_then(|b| match &b.content {
                Content::Audio(Audio {
                    source: Asset::Inline { bytes },
                    ..
                }) => Some(bytes.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
    pub fn conversation(&self) -> Conversation {
        Conversation::new(self.messages.iter().map(|(_, m)| m.clone()).collect())
    }
    fn id(&mut self) -> String {
        let id = format!("sdk_{}", self.next_id);
        self.next_id += 1;
        id
    }
    fn remember(&mut self, id: String, message: Message) {
        if let Some((_, value)) = self.messages.iter_mut().find(|(key, _)| *key == id) {
            *value = message;
        } else {
            self.messages.push((id, message));
        }
    }
    pub fn command(&mut self, command: Command) -> Result<r::RealtimeClientEvent> {
        use r::RealtimeClientEvent as E;
        Ok(match command {
            Command::Audio { bytes } => {
                self.input_audio.extend_from_slice(&bytes);
                E::InputAudioBufferAppendEvent(
                    r::InputAudioBufferAppendEvent::builder(
                        base64::engine::general_purpose::STANDARD.encode(bytes),
                        r::InputAudioBufferAppendEventType::InputAudioBufferAppend,
                    )
                    .build(),
                )
            }
            Command::CommitAudio => {
                let id = self.id();
                let audio = Audio {
                    source: Asset::Inline {
                        bytes: std::mem::take(&mut self.input_audio),
                    },
                    format: self.input_format.clone(),
                    transcript: None,
                };
                self.remember(
                    id.clone(),
                    Message {
                        role: Role::User,
                        content: vec![Block::new(Content::Audio(audio))],
                    },
                );
                self.pending_audio.push_back(id);
                E::InputAudioBufferCommitEvent(
                    r::InputAudioBufferCommitEvent::builder(
                        r::InputAudioBufferCommitEventType::InputAudioBufferCommit,
                    )
                    .build(),
                )
            }
            Command::Text { text } => {
                let id = self.id();
                self.remember(
                    id.clone(),
                    Message {
                        role: Role::User,
                        content: vec![Block::text(&text)],
                    },
                );
                let content = r::RealtimeConversationItemUserMessageContent::builder()
                    .type_(Some(
                        r::RealtimeConversationItemUserMessageContentType::InputText,
                    ))
                    .text(Some(text))
                    .build();
                let item = r::RealtimeConversationItemUserMessage::builder(
                    vec![content],
                    r::RealtimeConversationItemUserMessageRole::User,
                    r::RealtimeConversationItemUserMessageType::Message,
                )
                .id(Some(id))
                .build();
                E::ConversationItemCreateEvent(
                    r::ConversationItemCreateEvent::builder(
                        r::ConversationItem::RealtimeConversationItemUserMessage(item),
                        r::ConversationItemCreateEventType::ConversationItemCreate,
                    )
                    .build(),
                )
            }
            Command::ToolResult(result) => {
                let output = match &result.output {
                    ToolOutput::Json(serde_json::Value::String(text)) => text.clone(),
                    ToolOutput::Json(value) => value.to_string(),
                    ToolOutput::Blocks(_) => {
                        return Err(Error::invalid(
                            "Realtime function output requires JSON or text",
                        ));
                    }
                };
                let id = self.id();
                let item = r::RealtimeConversationItemFunctionCallOutput::builder(
                    result.id.clone(),
                    output,
                    r::RealtimeConversationItemFunctionCallOutputType::FunctionCallOutput,
                )
                .id(Some(id.clone()))
                .build();
                self.remember(
                    id,
                    Message {
                        role: Role::Tool,
                        content: vec![Block::new(Content::ToolResult(result))],
                    },
                );
                E::ConversationItemCreateEvent(
                    r::ConversationItemCreateEvent::builder(
                        r::ConversationItem::RealtimeConversationItemFunctionCallOutput(item),
                        r::ConversationItemCreateEventType::ConversationItemCreate,
                    )
                    .build(),
                )
            }
            Command::Respond => E::ResponseCreateEvent(
                r::ResponseCreateEvent::builder(r::ResponseCreateEventType::ResponseCreate).build(),
            ),
            Command::Cancel => E::ResponseCancelEvent(
                r::ResponseCancelEvent::builder(r::ResponseCancelEventType::ResponseCancel).build(),
            ),
        })
    }
    pub fn receive(&mut self, data: &str) -> Result<RealtimeEvent> {
        use r::RealtimeServerEvent as E;
        let event: E = serde_json::from_str(data).map_err(Error::invalid)?;
        Ok(match event {
            E::ResponseTextDeltaEvent(v) => RealtimeEvent::TextDelta {
                item_id: v.item_id,
                part: index(v.content_index)?,
                delta: v.delta,
            },
            E::ResponseAudioDeltaEvent(v) => {
                let part = index(v.content_index)?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(v.delta)
                    .map_err(Error::invalid)?;
                self.audio
                    .entry((v.item_id.clone(), part))
                    .or_default()
                    .extend_from_slice(&bytes);
                RealtimeEvent::AudioDelta {
                    item_id: v.item_id,
                    part,
                    bytes,
                    format: self.output_format.clone(),
                }
            }
            E::ResponseAudioTranscriptDeltaEvent(v) => {
                let part = index(v.content_index)?;
                self.transcripts
                    .entry((v.item_id.clone(), part))
                    .or_default()
                    .push_str(&v.delta);
                RealtimeEvent::TranscriptDelta {
                    item_id: v.item_id,
                    part,
                    delta: v.delta,
                }
            }
            E::ResponseAudioTranscriptDoneEvent(v) => {
                self.transcripts.insert(
                    (v.item_id.clone(), index(v.content_index)?),
                    v.transcript.clone(),
                );
                RealtimeEvent::Native(Box::new(E::ResponseAudioTranscriptDoneEvent(v)))
            }
            E::ResponseFunctionCallArgumentsDeltaEvent(v) => RealtimeEvent::ToolCallArgsDelta {
                item_id: v.item_id,
                delta: v.delta,
            },
            E::ResponseOutputItemDoneEvent(v) => self.item(v.item)?,
            E::ConversationItemDone(v) => self.item(v.item)?,
            E::InputAudioBufferCommittedEvent(v) => {
                if let Some(local) = self.pending_audio.pop_front()
                    && let Some((id, _)) = self.messages.iter_mut().find(|(id, _)| *id == local)
                {
                    *id = v.item_id.clone();
                }
                RealtimeEvent::Native(Box::new(E::InputAudioBufferCommittedEvent(v)))
            }
            E::ConversationItemInputAudioTranscriptionCompletedEvent(v) => {
                if let Some((_, message)) =
                    self.messages.iter_mut().find(|(id, _)| *id == v.item_id)
                {
                    if let Some(Block {
                        content: Content::Audio(audio),
                        ..
                    }) = message.content.get_mut(index(v.content_index)? as usize)
                    {
                        audio.transcript = Some(v.transcript.clone());
                    }
                }
                RealtimeEvent::Native(Box::new(
                    E::ConversationItemInputAudioTranscriptionCompletedEvent(v),
                ))
            }
            E::ResponseDoneEvent(v) => {
                for item in v.response.output.iter().flatten().flatten() {
                    self.item(item.clone())?;
                }
                RealtimeEvent::Native(Box::new(E::ResponseDoneEvent(v)))
            }
            E::RealtimeErrorEvent(_) => {
                return Err(Error::from_response(
                    200,
                    &http::HeaderMap::new(),
                    data.into(),
                ));
            }
            other => RealtimeEvent::Native(Box::new(other)),
        })
    }
    fn item(&mut self, item: r::ConversationItem) -> Result<RealtimeEvent> {
        use r::ConversationItem as I;
        let (id, role, content) = match item {
            I::RealtimeConversationItemAssistantMessage(v) => {
                let id =
                    v.id.flatten()
                        .ok_or_else(|| Error::invalid("Realtime assistant item has no ID"))?;
                let mut blocks = Vec::new();
                for (part, value) in v.content.into_iter().enumerate() {
                    if let Some(Some(text)) = value.text {
                        blocks.push(realtime_block(
                            Content::Text(Text {
                                text,
                                citations: Vec::new(),
                            }),
                            &id,
                            part,
                            value.rest.clone(),
                        )?);
                    }
                    if value.type_
                        == Some(Some(
                            r::RealtimeConversationItemAssistantMessageContentType::OutputAudio,
                        ))
                    {
                        let streamed_audio = self.audio.remove(&(id.clone(), part as u32));
                        let streamed_transcript =
                            self.transcripts.remove(&(id.clone(), part as u32));
                        let bytes = match value.audio.flatten() {
                            Some(data) => base64::engine::general_purpose::STANDARD
                                .decode(data)
                                .map_err(Error::invalid)?,
                            None => {
                                streamed_audio.unwrap_or_else(|| self.existing_audio(&id, part))
                            }
                        };
                        blocks.push(realtime_block(
                            Content::Audio(Audio {
                                source: Asset::Inline { bytes },
                                format: self.output_format.clone(),
                                transcript: value
                                    .transcript
                                    .flatten()
                                    .or(streamed_transcript)
                                    .or_else(|| {
                                        self.messages
                                            .iter()
                                            .find(|(key, _)| key == &id)
                                            .and_then(|(_, m)| m.content.get(part))
                                            .and_then(|b| match &b.content {
                                                Content::Audio(audio) => audio.transcript.clone(),
                                                _ => None,
                                            })
                                    }),
                            }),
                            &id,
                            part,
                            value.rest,
                        )?);
                    }
                }
                (id, Role::Assistant, blocks)
            }
            I::RealtimeConversationItemUserMessage(v) => {
                let id =
                    v.id.flatten()
                        .ok_or_else(|| Error::invalid("Realtime user item has no ID"))?;
                let previous = self
                    .messages
                    .iter()
                    .find(|(key, _)| *key == id)
                    .map(|(_, m)| m);
                let mut content = Vec::new();
                for (part, value) in v.content.into_iter().enumerate() {
                    if let Some(Some(text)) = value.text {
                        content.push(Block::text(text));
                    }
                    if value.type_
                        == Some(Some(
                            r::RealtimeConversationItemUserMessageContentType::InputAudio,
                        ))
                    {
                        let bytes = match value.audio.flatten() {
                            Some(data) => base64::engine::general_purpose::STANDARD
                                .decode(data)
                                .map_err(Error::invalid)?,
                            None => previous
                                .and_then(|m| m.content.get(part))
                                .and_then(|b| match &b.content {
                                    Content::Audio(Audio {
                                        source: Asset::Inline { bytes },
                                        ..
                                    }) => Some(bytes.clone()),
                                    _ => None,
                                })
                                .unwrap_or_default(),
                        };
                        content.push(Block::new(Content::Audio(Audio {
                            source: Asset::Inline { bytes },
                            format: self.input_format.clone(),
                            transcript: value.transcript.flatten(),
                        })));
                    }
                }
                (id, Role::User, content)
            }
            I::RealtimeConversationItemFunctionCall(v) => {
                let id =
                    v.id.flatten()
                        .ok_or_else(|| Error::invalid("Realtime tool item has no ID"))?;
                let call = ToolCall {
                    id: v
                        .call_id
                        .flatten()
                        .ok_or_else(|| Error::invalid("Realtime function has no call ID"))?,
                    name: v.name,
                    arguments: v.arguments,
                    provider_executed: false,
                };
                (
                    id.clone(),
                    Role::Assistant,
                    vec![realtime_block(Content::ToolCall(call), &id, 0, v.rest)?],
                )
            }
            I::RealtimeConversationItemFunctionCallOutput(v) => {
                let id =
                    v.id.flatten()
                        .ok_or_else(|| Error::invalid("Realtime tool output has no ID"))?;
                let result = ToolResult {
                    id: v.call_id,
                    name: String::new(),
                    output: ToolOutput::Json(serde_json::Value::String(v.output)),
                    is_error: false,
                    provider_executed: false,
                };
                (
                    id.clone(),
                    Role::Tool,
                    vec![realtime_block(Content::ToolResult(result), &id, 0, v.rest)?],
                )
            }
            other => {
                // Nonportable MCP/approval items remain typed native values.
                let mut block = Block::new(Content::Opaque);
                #[derive(serde::Serialize)]
                struct Native {
                    realtime_item: r::ConversationItem,
                }
                let metadata = serde_json::from_value(
                    serde_json::to_value(Native {
                        realtime_item: other,
                    })
                    .map_err(Error::invalid)?,
                )
                .map_err(Error::invalid)?;
                block.provider_metadata.insert(Vendor::Openai, metadata);
                (self.id(), Role::Assistant, vec![block])
            }
        };
        let message = Message { role, content };
        self.remember(id.clone(), message.clone());
        Ok(RealtimeEvent::Message {
            item_id: id,
            message,
        })
    }
}
fn realtime_block(content: Content, id: &str, part: usize, rest: protocol::Rest) -> Result<Block> {
    #[derive(serde::Serialize)]
    struct Metadata<'a> {
        realtime: Native<'a>,
    }
    #[derive(serde::Serialize)]
    struct Native<'a> {
        item_id: &'a str,
        part: usize,
        rest: protocol::Rest,
    }
    let metadata = serde_json::from_value(
        serde_json::to_value(Metadata {
            realtime: Native {
                item_id: id,
                part,
                rest,
            },
        })
        .map_err(Error::invalid)?,
    )
    .map_err(Error::invalid)?;
    let mut block = Block::new(content);
    block.provider_metadata.insert(Vendor::Openai, metadata);
    Ok(block)
}
fn index(value: i64) -> Result<u32> {
    value.try_into().map_err(Error::invalid)
}
fn format(value: Option<&r::RealtimeAudioFormats>) -> AudioFormat {
    match value {
        Some(r::RealtimeAudioFormats::RealtimeAudioFormatsAudioPCMU(_)) => AudioFormat::Encoded {
            media_type: "audio/pcmu".into(),
        },
        Some(r::RealtimeAudioFormats::RealtimeAudioFormatsAudioPCMA(_)) => AudioFormat::Encoded {
            media_type: "audio/pcma".into(),
        },
        Some(r::RealtimeAudioFormats::RealtimeAudioFormatsAudioPCM(_)) | None => {
            AudioFormat::pcm(24000, 1)
        }
    }
}
