use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec};

use super::{FAM_OAI, NEVER, POST, billed, ing};

pub(super) const RERANK: OperationSpec = billed(
    &[ing(POST, &[Lit("v1"), Lit("rerank")], FAM_OAI, NEVER)],
    Affinity::None,
);
