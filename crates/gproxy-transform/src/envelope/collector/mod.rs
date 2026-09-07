use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind, claude as claude_wire, openai};

use self::chat::ChatCollector;
use self::claude::ClaudeCollector;
use self::gemini::GeminiCollector;
use self::responses::ResponsesCollector;

mod chat;
mod claude;
mod gemini;
mod responses;

use super::{SseDecoder, SseFrame};
use crate::TransformError;

pub enum BufferedResponse {
    OpenAiChat(Box<openai::ChatCompletionResponse>),
    OpenAiResponses(Box<openai::ResponseObject>),
    Claude(Box<claude_wire::CreateMessageResponseBody>),
    Gemini(Box<gproxy_protocol::gemini::GenerateContentResponse>),
}

impl BufferedResponse {
    pub fn into_bytes(self) -> Result<Bytes, TransformError> {
        Ok(Bytes::from(match self {
            Self::OpenAiChat(response) => serde_json::to_vec(&response)?,
            Self::OpenAiResponses(response) => serde_json::to_vec(&response)?,
            Self::Claude(response) => serde_json::to_vec(&response)?,
            Self::Gemini(response) => serde_json::to_vec(&response)?,
        }))
    }
}

pub struct ResponseCollector {
    decoder: SseDecoder,
    state: Collector,
}

enum Collector {
    Chat(Box<ChatCollector>),
    Responses(Box<ResponsesCollector>),
    Claude(Box<ClaudeCollector>),
    Gemini(Box<GeminiCollector>),
}

impl ResponseCollector {
    pub fn new(kind: ContentGenerationKind) -> Result<Self, TransformError> {
        let state = match kind {
            ContentGenerationKind::OpenAiChat => Collector::Chat(Box::default()),
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket => {
                Collector::Responses(Box::default())
            }
            ContentGenerationKind::ClaudeMessages => Collector::Claude(Box::default()),
            ContentGenerationKind::GeminiGenerateContent => Collector::Gemini(Box::default()),
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        };
        Ok(Self {
            decoder: SseDecoder::default(),
            state,
        })
    }

    pub fn push(&mut self, chunk: Bytes) -> Result<(), TransformError> {
        for frame in self.decoder.push(&chunk)? {
            self.state.frame(frame)?;
        }
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.state.is_complete()
    }

    pub fn claude_has_output(&self) -> bool {
        matches!(&self.state, Collector::Claude(state) if state.has_output())
    }

    pub fn claude_has_open_tool(&self) -> bool {
        matches!(&self.state, Collector::Claude(state) if !state.open_tools.is_empty())
    }

    pub fn finish(mut self) -> Result<BufferedResponse, TransformError> {
        if let Some(frame) = self.decoder.finish()? {
            self.state.frame(frame)?;
        }
        self.state.finish()
    }
}

impl Collector {
    fn frame(&mut self, frame: SseFrame) -> Result<(), TransformError> {
        match self {
            Self::Chat(state) => state.frame(frame),
            Self::Responses(state) => state.frame(frame),
            Self::Claude(state) => state.frame(frame),
            Self::Gemini(state) => state.frame(frame),
        }
    }

    fn is_complete(&self) -> bool {
        match self {
            Self::Chat(state) => state.is_complete(),
            Self::Responses(state) => state.response.is_some(),
            Self::Claude(state) => state.complete,
            Self::Gemini(state) => state.is_complete(),
        }
    }

    fn finish(self) -> Result<BufferedResponse, TransformError> {
        match self {
            Self::Chat(state) => state
                .finish()
                .map(Box::new)
                .map(BufferedResponse::OpenAiChat),
            Self::Responses(state) => state
                .finish()
                .map(Box::new)
                .map(BufferedResponse::OpenAiResponses),
            Self::Claude(state) => state.finish().map(Box::new).map(BufferedResponse::Claude),
            Self::Gemini(state) => state.finish().map(Box::new).map(BufferedResponse::Gemini),
        }
    }
}
