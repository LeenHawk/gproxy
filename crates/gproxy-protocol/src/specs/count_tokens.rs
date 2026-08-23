use crate::spec::OperationSpec;
use crate::spec::Seg::{Lit, ParamAction};

use super::{FAM_CLA, FAM_GEM, FAM_OAI, NEVER, POST, free, ing};

pub(super) const COUNT_TOKENS: OperationSpec = free(&[
    ing(
        POST,
        &[Lit("v1"), Lit("messages"), Lit("count_tokens")],
        FAM_CLA,
        NEVER,
    ),
    ing(
        POST,
        &[Lit("v1"), Lit("responses"), Lit("input_tokens")],
        FAM_OAI,
        NEVER,
    ),
    ing(
        POST,
        &[
            Lit("v1beta"),
            Lit("models"),
            ParamAction("model", "countTokens"),
        ],
        FAM_GEM,
        NEVER,
    ),
]);
