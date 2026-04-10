//! WebSocket protocol bridge: transparent or cross-protocol duplex relay with usage tracking.

use gproxy_sdk::provider::engine::Usage;
use gproxy_sdk::provider::response::UpstreamError;

// ---------------------------------------------------------------------------
// WsProtocolBridge trait
// ---------------------------------------------------------------------------

/// Abstraction for bidirectional WS message conversion with usage tracking.
///
/// Implementations handle:
/// - **PassthroughBridge**: same-protocol transparent relay + usage extraction
/// - **OpenAiToGeminiBridge**: OpenAI Responses WS ↔ Gemini Live WS
/// - **GeminiToOpenAiBridge**: Gemini Live WS ↔ OpenAI Responses WS
pub(crate) trait WsProtocolBridge: Send {
    /// Convert a downstream (client) text message into zero or more upstream messages.
    fn convert_client_message(&mut self, msg: &str) -> Result<Vec<String>, UpstreamError>;

    /// Convert an upstream (server) text message into zero or more downstream messages.
    /// Returns extracted usage from this message, if any.
    fn convert_server_message(
        &mut self,
        msg: &str,
    ) -> Result<(Vec<String>, Option<Usage>), UpstreamError>;

    /// Accumulated usage over the entire connection lifetime.
    fn final_usage(&self) -> Option<Usage>;

    /// Take and reset accumulated usage. Used on model change to snapshot
    /// the current response's usage before starting a new model segment.
    fn take_accumulated_usage(&mut self) -> Option<Usage>;
}

// ---------------------------------------------------------------------------
// PassthroughBridge — same protocol, usage tracking only
// ---------------------------------------------------------------------------

pub(crate) struct PassthroughBridge {
    protocol: String,
    accumulated_usage: Usage,
    has_usage: bool,
}

impl PassthroughBridge {
    pub fn new(protocol: impl Into<String>) -> Self {
        Self {
            protocol: protocol.into(),
            accumulated_usage: Usage::default(),
            has_usage: false,
        }
    }
}

impl WsProtocolBridge for PassthroughBridge {
    fn convert_client_message(&mut self, msg: &str) -> Result<Vec<String>, UpstreamError> {
        Ok(vec![msg.to_string()])
    }

    fn convert_server_message(
        &mut self,
        msg: &str,
    ) -> Result<(Vec<String>, Option<Usage>), UpstreamError> {
        let usage = extract_ws_usage(&self.protocol, msg.as_bytes());
        if let Some(ref u) = usage {
            merge_usage(&mut self.accumulated_usage, u);
            self.has_usage = true;
        }
        Ok((vec![msg.to_string()], usage))
    }

    fn final_usage(&self) -> Option<Usage> {
        if self.has_usage {
            Some(self.accumulated_usage.clone())
        } else {
            None
        }
    }

