use bytes::Bytes;
use gproxy_channel_api::{
    Channel, Disposition, OperationDriver, ResponseView, StreamCtx, StreamDecoder, TransportError,
};

use crate::api::Core;
use crate::boundary::{ByteStream, ExecOutcome};
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::host::{Host, UpstreamTransport};

pub(crate) mod body;
mod prepare;
mod transform;

pub(crate) use prepare::{native_support, prepare, support};

enum Egress {
    Http(Box<http::Request<Bytes>>),
    Orchestrated(Box<dyn OperationDriver>),
}

pub(crate) struct Prepared {
    channel: &'static str,
    egress: Egress,
    stream: bool,
    downstream_stream: bool,
    facts: FunnelCtx,
}

pub(crate) struct Completed {
    channel: &'static str,
    pub facts: FunnelCtx,
    pub disposition: Disposition,
    body: AttemptBody,
}

#[derive(Clone, Copy)]
pub(crate) struct AdmissionCtx {
    pub admitted: bool,
    pub owner_user_id: Option<i64>,
}

enum AttemptBody {
    Buffered(funnel::BufferedRelay),
    Streaming(http::Response<ByteStream>, Option<Box<dyn StreamDecoder>>),
}

pub(crate) enum Failure {
    Transport {
        facts: FunnelCtx,
        error: TransportError,
    },
    Interrupted {
        channel: &'static str,
        facts: FunnelCtx,
        status: http::StatusCode,
        headers: http::HeaderMap,
        body: Bytes,
        error: TransportError,
    },
    Committed {
        error: CoreError,
    },
}

pub(crate) async fn send<H: Host>(
    core: &Core<H>,
    prepared: Prepared,
) -> Result<Completed, Failure> {
    let Prepared {
        channel,
        egress,
        stream,
        downstream_stream,
        mut facts,
    } = prepared;
    let committed = matches!(&egress, Egress::Orchestrated(_));
    let response = match egress {
        Egress::Http(request) => match core.host.transport().send(*request).await {
            Ok(response) => response,
            Err(error) => {
                crate::funnel::health::degraded(
                    core.host.as_ref(),
                    &facts.target,
                    facts.credential_version,
                    None,
                    "upstream transport failed",
                )
                .await;
                return Err(Failure::Transport { facts, error });
            }
        },
        Egress::Orchestrated(driver) => {
            match crate::orchestration::run(core, channel, driver, &mut facts).await {
                Ok(response) => response,
                Err(error) => return Err(Failure::Committed { error }),
            }
        }
    };
    let channel = core
        .channels
        .get(channel)
        .expect("prepared attempt channel remains registered");
    if stream && response.status().is_success() {
        let disposition = committed_disposition(classify(channel, &response, &[]), committed);
        crate::funnel::health::response(
            core.host.as_ref(),
            &facts.target,
            facts.credential_version,
            disposition,
            response.status(),
        )
        .await;
        let key = facts.key.expect("operation attempt has an upstream key");
        let mut decoder = channel.stream_decoder(StreamCtx {
            key,
            framing: facts.target_framing,
            request_body: &facts.request_body,
            response_headers: response.headers(),
        });
        let source = facts
            .source_key
            .expect("operation attempt has a source key");
        if source.kind != key.kind || facts.source_framing != facts.target_framing {
            let source_framing = if downstream_stream {
                facts.source_framing
            } else {
                gproxy_protocol::StreamFraming::Sse
            };
            decoder = Some(Box::new(transform::TransformDecoder::new(
                source,
                key,
                source_framing,
                facts.target_framing,
                decoder,
            )));
        }
        if !downstream_stream {
            let gproxy_protocol::OperationKind::ContentGeneration(kind) = source.kind else {
                let (parts, _) = response.into_parts();
                return Err(Failure::Interrupted {
                    channel: channel.descriptor().id,
                    facts,
                    status: parts.status,
                    headers: parts.headers,
                    body: Bytes::new(),
                    error: TransportError::Interrupted(
                        "buffered stream source is not content generation".into(),
                    ),
                });
            };
            return match body::collect_stream(response, decoder, kind).await {
                Ok(collected) => Ok(Completed {
                    channel: channel.descriptor().id,
                    facts,
                    disposition,
                    body: AttemptBody::Buffered(funnel::BufferedRelay {
                        response: collected.response,
                        usage: collected.usage,
                        actual_service_tier: collected.actual_service_tier,
                        capture_body: Some(collected.capture_body),
                        outward_ready: true,
                    }),
                }),
                Err(failure) => {
                    crate::funnel::health::degraded(
                        core.host.as_ref(),
                        &facts.target,
                        facts.credential_version,
                        Some(failure.status),
                        "upstream response interrupted",
                    )
                    .await;
                    Err(Failure::Interrupted {
                        channel: channel.descriptor().id,
                        facts,
                        status: failure.status,
                        headers: failure.headers,
                        body: failure.body,
                        error: failure.error,
                    })
                }
            };
        }
        return Ok(Completed {
            channel: channel.descriptor().id,
            facts,
            disposition,
            body: AttemptBody::Streaming(response, decoder),
        });
    }
    let response = match body::collect(response).await {
        Ok(response) => response,
        Err(failure) => {
            crate::funnel::health::degraded(
                core.host.as_ref(),
                &facts.target,
                facts.credential_version,
                Some(failure.status),
                "upstream response interrupted",
            )
            .await;
            return Err(Failure::Interrupted {
                channel: channel.descriptor().id,
                facts,
                status: failure.status,
                headers: failure.headers,
                body: failure.body,
                error: failure.error,
            });
        }
    };
    let disposition =
        committed_disposition(classify(channel, &response, response.body()), committed);
    crate::funnel::health::response(
        core.host.as_ref(),
        &facts.target,
        facts.credential_version,
        disposition,
        response.status(),
    )
    .await;
    Ok(Completed {
        channel: channel.descriptor().id,
        facts,
        disposition,
        body: AttemptBody::Buffered(funnel::BufferedRelay::native(response)),
    })
}

fn committed_disposition(disposition: Disposition, committed: bool) -> Disposition {
    if committed && disposition.should_failover() {
        Disposition::Terminal
    } else {
        disposition
    }
}

pub(crate) async fn finish<H: Host>(
    core: &Core<H>,
    control: &impl crate::control::ControlPlane,
    completed: Completed,
) -> ExecOutcome {
    let channel = core
        .channels
        .shared(completed.channel)
        .expect("completed attempt channel remains registered");
    match completed.body {
        AttemptBody::Buffered(response) => {
            funnel::buffered(
                core.host.clone(),
                channel.as_ref(),
                Some(control as &dyn crate::control::ControlPlane),
                Some(channel.clone()),
                completed.facts,
                response,
                completed.disposition,
            )
            .await
        }
        AttemptBody::Streaming(response, decoder) => funnel::streaming(
            core.host.clone(),
            completed.facts,
            response,
            completed.disposition,
            decoder,
        ),
    }
}

pub(crate) fn discard(completed: Completed) -> (FunnelCtx, http::StatusCode, Option<Bytes>) {
    match completed.body {
        AttemptBody::Buffered(relay) => {
            let (parts, body) = relay.response.into_parts();
            (completed.facts, parts.status, Some(body))
        }
        AttemptBody::Streaming(response, _) => (completed.facts, response.status(), None),
    }
}

fn classify<B>(channel: &dyn Channel, response: &http::Response<B>, body: &[u8]) -> Disposition {
    channel.classify(ResponseView {
        status: response.status(),
        headers: response.headers(),
        body,
    })
}
