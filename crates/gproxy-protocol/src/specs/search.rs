use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec};

use super::{FAM_OAI, NEVER, POST, billed, ing};

pub(super) const WEB_SEARCH: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("alpha"), Lit("search")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::None,
);
