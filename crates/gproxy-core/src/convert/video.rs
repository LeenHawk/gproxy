//! video conversion family. Not converted yet: the driver reports the pair as
//! unsupported so the attempt loop returns a clear error instead of guessing.

use super::{Call, Converted};
use gproxy_protocol::transform::TransformError;
use gproxy_seaorm::BatchConnectionTrait;

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    Err(TransformError::unsupported(
        "video",
        format!(
            "{:?} from {:?} to {:?} is not converted yet",
            call.client.operation, call.client.dialect, call.target
        ),
    ))
}
