use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec, SettleMode};

use super::{FAM_OAI, NEVER, POST, ing};

// The SDP answer has no usage. Realtime usage arrives on the session event
// stream, or not at all when WebRTC media bypasses the proxy.
pub(super) const REALTIME_CALL: OperationSpec = OperationSpec {
    ingress: &[ing(
        POST,
        &[Lit("v1"), Lit("realtime"), Lit("calls")],
        FAM_OAI,
        NEVER,
    )],
    settle: SettleMode::Free,
    affinity: Affinity::Resource("realtime_call"),
};
