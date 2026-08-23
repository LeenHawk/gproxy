use crate::spec::Seg::{Lit, ParamAction};
use crate::spec::{Affinity, OperationSpec};

use super::{BODY_OR_FORM_STREAM, BODY_STREAM, FAM_GEM, FAM_OAI, NEVER, POST, billed, ing};

pub(super) const CREATE_IMAGE: OperationSpec = billed(
    &[
        ing(
            POST,
            &[Lit("v1"), Lit("images"), Lit("generations")],
            FAM_OAI,
            BODY_STREAM,
        ),
        ing(
            POST,
            &[
                Lit("v1beta"),
                Lit("models"),
                ParamAction("model", "predict"),
            ],
            FAM_GEM,
            NEVER,
        ),
    ],
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
