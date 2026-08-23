use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec};

use super::{BODY_OR_FORM_STREAM, BODY_STREAM, FAM_OAI, POST, billed, ing};

pub(super) const CREATE_IMAGE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("images"), Lit("generations")],
        FAM_OAI,
        BODY_STREAM,
    )],
    Affinity::None,
);

pub(super) const EDIT_IMAGE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("images"), Lit("edits")],
        FAM_OAI,
        BODY_OR_FORM_STREAM,
    )],
    Affinity::None,
);