    fn take_accumulated_usage(&mut self) -> Option<Usage> {
        if self.has_usage {
            self.has_usage = false;
            Some(std::mem::take(&mut self.accumulated_usage))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Usage extraction from WS messages
// ---------------------------------------------------------------------------

fn extract_ws_usage(protocol: &str, msg: &[u8]) -> Option<Usage> {
    match protocol {
        "openai" | "openai_response" => extract_openai_ws_usage(msg),
        "gemini" => extract_gemini_ws_usage(msg),
        _ => None,
    }
}

fn extract_openai_ws_usage(msg: &[u8]) -> Option<Usage> {
    use gproxy_sdk::protocol::openai::create_response::stream::ResponseStreamEvent;

    // OpenAI WS server messages can be stream events, errors, etc.
    // Try to parse as ResponseStreamEvent (most common)
    let event: ResponseStreamEvent = serde_json::from_slice(msg).ok()?;
    match event {
        ResponseStreamEvent::Created { response, .. }
        | ResponseStreamEvent::Queued { response, .. }
        | ResponseStreamEvent::InProgress { response, .. }
        | ResponseStreamEvent::Completed { response, .. }
        | ResponseStreamEvent::Incomplete { response, .. }
        | ResponseStreamEvent::Failed { response, .. } => {
            let u = response.usage?;
            Some(Usage {
                input_tokens: i64::try_from(u.input_tokens).ok(),
                output_tokens: i64::try_from(u.output_tokens).ok(),
                cache_read_input_tokens: i64::try_from(u.input_tokens_details.cached_tokens).ok(),
                cache_creation_input_tokens: None,
                cache_creation_input_tokens_5min: None,
                cache_creation_input_tokens_1h: None,
            })
        }
        _ => None,
    }
}

fn extract_gemini_ws_usage(msg: &[u8]) -> Option<Usage> {
    use gproxy_sdk::protocol::gemini::live::types::GeminiBidiGenerateContentServerMessage;

    let server_msg: GeminiBidiGenerateContentServerMessage = serde_json::from_slice(msg).ok()?;
    let u = server_msg.usage_metadata?;
    Some(Usage {
        input_tokens: u.prompt_token_count.and_then(|v| i64::try_from(v).ok()),
        output_tokens: u.response_token_count.and_then(|v| i64::try_from(v).ok()),
        cache_read_input_tokens: u
            .cached_content_token_count
            .and_then(|v| i64::try_from(v).ok()),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    })
}

fn merge_usage(dst: &mut Usage, src: &Usage) {
    if src.input_tokens.is_some() {
        dst.input_tokens = src.input_tokens;
    }
    if src.output_tokens.is_some() {
        dst.output_tokens = src.output_tokens;
    }
    if src.cache_read_input_tokens.is_some() {
        dst.cache_read_input_tokens = src.cache_read_input_tokens;
    }
    if src.cache_creation_input_tokens.is_some() {
        dst.cache_creation_input_tokens = src.cache_creation_input_tokens;
    }
    if src.cache_creation_input_tokens_5min.is_some() {
        dst.cache_creation_input_tokens_5min = src.cache_creation_input_tokens_5min;
    }
    if src.cache_creation_input_tokens_1h.is_some() {
        dst.cache_creation_input_tokens_1h = src.cache_creation_input_tokens_1h;
    }
}

// ---------------------------------------------------------------------------
// OpenAiToGeminiBridge — OpenAI WS client ↔ Gemini Live upstream
// ---------------------------------------------------------------------------

pub(crate) struct OpenAiToGeminiBridge {
    _model: Option<String>,
    setup_sent: bool,
    /// Stateful converter: Gemini HTTP chunks → OpenAI Response stream events
    stream_converter:
        gproxy_sdk::protocol::transform::openai::stream_generate_content::openai_response::gemini::response::GeminiToOpenAiResponseStream,
    accumulated_usage: Usage,
    has_usage: bool,
}

impl OpenAiToGeminiBridge {
    pub fn new(model: Option<String>) -> Self {
        Self {
            _model: model,
            setup_sent: false,
            stream_converter: Default::default(),
            accumulated_usage: Usage::default(),
            has_usage: false,
        }
    }
}

impl WsProtocolBridge for OpenAiToGeminiBridge {
    fn convert_client_message(&mut self, msg: &str) -> Result<Vec<String>, UpstreamError> {
        use gproxy_sdk::protocol::gemini::live::types::GeminiBidiGenerateContentClientMessage;
        use gproxy_sdk::protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest;
        use gproxy_sdk::protocol::openai::create_response::websocket::types::OpenAiCreateResponseWebSocketClientMessage;

        // Parse OpenAI WS client message
        let client_msg: OpenAiCreateResponseWebSocketClientMessage = serde_json::from_str(msg)
            .map_err(|e| {
                UpstreamError::Channel(format!("failed to parse OpenAI WS client message: {e}"))
            })?;

        // OpenAI WS msg → OpenAI HTTP request
        let openai_request =
            gproxy_sdk::protocol::openai::create_response::request::OpenAiCreateResponseRequest::try_from(
                &client_msg,
            )
            .map_err(|e| UpstreamError::Channel(format!("OpenAI WS → HTTP request: {e}")))?;

        // OpenAI HTTP request → Gemini Stream request
        let gemini_request = GeminiStreamGenerateContentRequest::try_from(&openai_request)
            .map_err(|e| UpstreamError::Channel(format!("OpenAI → Gemini request: {e}")))?;

        // Gemini Stream request → Gemini Live WS frames
        let frames = Vec::<GeminiBidiGenerateContentClientMessage>::try_from(&gemini_request)
            .map_err(|e| UpstreamError::Channel(format!("Gemini HTTP → Live frames: {e}")))?;

        // If this is the first message and no setup frame was generated,
        // we might need to send the setup frame explicitly
        if !self.setup_sent {
            self.setup_sent = true;
        }

        // Serialize all frames to JSON
        let mut out = Vec::with_capacity(frames.len());
        for frame in &frames {
            let json = serde_json::to_string(frame)
                .map_err(|e| UpstreamError::Channel(format!("serialize Gemini frame: {e}")))?;
            out.push(json);
        }
        Ok(out)
    }

    fn convert_server_message(
        &mut self,
        msg: &str,
    ) -> Result<(Vec<String>, Option<Usage>), UpstreamError> {
        use gproxy_sdk::protocol::gemini::live::types::{
            GeminiBidiGenerateContentServerMessage, GeminiBidiGenerateContentServerMessageType,
        };
        use gproxy_sdk::protocol::transform::gemini::websocket::context::GeminiWebsocketTransformContext;
        use gproxy_sdk::protocol::transform::gemini::websocket::to_http::response::server_message_to_chunk;

        // Parse Gemini Live server message
        let server_msg: GeminiBidiGenerateContentServerMessage = serde_json::from_str(msg)
            .map_err(|e| {
                UpstreamError::Channel(format!("failed to parse Gemini Live message: {e}"))
            })?;

        // Extract usage from this message
        let usage = extract_gemini_ws_usage(msg.as_bytes());
        if let Some(ref u) = usage {
            merge_usage(&mut self.accumulated_usage, u);
            self.has_usage = true;
        }

        // Check for interrupted/generation_complete before converting
        let is_complete = matches!(
            &server_msg.message_type,
            GeminiBidiGenerateContentServerMessageType::ServerContent { server_content }
                if server_content.generation_complete == Some(true)
                    || server_content.interrupted == Some(true)
        );

        // Convert Gemini Live msg → Gemini HTTP chunk (reusing existing converter)
        let mut ctx = GeminiWebsocketTransformContext::default();
        let mut openai_events = Vec::new();

        if let Some(chunk) = server_message_to_chunk(server_msg, &mut ctx) {
            self.stream_converter.on_chunk(chunk, &mut openai_events);
        }

        if is_complete {
            self.stream_converter.finish(&mut openai_events);
        }

        // Serialize OpenAI events as WS messages
        let mut out = Vec::with_capacity(openai_events.len());
        for event in &openai_events {
            let json = serde_json::to_string(event)
                .map_err(|e| UpstreamError::Channel(format!("serialize OpenAI event: {e}")))?;
            out.push(json);
        }
        Ok((out, usage))
    }

    fn final_usage(&self) -> Option<Usage> {
        if self.has_usage {
            Some(self.accumulated_usage.clone())
        } else {
            None
        }
    }

    fn take_accumulated_usage(&mut self) -> Option<Usage> {
        if self.has_usage {
            self.has_usage = false;
            Some(std::mem::take(&mut self.accumulated_usage))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// GeminiToOpenAiBridge — Gemini Live client ↔ OpenAI WS upstream
// ---------------------------------------------------------------------------

pub(crate) struct GeminiToOpenAiBridge {
    _model: Option<String>,
    /// Stateful converter: OpenAI Response events → Gemini HTTP chunks
    stream_converter:
        gproxy_sdk::protocol::transform::gemini::stream_generate_content::openai_response::response::OpenAiResponseToGeminiStream,
    accumulated_usage: Usage,
    has_usage: bool,
}

impl GeminiToOpenAiBridge {
    pub fn new(model: Option<String>) -> Self {
        Self {
            _model: model,
            stream_converter: Default::default(),
            accumulated_usage: Usage::default(),
            has_usage: false,
        }
    }
}

impl WsProtocolBridge for GeminiToOpenAiBridge {
    fn convert_client_message(&mut self, msg: &str) -> Result<Vec<String>, UpstreamError> {
        use gproxy_sdk::protocol::gemini::live::types::GeminiBidiGenerateContentClientMessage;
        use gproxy_sdk::protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest;
        use gproxy_sdk::protocol::openai::create_response::request::OpenAiCreateResponseRequest;
        use gproxy_sdk::protocol::openai::create_response::websocket::types::OpenAiCreateResponseWebSocketClientMessage;

        // Parse Gemini Live client message
        let client_msg: GeminiBidiGenerateContentClientMessage = serde_json::from_str(msg)
            .map_err(|e| {
                UpstreamError::Channel(format!("failed to parse Gemini Live client message: {e}"))
            })?;

        // Gemini Live msg → Gemini HTTP stream request (takes single message as slice)
        let gemini_request = GeminiStreamGenerateContentRequest::try_from(&[client_msg] as &[_])
            .map_err(|e| UpstreamError::Channel(format!("Gemini Live → HTTP request: {e}")))?;

        // Gemini HTTP request → OpenAI Response request (owned)
        let openai_request = OpenAiCreateResponseRequest::try_from(gemini_request)
            .map_err(|e| UpstreamError::Channel(format!("Gemini → OpenAI request: {e}")))?;

        // Wrap as OpenAI WS ResponseCreate message
        let ws_msg =
            OpenAiCreateResponseWebSocketClientMessage::ResponseCreate(
                gproxy_sdk::protocol::openai::create_response::websocket::types::OpenAiCreateResponseCreateWebSocketRequestBody {
                    request: openai_request.body,
                    generate: None,
                    client_metadata: None,
                },
            );

        let json = serde_json::to_string(&ws_msg)
            .map_err(|e| UpstreamError::Channel(format!("serialize OpenAI WS message: {e}")))?;
        Ok(vec![json])
    }

    fn convert_server_message(
        &mut self,
        msg: &str,
    ) -> Result<(Vec<String>, Option<Usage>), UpstreamError> {
        use gproxy_sdk::protocol::openai::create_response::stream::ResponseStreamEvent;

        // Parse OpenAI WS server message — try as stream event first
        let event: ResponseStreamEvent = serde_json::from_str(msg)
            .map_err(|e| UpstreamError::Channel(format!("failed to parse OpenAI WS event: {e}")))?;

        // Extract usage from OpenAI event
        let usage = extract_openai_ws_usage(msg.as_bytes());
        if let Some(ref u) = usage {
            merge_usage(&mut self.accumulated_usage, u);
            self.has_usage = true;
        }

        // Convert OpenAI event → Gemini HTTP chunks
        let mut gemini_chunks = Vec::new();
        self.stream_converter
            .on_stream_event(event, &mut gemini_chunks);

        // Convert Gemini HTTP chunks → Gemini Live server messages (reusing existing converter)
        use gproxy_sdk::protocol::gemini::live::response::GeminiLiveMessageResponse;
        use gproxy_sdk::protocol::transform::gemini::websocket::from_http::response::{
            candidate_to_server_message, usage_generate_to_live,
        };

        let mut out = Vec::new();
        for chunk in gemini_chunks {
            let live_usage = usage_generate_to_live(chunk.usage_metadata);

            if let Some(candidates) = chunk.candidates {
                for candidate in candidates {
                    if let Some(live_msg) =
                        candidate_to_server_message(candidate, live_usage.clone())
                    {
                        match live_msg {
                            GeminiLiveMessageResponse::Message(msg) => {
                                let json = serde_json::to_string(&msg).map_err(|e| {
                                    UpstreamError::Channel(format!(
                                        "serialize Gemini Live message: {e}"
                                    ))
                                })?;
                                out.push(json);
                            }
                            GeminiLiveMessageResponse::Error(err) => {
                                let json = serde_json::to_string(&err).map_err(|e| {
                                    UpstreamError::Channel(format!(
                                        "serialize Gemini Live error: {e}"
                                    ))
                                })?;
                                out.push(json);
                            }
                        }
                    }
                }
            }
        }

        Ok((out, usage))
    }

    fn final_usage(&self) -> Option<Usage> {
        if self.has_usage {
            Some(self.accumulated_usage.clone())
        } else {
            None
        }
    }

    fn take_accumulated_usage(&mut self) -> Option<Usage> {
        if self.has_usage {
            self.has_usage = false;
            Some(std::mem::take(&mut self.accumulated_usage))
        } else {
            None
        }
    }
}
