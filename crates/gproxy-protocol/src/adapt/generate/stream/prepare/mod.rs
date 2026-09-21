//! Pair preparation reuses the declared buffered request mappings and scoped
//! history recovery, then explicitly selects native streaming transport.

mod chat_claude;
mod chat_gemini;
mod chat_responses;
mod claude_gemini;
mod claude_responses;
mod gemini_responses;
mod limits;

use crate::wire::{
    DeclaredFields,
    claude::generate_content as c,
    gemini as g,
    openai::{chat as h, responses as r},
};

pub use chat_gemini::ChatViaGeminiStreamFacts;
pub use claude_gemini::{ClaudeViaGeminiStreamFacts, GeminiViaClaudeStreamFacts};
pub use claude_responses::ResponsesViaClaudeStreamFacts;
pub use gemini_responses::ResponsesViaGeminiStreamFacts;

trait RequestMode: DeclaredFields + Clone {
    fn buffered(self) -> Self;
    fn streaming(self) -> Self;
    fn emit_usage(&self) -> bool {
        true
    }
}

impl RequestMode for h::GenerateContentRequestBody {
    fn buffered(mut self) -> Self {
        self.stream = Some(Some(false));
        self
    }
    fn streaming(mut self) -> Self {
        self.stream = Some(Some(true));
        let mut options = self
            .stream_options
            .take()
            .flatten()
            .unwrap_or_else(|| h::StreamOptions::builder().build())
            .into_declared();
        options.include_usage = Some(true);
        self.stream_options = Some(Some(options));
        self
    }
    fn emit_usage(&self) -> bool {
        if self.stream.flatten() != Some(true) {
            return true;
        }
        self.stream_options
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(|v| v.include_usage)
            == Some(true)
    }
}

impl RequestMode for c::GenerateContentRequestBody {
    fn buffered(mut self) -> Self {
        self.stream = Some(false);
        self
    }
    fn streaming(mut self) -> Self {
        self.stream = Some(true);
        self
    }
}

impl RequestMode for r::GenerateContentRequestBody {
    fn buffered(mut self) -> Self {
        self.stream = Some(Some(false));
        self
    }
    fn streaming(mut self) -> Self {
        self.stream = Some(Some(true));
        self
    }
}

impl RequestMode for g::GenerateContentRequestBody {
    fn buffered(self) -> Self {
        self
    }
    fn streaming(self) -> Self {
        self
    }
}
