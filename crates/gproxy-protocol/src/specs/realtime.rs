use crate::operation::{OperationKind, WireFamily};
use crate::spec::Seg::Lit;
use crate::spec::{
    Affinity, Ingress, OperationSpec, PathPattern, SettleMode, StreamDetect, StreamFraming,
};

use super::{FAM_OAI, GET, NEVER, POST, ing};

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

pub(super) const CONNECT_REALTIME: OperationSpec = OperationSpec {
    ingress: &[Ingress {
        method: GET,
        pattern: PathPattern(&[Lit("v1"), Lit("realtime")]),
        kind: OperationKind::Family(WireFamily::OpenAi),
        stream: StreamDetect::Never,
        framing: StreamFraming::WebSocket,
        upgrade: true,
    }],
    settle: SettleMode::OnSessionEnd,
    affinity: Affinity::Session,
};
