mod new_turn;
mod resume_turn;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, OperationDriver, PrepareCtx};

pub(super) fn driver(
    ctx: PrepareCtx<'_>,
) -> Result<Option<Box<dyn OperationDriver>>, ChannelError> {
    let request = super::request::parse(ctx.body)?;
    let results = super::request::tool_results(&request);
    if results.is_empty() {
        new_turn::NewTurn::new(ctx, &request)
            .map(|driver| Some(Box::new(driver) as Box<dyn OperationDriver>))
    } else {
        resume_turn::ResumeTurn::new(ctx, results)
            .map(|driver| Some(Box::new(driver) as Box<dyn OperationDriver>))
    }
}

fn response(
    input: Option<gproxy_channel_api::DriverInput>,
) -> Result<gproxy_channel_api::StepResponse, ChannelError> {
    match input {
        Some(gproxy_channel_api::DriverInput::Response(response))
            if response.status.is_success() =>
        {
            Ok(response)
        }
        Some(gproxy_channel_api::DriverInput::Response(response)) => Err(ChannelError::Prepare(
            format!("ClaudeWeb step returned {}", response.status),
        )),
        _ => Err(ChannelError::Prepare(
            "ClaudeWeb driver input mismatch".into(),
        )),
    }
}

fn input_tokens(body: &Bytes) -> u64 {
    u64::try_from(String::from_utf8_lossy(body).chars().count())
        .unwrap_or(u64::MAX)
        .div_ceil(4)
}
