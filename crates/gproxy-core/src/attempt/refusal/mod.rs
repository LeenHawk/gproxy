mod buffered;
mod credit;
mod meter;
pub(crate) mod pin;
mod policy;
mod retry;
mod stream;
mod stream_events;
mod stream_retry;

use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx};

use crate::error::CoreError;

pub(crate) struct Wrapped {
    pub response: http::Response<crate::boundary::ByteStream>,
    pub decoder: Option<Box<dyn gproxy_channel_api::StreamDecoder>>,
    pub usage: Option<gproxy_channel_api::NormalizedUsage>,
}

pub(crate) async fn wrap<H: crate::host::Host>(
    core: &crate::api::Core<H>,
    facts: &crate::funnel::FunnelCtx,
    response: http::Response<crate::boundary::ByteStream>,
    replay: Replay,
    streaming: bool,
) -> Wrapped {
    let mut runner = retry::Runner::new(core, facts, replay);
    if streaming {
        let decoder = runner.meter.decoder();
        Wrapped {
            response: stream::wrap(runner, response),
            decoder: Some(decoder),
            usage: None,
        }
    } else {
        let response = buffered::run(&mut runner, response).await;
        Wrapped {
            response,
            decoder: None,
            usage: runner.meter.usage(),
        }
    }
}

pub(crate) struct Replay {
    policy: policy::Policy,
    body: Bytes,
    headers: http::HeaderMap,
    method: http::Method,
    path: String,
    query: Option<String>,
    session_id: Option<String>,
    pub(crate) budget: u32,
    pub(crate) pin_key: Option<String>,
    pub(crate) pinned: bool,
}

impl Replay {
    pub(crate) fn read(
        channel: &dyn Channel,
        ctx: &PrepareCtx<'_>,
    ) -> Result<Option<Self>, CoreError> {
        Ok(policy::Policy::read(channel, ctx)?.map(|policy| Self {
            policy,
            body: ctx.body.clone(),
            headers: ctx.headers.clone(),
            method: ctx.method.clone(),
            path: ctx.path.into(),
            query: ctx.query.map(str::to_owned),
            session_id: ctx.session_id.map(str::to_owned),
            budget: 4,
            pin_key: None,
            pinned: false,
        }))
    }

    pub(crate) fn capture(&mut self, request: &http::Request<Bytes>) {
        self.body = request.body().clone();
        self.headers = request.headers().clone();
    }

    pub(crate) fn prepare_body(&self, body: &Bytes) -> Bytes {
        if self.policy.capabilities.server_side && !self.pinned {
            return body.clone();
        }
        let mut body: serde_json::Value =
            serde_json::from_slice(body).expect("validated fallback body");
        body.as_object_mut()
            .expect("request object")
            .remove("fallbacks");
        Bytes::from(serde_json::to_vec(&body).expect("JSON serializes"))
    }

    pub(crate) fn prepare_headers(&self, headers: &mut http::HeaderMap) {
        if self.policy.capabilities.credit {
            let current = headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            let mut betas = current
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            if !betas
                .iter()
                .any(|value| value.starts_with("fallback-credit-"))
            {
                betas.push("fallback-credit-2026-07-01");
            }
            headers.insert(
                "anthropic-beta",
                betas
                    .join(",")
                    .parse()
                    .expect("existing beta values are valid"),
            );
        }
    }
}
