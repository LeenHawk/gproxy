use std::time::Duration;

use gproxy_channel_api::PreparedRequest;

use crate::Shared;
use crate::continuation::{Continuation, ContinuationKey};
use crate::control::Target;
use crate::host::Host;

pub(super) fn spawn_request<H: Host>(
    host: Shared<H>,
    target: Target,
    request_id: String,
    prepared: PreparedRequest,
) {
    let task_host = host.clone();
    let Some(spawner) = host.spawner() else {
        return;
    };
    spawner.spawn(Box::pin(async move {
        if let Err(error) = super::call::run(
            task_host, None, target, None, request_id, "cleanup", prepared,
        )
        .await
        {
            tracing::warn!(error = %error, "operation cleanup failed");
        }
    }));
}

pub(super) fn spawn_continuation<H: Host>(host: Shared<H>, continuation: Continuation) {
    let request_id = format!("{}:cleanup", continuation.generation);
    spawn_request(host, continuation.target, request_id, continuation.cleanup);
}

pub(super) fn schedule_expiry<H: Host>(
    host: Shared<H>,
    key: ContinuationKey,
    generation: String,
    ttl_secs: u64,
) {
    let task_host = host.clone();
    let Some(spawner) = host.spawner() else {
        return;
    };
    spawner.spawn(Box::pin(async move {
        task_host.wait(Duration::from_secs(ttl_secs)).await;
        let result = task_host
            .continuations()
            .expect("continuation capability was checked")
            .take_generation(&key, &generation);
        match result {
            Ok(Some(expired)) => spawn_continuation(task_host, expired),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                error = %error,
                "operation continuation expiry cleanup failed"
            ),
        }
    }));
}
