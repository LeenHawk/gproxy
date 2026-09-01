use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec};

use super::{FAM_OAI, NEVER, POST, billed, ing};

pub(super) const COMPACT: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("responses"), Lit("compact")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::Session,
);
