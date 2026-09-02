use crate::spec::OperationSpec;
use crate::spec::Seg::Lit;

use super::{FAM_OAI, NEVER, POST, free, ing};

pub(super) const CREATE_CONVERSATION: OperationSpec = free(&[ing(
    POST,
    &[Lit("v1"), Lit("conversations")],
    FAM_OAI,
    NEVER,
)]);
