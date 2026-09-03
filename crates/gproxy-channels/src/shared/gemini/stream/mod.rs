mod json_array;
mod sse;

use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamCtx, StreamDecoder, StreamEnd, StreamTail};
use gproxy_protocol::gemini::{
    BlockReason, BlockReasonKnown, FinishReason, FinishReasonKnown, GenerateContentResponse,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind, StreamFraming};

pub(crate) struct GeminiStreamDecoder {
    parser: Parser,
    terminal: Terminal,
    response_tier: Option<String>,
    usage: Option<gproxy_channel_api::NormalizedUsage>,
}

enum Parser {
    Sse(sse::Decoder),
    JsonArray(json_array::Decoder),
}

impl GeminiStreamDecoder {
    pub(crate) fn for_operation(ctx: StreamCtx<'_>) -> Option<Self> {
        if ctx.key.operation != Operation::StreamGenerateContent
            || ctx.key.kind
                != OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
        {
            return None;
        }
        let parser = match ctx.framing {
            StreamFraming::Sse => Parser::Sse(sse::Decoder::default()),
            StreamFraming::JsonArray => Parser::JsonArray(json_array::Decoder::default()),
            StreamFraming::WebSocket => return None,
        };
        Some(Self {
            parser,
            terminal: Terminal::default(),
            response_tier: super::usage::response_tier(ctx.response_headers),
            usage: None,
        })
    }

    fn parse(&mut self, chunk: &[u8]) -> Result<Vec<GenerateContentResponse>, ChannelError> {
        match &mut self.parser {
            Parser::Sse(parser) => parser.push(chunk),
            Parser::JsonArray(parser) => parser.push(chunk),
        }
    }

    fn parse_finish(&mut self) -> Result<Vec<GenerateContentResponse>, ChannelError> {
        match &mut self.parser {
            Parser::Sse(parser) => parser.finish(),
            Parser::JsonArray(parser) => parser.finish(),
        }
    }

    fn observe(&mut self, chunks: Vec<GenerateContentResponse>) -> Result<(), ChannelError> {
        for chunk in chunks {
            if let Some(metadata) = chunk.usage_metadata.as_ref() {
                if self.response_tier.is_none() {
                    self.response_tier = metadata
                        .service_tier
                        .as_ref()
                        .and_then(super::usage::tier_name);
                }
                // Code Assist can attach prompt-only usage to early thinking
                // and tool-call frames. Later frames carry cumulative output
                // counts, which remain subject to the strict usage checks.
                if metadata.prompt_token_count.is_none()
                    || metadata.candidates_token_count.is_none()
                {
                    self.terminal.observe(&chunk)?;
                    continue;
                }
                let mut usage = super::usage::normalize(metadata)
                    .map_err(|error| ChannelError::Decode(format!("Gemini usage: {error}")))?;
                if let Some(tier) = self.response_tier.as_ref() {
                    usage.dimensions.insert("service_tier".into(), tier.clone());
                }
                self.usage = Some(usage);
            }
            self.terminal.observe(&chunk)?;
        }
        Ok(())
    }
}

impl StreamDecoder for GeminiStreamDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let parsed = self.parse(&chunk)?;
        self.observe(parsed)?;
        if chunk.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![Frame(chunk)])
        }
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        if end == StreamEnd::Interrupted {
            return Ok(StreamTail {
                frames: Vec::new(),
                usage: self.usage.take(),
                actual_service_tier: self.response_tier.take(),
            });
        }
        let parsed = self.parse_finish()?;
        self.observe(parsed)?;
        if !self.terminal.is_complete() {
            return Err(ChannelError::Decode(
                "Gemini stream ended without terminal candidate or block reason".into(),
            ));
        }
        Ok(StreamTail {
            frames: Vec::new(),
            usage: self.usage.take(),
            actual_service_tier: self.response_tier.take(),
        })
    }
}

#[derive(Default)]
struct Terminal {
    candidates: BTreeMap<i32, bool>,
    blocked: bool,
    response_id: Option<String>,
    model_version: Option<String>,
}

impl Terminal {
    fn observe(&mut self, chunk: &GenerateContentResponse) -> Result<(), ChannelError> {
        set_identity(
            &mut self.response_id,
            chunk.response_id.as_ref(),
            "responseId",
        )?;
        set_identity(
            &mut self.model_version,
            chunk.model_version.as_ref(),
            "modelVersion",
        )?;
        for (fallback, candidate) in chunk.candidates.iter().enumerate() {
            let index = match candidate.index {
                Some(index) if index >= 0 => index,
                Some(_) => {
                    return Err(ChannelError::Decode(
                        "Gemini candidate index is negative".into(),
                    ));
                }
                None => i32::try_from(fallback).map_err(|_| {
                    ChannelError::Decode("Gemini candidate index exceeds i32".into())
                })?,
            };
            if self.candidates.get(&index).copied() == Some(true) {
                return Err(ChannelError::Decode(
                    "Gemini candidate data followed finishReason".into(),
                ));
            }
            if matches!(
                candidate.finish_reason.as_ref(),
                Some(FinishReason::Known(
                    FinishReasonKnown::FinishReasonUnspecified
                ))
            ) {
                return Err(ChannelError::Decode(
                    "Gemini candidate finishReason is unspecified".into(),
                ));
            }
            self.candidates
                .insert(index, candidate.finish_reason.is_some());
        }
        let block_reason = chunk
            .prompt_feedback
            .as_ref()
            .and_then(|feedback| feedback.block_reason.as_ref());
        if matches!(
            block_reason,
            Some(BlockReason::Known(BlockReasonKnown::BlockReasonUnspecified))
        ) {
            return Err(ChannelError::Decode(
                "Gemini prompt blockReason is unspecified".into(),
            ));
        }
        if block_reason.is_some() {
            self.blocked = true;
        }
        Ok(())
    }

    fn is_complete(&self) -> bool {
        (!self.candidates.is_empty() && self.candidates.values().all(|finished| *finished))
            || (self.candidates.is_empty() && self.blocked)
    }
}

fn set_identity(
    target: &mut Option<String>,
    update: Option<&String>,
    field: &'static str,
) -> Result<(), ChannelError> {
    if let Some(update) = update {
        if target.as_ref().is_some_and(|current| current != update) {
            return Err(ChannelError::Decode(format!(
                "Gemini {field} changed during the stream"
            )));
        }
        *target = Some(update.clone());
    }
    Ok(())
}
