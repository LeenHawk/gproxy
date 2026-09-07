use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::future::Either;
use gproxy_channel_api::CallerIdentity;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind, StreamFraming};

use super::AdmittedRequest;
use super::request::Classified;
use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx, ResponseBody};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::host::Host;

mod handoff;

const KEEPALIVE: Duration = Duration::from_secs(15);

pub(super) struct Synthetic {
    kind: ContentGenerationKind,
    framing: StreamFraming,
    control: Arc<dyn ControlPlane>,
}

/// A streaming client whose first usable target is routed onto the buffered
/// sibling waits for one whole upstream object. A host that can detach the
/// upstream work opens the stream at once and keeps it warm meanwhile; one that
/// cannot keeps the inline path, where the funnel synthesizes at the end.
pub(super) fn plan<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    plan: &Plan,
    classified: &Classified,
) -> Option<Synthetic> {
    if !classified.stream || classified.framing == StreamFraming::WebSocket {
        return None;
    }
    let OperationKind::ContentGeneration(kind) = classified.key.kind() else {
        return None;
    };
    core.host.spawner()?;
    let support = plan.targets.iter().find_map(|target| {
        crate::attempt::support(core, target, classified.key)
            .ok()
            .flatten()
    })?;
    if support.source.operation() != Operation::StreamGenerateContent
        || support.target.operation() != Operation::GenerateContent
    {
        return None;
    }
    Some(Synthetic {
        kind,
        framing: classified.framing,
        control: control.shared()?,
    })
}

pub(super) fn run<H: Host>(
    core: &Core<H>,
    synthetic: Synthetic,
    ctx: RequestCtx,
    plan: Plan,
    request: AdmittedRequest,
    identity: CallerIdentity,
) -> ExecOutcome {
    let Synthetic {
        kind,
        framing,
        control,
    } = synthetic;
    let (sender, receiver) = handoff::channel();
    let task_core = core.clone();
    core.host
        .spawner()
        .expect("synthetic stream was planned with a spawner")
        .spawn(Box::pin(async move {
            let work = super::upstream(&task_core, control.as_ref(), ctx, plan, request, identity);
            let mut work = std::pin::pin!(work);
            let result = loop {
                match futures_util::future::select(work.as_mut(), task_core.host.wait(KEEPALIVE))
                    .await
                {
                    Either::Left((result, _)) => break result,
                    Either::Right(((), _)) => {
                        if let Some(frame) = gproxy_transform::synthesize_keepalive(kind, framing) {
                            sender.push(frame);
                        }
                    }
                }
            };
            relay(kind, framing, &sender, result).await;
            sender.close();
        }));
    crate::funnel::synth::opened(framing, Box::pin(receiver))
}

async fn relay(
    kind: ContentGenerationKind,
    framing: StreamFraming,
    sender: &handoff::Sender,
    result: Result<ExecOutcome, CoreError>,
) {
    let message = match result {
        Ok(outcome) if outcome.status.is_success() => match outcome.body {
            ResponseBody::Stream(mut stream) => {
                while let Some(Ok(frame)) = stream.next().await {
                    sender.push(frame);
                }
                return;
            }
            ResponseBody::Full(body) => {
                sender.push(body);
                return;
            }
            ResponseBody::WebSocket(_) => "upstream answered with a websocket".to_owned(),
        },
        Ok(outcome) => format!("upstream request failed with status {}", outcome.status),
        Err(error) => format!("upstream request failed with status {}", error.status()),
    };
    if let Ok(frames) = gproxy_transform::synthesize_error(kind, framing, &message) {
        for frame in frames {
            sender.push(frame);
        }
    }
}
