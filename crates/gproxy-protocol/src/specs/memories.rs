use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec};

use super::{FAM_OAI, NEVER, POST, billed, ing};

pub(super) const SUMMARIZE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("memories"), Lit("trace_summarize")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::None,
);
