use crate::spec::Seg::{Lit, ParamAction};
use crate::spec::{Affinity, OperationSpec};

use super::{FAM_GEM, FAM_OAI, NEVER, POST, billed, ing};

pub(super) const EMBEDDING: OperationSpec = billed(
    &[
        ing(POST, &[Lit("v1"), Lit("embeddings")], FAM_OAI, NEVER),
        ing(
            POST,
            &[
                Lit("v1beta"),
                Lit("models"),
                ParamAction("model", "embedContent"),
            ],
            FAM_GEM,
            NEVER,
        ),
    ],
    Affinity::None,
);

pub(super) const BATCH_EMBEDDING: OperationSpec = billed(
    &[ing(
        POST,
        &[
            Lit("v1beta"),
            Lit("models"),
            ParamAction("model", "batchEmbedContents"),
        ],
        FAM_GEM,
        NEVER,
    )],
    Affinity::None,
);
