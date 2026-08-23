use crate::spec::OperationSpec;
use crate::spec::Seg::{Lit, Param};

use super::{FAM_CLA, FAM_GEM, FAM_OAI, GET, NEVER, free, ing};

pub(super) const LIST_MODELS: OperationSpec = free(&[
    ing(GET, &[Lit("v1"), Lit("models")], FAM_OAI, NEVER),
    ing(GET, &[Lit("v1"), Lit("models")], FAM_CLA, NEVER),
    ing(GET, &[Lit("v1beta"), Lit("models")], FAM_GEM, NEVER),
]);

pub(super) const GET_MODEL: OperationSpec = free(&[
    ing(
        GET,
        &[Lit("v1"), Lit("models"), Param("id")],
        FAM_OAI,
        NEVER,
    ),
    ing(
        GET,
        &[Lit("v1"), Lit("models"), Param("id")],
        FAM_CLA,
        NEVER,
    ),
    ing(
        GET,
        &[Lit("v1beta"), Lit("models"), Param("model")],
        FAM_GEM,
        NEVER,
    ),
]);
