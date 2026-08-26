use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec, SettleMode};

use super::{FAM_OAI, NEVER, POST, ing};

// The SDP answer has no usage. Realtime usage arrives through the trusted
// server-side observer for the call.
pub(super) const REALTIME_CALL: OperationSpec = OperationSpec {
    ingress: &[ing(
        POST,
        &[Lit("v1"), Lit("realtime"), Lit("calls")],
        FAM_OAI,
        NEVER,
    )],
    settle: SettleMode::OnSessionEnd,
    affinity: Affinity::Resource("realtime_call"),
};
