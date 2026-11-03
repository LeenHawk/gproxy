//! Native dialect dispatch. Shared mappings remain outside vendor modules.
#[cfg(feature = "chat")]
pub mod chat;
#[cfg(feature = "claude")]
pub mod claude;
#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "openai")]
pub mod openai;
pub(crate) mod shared;
use crate::{Error, Result, ir::*};
use base64::Engine;
#[cfg(any(feature = "claude", feature = "openai", feature = "chat"))]
use gproxy_protocol::Rest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub(crate) use shared::*;
pub(crate) use shared::{citations, media};
#[cfg(feature = "gemini")]
use std::collections::BTreeMap;
pub fn render(request: &GenerateRequest, streaming: bool) -> Result<Rendered<Value>> {
    #[cfg(not(any(feature = "claude", feature = "openai", feature = "chat")))]
    let _ = streaming;
    fn json<T: Serialize>(rendered: Rendered<T>) -> Result<Rendered<Value>> {
        Ok(Rendered {
            request: serde_json::to_value(rendered.request).map_err(Error::invalid)?,
            warnings: rendered.warnings,
        })
    }
    match request.model.dialect {
        #[cfg(feature = "claude")]
        Dialect::Claude => json(claude::render(request, streaming)?),
        #[cfg(feature = "openai")]
        Dialect::Openai => json(openai::render(request, streaming)?),
        #[cfg(feature = "chat")]
        Dialect::OpenaiChat => json(chat::render(request, streaming)?),
        #[cfg(feature = "gemini")]
        Dialect::Gemini => json(gemini::render(request)?),
    }
}

pub(crate) enum StreamState {
    #[cfg(feature = "claude")]
    Claude(claude::StreamState),
    #[cfg(feature = "openai")]
    Openai(openai::StreamState),
    #[cfg(feature = "chat")]
    Chat(Box<chat::StreamState>),
    #[cfg(feature = "gemini")]
    Gemini(gemini::StreamState),
}
impl StreamState {
    pub(crate) fn new(dialect: Dialect, _format: Option<AudioFormat>) -> Self {
        match dialect {
            #[cfg(feature = "claude")]
            Dialect::Claude => Self::Claude(claude::StreamState::new()),
            #[cfg(feature = "openai")]
            Dialect::Openai => Self::Openai(openai::StreamState::new()),
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => Self::Chat(Box::new(chat::StreamState::new(_format))),
            #[cfg(feature = "gemini")]
            Dialect::Gemini => Self::Gemini(gemini::StreamState::new()),
        }
    }
    pub(crate) fn usage(&self) -> Usage {
        match self {
            #[cfg(feature = "claude")]
            Self::Claude(state) => state.usage(),
            #[cfg(feature = "openai")]
            Self::Openai(state) => state.usage(),
            #[cfg(feature = "chat")]
            Self::Chat(state) => state.usage(),
            #[cfg(feature = "gemini")]
            Self::Gemini(state) => state.usage(),
        }
    }
    pub(crate) fn push(&mut self, data: &str) -> Result<Vec<crate::stream::Event>> {
        match self {
            #[cfg(feature = "claude")]
            Self::Claude(state) => state.push(data),
            #[cfg(feature = "openai")]
            Self::Openai(state) => state.push(data),
            #[cfg(feature = "chat")]
            Self::Chat(state) => state.push(data),
            #[cfg(feature = "gemini")]
            Self::Gemini(state) => state.push(data),
        }
    }
    pub(crate) fn finish(self) -> Result<Vec<crate::stream::Event>> {
        match self {
            #[cfg(feature = "claude")]
            Self::Claude(state) => state.finish(),
            #[cfg(feature = "openai")]
            Self::Openai(state) => state.finish(),
            #[cfg(feature = "chat")]
            Self::Chat(state) => state.finish(),
            #[cfg(feature = "gemini")]
            Self::Gemini(state) => state.finish(),
        }
    }
}

mod parse;
pub(crate) use parse::parse;
