use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelError, Disposition, Frame, NormalizedUsage,
    PrepareCtx, PreparedRequest, ResponseView, SimpleHttp, StreamDecoder, StreamTail,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};

use super::memory::MemoryHost;

const KEY: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::OpenAiResponses,
);
const STREAM_KEY: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::OpenAiResponses,
);
static SUPPORTS: [OperationKey; 2] = [KEY, STREAM_KEY];
static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "memory",
    display_name: "Memory",
    supports: &SUPPORTS,
};

impl Channel for MemoryHost {
    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let token = ctx.secret["access_token"]
            .as_str()
            .ok_or_else(|| ChannelError::Secret("access_token missing".into()))?;
        let request = http::Request::builder()
            .method(ctx.method)
            .uri("https://upstream.test/v1/responses")
            .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
            .body(ctx.body.clone())
            .map_err(|error| ChannelError::Prepare(error.to_string()))?;
        Ok(PreparedRequest {
            request,
            websocket: false,
        })
    }

    fn classify(&self, response: ResponseView<'_>) -> Disposition {
        if response.status.is_success() {
            Disposition::Success
        } else {
            Disposition::Terminal
        }
    }

    fn extract_usage(&self, _: OperationKey, body: &[u8]) -> Option<NormalizedUsage> {
        serde_json::from_slice::<serde_json::Value>(body)
            .ok()?
            .get("usage")?;
        Some(NormalizedUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        })
    }

    fn stream_decoder(&self, _: OperationKey) -> Option<Box<dyn StreamDecoder>> {
        Some(Box::new(self.clone()))
    }

    fn refresh_due(&self, secret: &serde_json::Value) -> Option<i64> {
        secret.get("expires_at")?.as_i64()
    }

    fn refresh<'a>(
        &'a self,
        _: &'a serde_json::Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<serde_json::Value, ChannelError>>> {
        let request = http::Request::post("https://auth.test/refresh")
            .body(Bytes::new())
            .expect("refresh request");
        let send = http.send(request);
        Some(Box::pin(async move {
            let response = send.await?;
            serde_json::from_slice(response.body())
                .map_err(|error| ChannelError::Refresh(error.to_string()))
        }))
    }
}

impl StreamDecoder for MemoryHost {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, ChannelError> {
        Ok(vec![Frame(Bytes::copy_from_slice(chunk))])
    }

    fn finish(&mut self) -> StreamTail {
        StreamTail {
            usage: Some(NormalizedUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            }),
        }
    }
}
