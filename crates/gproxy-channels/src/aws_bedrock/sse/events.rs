use std::collections::BTreeMap;

use gproxy_channel_api::{ChannelError, Frame};
use gproxy_protocol::aws::{
    ConverseStreamEvent, ServiceTier, StopReason, StreamException, TokenUsage,
};

use super::wire;

#[derive(Default)]
pub(super) struct State {
    pub(super) started: bool,
    pub(super) message_stopped: bool,
    pub(super) metadata_seen: bool,
    pub(super) terminal: bool,
    pub(super) normalized: Option<gproxy_channel_api::NormalizedUsage>,
    pub(super) blocks: BTreeMap<u64, ActiveBlock>,
    stop_reason: Option<StopReason>,
    tokens: Option<TokenUsage>,
    service_tier: Option<ServiceTier>,
}

#[derive(Clone, Copy)]
pub(super) enum ActiveBlock {
    Text,
    Thinking,
    Tool,
}

impl State {
    pub(super) fn handle(
        &mut self,
        event: ConverseStreamEvent,
    ) -> Result<Vec<Frame>, ChannelError> {
        match event {
            ConverseStreamEvent::MessageStart(_) => self.start(),
            ConverseStreamEvent::ContentBlockStart(event) => {
                self.block_start(event.content_block_index, event.start)
            }
            ConverseStreamEvent::ContentBlockDelta(event) => {
                self.block_delta(event.content_block_index, event.delta)
            }
            ConverseStreamEvent::ContentBlockStop(event) => {
                self.block_stop(event.content_block_index)
            }
            ConverseStreamEvent::MessageStop(event) => self.message_stop(event.stop_reason),
            ConverseStreamEvent::Metadata(event) => self.metadata(event.usage, event.service_tier),
            ConverseStreamEvent::InternalServerException(error) => {
                self.exception("internal_server_error", error)
            }
            ConverseStreamEvent::ValidationException(error) => {
                self.exception("validation_error", error)
            }
            ConverseStreamEvent::ThrottlingException(error) => {
                self.exception("rate_limit_error", error)
            }
            ConverseStreamEvent::ServiceUnavailableException(error) => {
                self.exception("service_unavailable", error)
            }
            ConverseStreamEvent::ModelStreamErrorException(error) => {
                self.terminal = true;
                Ok(vec![wire::error(
                    "model_stream_error",
                    error
                        .message
                        .as_deref()
                        .or(error.original_message.as_deref())
                        .unwrap_or("AWS Bedrock model stream failed"),
                )?])
            }
            ConverseStreamEvent::Unknown { event_type, .. } => Err(decode(format!(
                "unsupported Bedrock stream event {event_type}"
            ))),
        }
    }

    fn start(&mut self) -> Result<Vec<Frame>, ChannelError> {
        if self.started {
            return Err(decode("duplicate messageStart"));
        }
        self.started = true;
        Ok(vec![wire::message_start()?])
    }

    pub(super) fn ensure_started(&mut self, output: &mut Vec<Frame>) -> Result<(), ChannelError> {
        if !self.started {
            output.extend(self.start()?);
        }
        Ok(())
    }

    fn message_stop(&mut self, reason: StopReason) -> Result<Vec<Frame>, ChannelError> {
        if !self.blocks.is_empty() {
            return Err(decode("messageStop arrived with open content blocks"));
        }
        if self.message_stopped {
            return Err(decode("duplicate messageStop"));
        }
        self.message_stopped = true;
        self.stop_reason = Some(reason);
        self.maybe_finish()
    }

    fn metadata(
        &mut self,
        tokens: TokenUsage,
        tier: Option<ServiceTier>,
    ) -> Result<Vec<Frame>, ChannelError> {
        if self.metadata_seen {
            return Err(decode("duplicate metadata event"));
        }
        self.metadata_seen = true;
        self.normalized = Some(super::super::usage::from_tokens(&tokens, tier.as_ref()));
        self.tokens = Some(tokens);
        self.service_tier = tier;
        self.maybe_finish()
    }

    fn maybe_finish(&mut self) -> Result<Vec<Frame>, ChannelError> {
        if !self.message_stopped || !self.metadata_seen {
            return Ok(Vec::new());
        }
        self.terminal = true;
        wire::message_end(
            super::finish::stop_reason(self.stop_reason.take().expect("message stop was stored")),
            super::finish::claude_usage(
                self.tokens.as_ref().expect("metadata tokens were stored"),
                self.service_tier.as_ref(),
            ),
        )
    }

    fn exception(
        &mut self,
        kind: &str,
        error: StreamException,
    ) -> Result<Vec<Frame>, ChannelError> {
        self.terminal = true;
        Ok(vec![wire::error(
            kind,
            error
                .message
                .as_deref()
                .unwrap_or("AWS Bedrock stream failed"),
        )?])
    }
}

pub(super) fn decode(message: impl Into<String>) -> ChannelError {
    ChannelError::Decode(format!("Bedrock stream: {}", message.into()))
}
