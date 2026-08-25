use gproxy_channel_api::{Channel, OperationDriver};

use super::super::AdmissionCtx;
use crate::api::Core;
use crate::control::Target;
use crate::error::CoreError;
use crate::host::Host;

pub(super) fn validate<H: Host>(
    core: &Core<H>,
    channel: &dyn Channel,
    target: &Target,
    admission: AdmissionCtx,
    driver: &dyn OperationDriver,
) -> Result<(), CoreError> {
    let owner_user_id = admission.owner_user_id.ok_or(CoreError::Unsupported)?;
    if let Some(id) = driver.claim_id() {
        let key = crate::continuation::ContinuationKey {
            channel: channel.descriptor().id,
            provider_id: target.provider.id,
            owner_user_id,
            id: id.into(),
        };
        let meta = core
            .host
            .continuations()
            .ok_or(CoreError::Unsupported)?
            .peek(&key)?
            .ok_or(CoreError::Unsupported)?;
        if meta.credential != target.credential {
            return Err(CoreError::Unsupported);
        }
    }
    Ok(())
}
