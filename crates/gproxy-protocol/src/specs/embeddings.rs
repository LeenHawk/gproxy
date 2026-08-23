use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec};

use super::{FAM_OAI, NEVER, POST, billed, ing};

pub(super) const EMBEDDING: OperationSpec = billed(
    &[ing(POST, &[Lit("v1"), Lit("embeddings")], FAM_OAI, NEVER)],
    Affinity::None,
);
