mod framing;

use bytes::Bytes;
use gproxy_protocol::{OperationKey, StreamFraming};

use self::framing::{FrameDecoder, FrameEncoder};
use super::SseFrame;
use crate::TransformError;
use crate::registry::{self, TransformPair};

pub struct ResponseStream {
    decoder: FrameDecoder,
    converter: Box<dyn Converter>,
    encoder: FrameEncoder,
}

pub(crate) trait Converter: Send {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError>;
    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError>;
}

impl ResponseStream {
    pub fn new(source: OperationKey, target: OperationKey) -> Result<Self, TransformError> {
        Self::new_framed(source, target, StreamFraming::Sse, StreamFraming::Sse)
    }

    pub fn new_framed(
        source: OperationKey,
        target: OperationKey,
        source_framing: StreamFraming,
        target_framing: StreamFraming,
    ) -> Result<Self, TransformError> {
        let converter: Box<dyn Converter> = if source == target {
            Box::new(Passthrough)
        } else {
            let pair =
                registry::resolve(source, target).ok_or(TransformError::UnsupportedPair {
                    source_key: source,
                    target_key: target,
                })?;
            converter(pair, source, target)?
        };
        Ok(Self {
            decoder: FrameDecoder::new(target_framing)?,
            converter,
            encoder: FrameEncoder::new(source_framing)?,
        })
    }

    pub fn push(&mut self, chunk: Bytes) -> Result<Vec<Bytes>, TransformError> {
        let frames = self.decoder.push(&chunk)?;
        self.convert(frames)
    }

    pub fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        let frames = self.decoder.finish()?;
        let mut output = self.convert(frames)?;
        let terminal = self.converter.finish()?;
        output.extend(self.encoder.push(terminal)?);
        output.extend(self.encoder.finish()?);
        Ok(output)
    }

    fn convert(&mut self, frames: Vec<SseFrame>) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        for frame in frames {
            output.extend(self.encoder.push(self.converter.frame(frame)?)?);
        }
        Ok(output)
    }
}

struct Passthrough;

impl Converter for Passthrough {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        Ok(vec![SseFrame::encode(frame.event.as_deref(), &frame.data)])
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        Ok(Vec::new())
    }
}

fn converter(
    pair: TransformPair,
    source: OperationKey,
    target: OperationKey,
) -> Result<Box<dyn Converter>, TransformError> {
    Ok(match pair {
        TransformPair::ChatToClaude => {
            crate::generate_content::openai_chat_to_claude_messages::stream::converter()
        }
        TransformPair::ResponsesToClaude => {
            crate::generate_content::openai_responses_to_claude_messages::stream::converter()
        }
        TransformPair::ClaudeToChat => {
            crate::generate_content::claude_messages_to_openai_chat::stream::converter()
        }
        TransformPair::ClaudeToResponses => {
            crate::generate_content::claude_messages_to_openai_responses::stream::converter()
        }
        TransformPair::ClaudeToGemini => {
            crate::generate_content::gemini_generate_content_to_claude_messages::stream::converter()
        }
        TransformPair::GeminiToClaude => {
            crate::generate_content::claude_messages_to_gemini_generate_content::stream::converter()
        }
        TransformPair::GeminiToChat => {
            crate::generate_content::gemini_generate_content_to_openai_chat::stream::converter()
        }
        TransformPair::ChatToGemini => {
            crate::generate_content::openai_chat_to_gemini_generate_content::stream::converter()
        }
        TransformPair::GeminiToResponses => {
            crate::generate_content::gemini_generate_content_to_openai_responses::stream::converter(
            )
        }
        TransformPair::ResponsesToGemini => {
            crate::generate_content::openai_responses_to_gemini_generate_content::stream::converter(
            )
        }
        TransformPair::OpenAiChatToResponses => {
            crate::generate_content::openai_chat_to_openai_responses::stream::converter()
        }
        TransformPair::OpenAiResponsesToChat => {
            crate::generate_content::openai_responses_to_openai_chat::stream::converter()
        }
        TransformPair::OpenAiCreateImageToGemini => crate::images::stream::from_gemini(false),
        TransformPair::OpenAiEditImageToGemini => crate::images::stream::from_gemini(true),
        TransformPair::OpenAiCreateImageToResponses => crate::images::stream::from_responses(false),
        TransformPair::OpenAiEditImageToResponses => crate::images::stream::from_responses(true),
        _ => {
            return Err(TransformError::UnsupportedPair {
                source_key: source,
                target_key: target,
            });
        }
    })
}
