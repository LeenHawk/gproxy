use gproxy_channel_api::ChannelError;

use crate::api::Core;
use crate::control::Target;
use crate::error::CoreError;
use crate::host::Host;

pub(super) async fn result<T, H: Host>(
    core: &Core<H>,
    target: &Target,
    version: u64,
    result: Result<T, ChannelError>,
) -> Result<T, CoreError> {
    match result {
        Ok(value) => Ok(value),
        Err(error @ ChannelError::Secret(_)) => {
            crate::funnel::health::dead(
                core.host.as_ref(),
                target,
                Some(version),
                "credential rejected during request preparation",
            )
            .await;
            Err(error.into())
        }
        Err(error @ ChannelError::Refresh(_)) => {
            crate::funnel::health::degraded(
                core.host.as_ref(),
                target,
                Some(version),
                None,
                "request preparation refresh failed",
            )
            .await;
            Err(error.into())
        }
        Err(error) => Err(error.into()),
    }
}
