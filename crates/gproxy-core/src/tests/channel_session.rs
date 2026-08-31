use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, Frame, NormalizedUsage, PreparedRequest, PreparedSession, RealtimeMeter,
    SessionPrepareCtx, StreamDecoder, StreamEnd, StreamTail,
};

use super::memory::MemoryHost;

pub(super) fn prepare_test_session(
    ctx: SessionPrepareCtx<'_>,
) -> Result<PreparedSession, ChannelError> {
    let id = ctx
        .response_headers
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit('/').next())
        .ok_or_else(|| ChannelError::Observe("test call id missing".into()))?;
    let request = http::Request::get(format!("wss://upstream.test/session?call_id={id}"))
        .body(Bytes::new())
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    Ok(PreparedSession {
        id: id.into(),
        request: PreparedRequest {
            request,
            framing: None,
            websocket: true,
            profile: None,
        },
        meter: RealtimeMeter::new(ctx.request_body, ctx.upstream_model),
    })
}

impl StreamDecoder for MemoryHost {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        Ok(vec![Frame(chunk)])
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        let omit_usage = self.state.lock().expect("state lock").omit_usage;
        Ok(StreamTail {
            frames: (end == StreamEnd::Complete)
                .then_some(Frame(Bytes::from_static(b"tail")))
                .into_iter()
                .collect(),
            usage: (!omit_usage).then(|| NormalizedUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            }),
            actual_service_tier: None,
        })
    }
}
