use gproxy_channel_api::{ChannelError, DriverInput, OperationDriver, OperationStep, PrepareCtx};
use serde_json::Value;

use crate::claudeweb::stream::{Codec, SessionState};

pub(super) struct ResumeTurn {
    requests: super::super::prepare::Requests,
    results: Vec<Value>,
    claim: String,
    state: Option<SessionState>,
    index: usize,
    claimed: bool,
}

impl ResumeTurn {
    pub(super) fn new(ctx: PrepareCtx<'_>, results: Vec<Value>) -> Result<Self, ChannelError> {
        let claim = results
            .first()
            .and_then(|result| result.get("tool_use_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| ChannelError::Prepare("tool_result id missing".into()))?
            .to_owned();
        if results
            .iter()
            .any(|result| result.get("tool_use_id").and_then(Value::as_str) != Some(claim.as_str()))
        {
            return Err(ChannelError::Prepare(
                "tool results refer to different continuations".into(),
            ));
        }
        let headers = crate::policy::CLAUDE_WEB
            .filter_request_headers(ctx.headers, ctx.provider_settings)
            .map_err(ChannelError::Prepare)?;
        Ok(Self {
            requests: super::super::prepare::Requests::new(
                ctx.secret,
                ctx.provider_settings,
                headers,
            )?,
            results,
            claim,
            state: None,
            index: 0,
            claimed: false,
        })
    }

    fn post(&self) -> Result<OperationStep, ChannelError> {
        let state = self.state.as_ref().expect("continuation state is present");
        Ok(OperationStep::Call {
            label: "tool_result",
            request: Box::new(
                self.requests
                    .tool_result(&state.conversation, &self.results[self.index])?,
            ),
        })
    }
}

impl OperationDriver for ResumeTurn {
    fn claim_id(&self) -> Option<&str> {
        Some(&self.claim)
    }

    fn next(&mut self, input: Option<DriverInput>) -> Result<OperationStep, ChannelError> {
        if !self.claimed {
            self.claimed = true;
            return Ok(OperationStep::Claim {
                id: self.claim.clone(),
            });
        }
        if self.state.is_none() {
            let Some(DriverInput::Continuation(value)) = input else {
                return Err(ChannelError::Prepare("continuation state missing".into()));
            };
            let mut state: SessionState = serde_json::from_value(value)
                .map_err(|error| ChannelError::Prepare(format!("continuation state: {error}")))?;
            state.input_tokens = state.input_tokens.saturating_add(
                self.results
                    .iter()
                    .map(|value| super::input_tokens(&bytes::Bytes::from(value.to_string())))
                    .sum::<u64>(),
            );
            self.state = Some(state);
            return self.post();
        }
        super::response(input)?;
        self.index += 1;
        if self.index < self.results.len() {
            return self.post();
        }
        let state = self.state.clone().expect("continuation state is present");
        Ok(OperationStep::Resume {
            stream: Box::new(Codec::new(state.clone(), true)),
            cleanup: Box::new(self.requests.cleanup(&state.conversation)?),
            ttl_secs: 10 * 60,
        })
    }
}
