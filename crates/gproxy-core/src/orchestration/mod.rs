mod call;
mod cleanup;
mod final_call;
mod stream;

use std::collections::VecDeque;

use gproxy_channel_api::{DriverInput, OperationDriver, OperationStep};

use crate::api::Core;
use crate::boundary::ByteStream;
use crate::continuation::{Continuation, ContinuationKey};
use crate::error::CoreError;
use crate::funnel::FunnelCtx;
use crate::host::{Host, UpstreamTransport};

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    channel: &'static str,
    mut driver: Box<dyn OperationDriver>,
    facts: &mut FunnelCtx,
) -> Result<http::Response<ByteStream>, CoreError> {
    let mut claimed = None;
    let result = drive(core, channel, driver.as_mut(), facts, &mut claimed).await;
    if result.is_err() {
        if let Some(claimed) = claimed {
            cleanup::spawn_continuation(core.host.clone(), claimed);
        } else if let Some(cleanup) = driver.abort() {
            cleanup::spawn_request(
                core.host.clone(),
                facts.target.clone(),
                format!("{}:cleanup", facts.request_id),
                cleanup,
            );
        }
    }
    result
}

async fn drive<H: Host>(
    core: &Core<H>,
    channel: &'static str,
    driver: &mut dyn OperationDriver,
    facts: &mut FunnelCtx,
    claimed: &mut Option<Continuation>,
) -> Result<http::Response<ByteStream>, CoreError> {
    let mut input = None;
    let mut sequence = 0_u64;
    loop {
        match driver.next(input.take())? {
            OperationStep::Call { label, request } => {
                let request_id = format!("{}:operation:{label}:{sequence}", facts.request_id);
                sequence = sequence.saturating_add(1);
                let response = call::run(
                    core.host.clone(),
                    Some(
                        core.channels
                            .get(channel)
                            .expect("orchestrating channel remains registered"),
                    ),
                    facts.target.clone(),
                    facts.credential_version,
                    request_id,
                    label,
                    *request,
                )
                .await?;
                input = Some(DriverInput::Response(response));
            }
            OperationStep::Claim { id } => {
                if claimed.is_some() {
                    return Err(CoreError::Internal("operation driver claimed twice".into()));
                }
                let key = scoped_key(facts, channel, id)?;
                let value = core
                    .host
                    .continuations()
                    .expect("continuation capability was checked")
                    .take(&key)?
                    .ok_or_else(|| CoreError::UnknownRoute("continuation expired".into()))?;
                if value.meta().credential != facts.target.credential {
                    return Err(CoreError::UnknownRoute(
                        "continuation credential is no longer eligible".into(),
                    ));
                }
                input = Some(DriverInput::Continuation(value.state.clone()));
                *claimed = Some(value);
            }
            OperationStep::Final {
                label,
                request,
                stream: codec,
                cleanup,
                ttl_secs,
            } => {
                if claimed.is_some() {
                    return Err(CoreError::Internal(
                        "claimed continuation used by a new final call".into(),
                    ));
                }
                let mut request = *request;
                crate::fingerprint::apply_prepared(&mut request, &facts.target.provider)?;
                let url = request.request.uri().to_string();
                facts.upstream_url = Some(url.clone());
                facts.request_body = request.request.body().clone();
                facts.surface_label = Some(label);
                let response = match core.host.transport().send(request.request).await {
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
                        crate::funnel::error::terminal_transport(core.host.as_ref(), facts, &error)
                            .await;
                        return Err(error.into());
                    }
                };
                return final_call::wrap_response(
                    core,
                    channel,
                    facts,
                    response,
                    final_call::FinalStream {
                        pending: VecDeque::new(),
                        codec,
                        cleanup: *cleanup,
                        ttl_secs,
                        url,
                    },
                );
            }
            OperationStep::Resume {
                stream: codec,
                cleanup,
                ttl_secs,
            } => {
                let value = claimed.take().ok_or_else(|| {
                    CoreError::Internal("resume step has no claimed continuation".into())
                })?;
                facts.upstream_url = Some(value.upstream_url.clone());
                facts.surface_label = Some("resume");
                let scope = final_call::stream_scope(
                    channel,
                    facts,
                    value.status,
                    value.headers.clone(),
                    value.upstream_url,
                    *cleanup,
                    ttl_secs,
                )?;
                let body =
                    stream::wrap(core.host.clone(), value.stream, value.pending, codec, scope);
                let mut response = http::Response::new(body);
                *response.status_mut() = value.status;
                *response.headers_mut() = value.headers;
                return Ok(response);
            }
        }
    }
}

fn scoped_key(
    facts: &FunnelCtx,
    channel: &'static str,
    id: String,
) -> Result<ContinuationKey, CoreError> {
    Ok(ContinuationKey {
        channel,
        provider_id: facts.target.provider.id,
        owner_user_id: facts.owner_user_id.ok_or(CoreError::Unsupported)?,
        id,
    })
}
