use crate::operation::ContentGenerationKind::OpenAiResponses;
use crate::operation::OperationKind::ContentGeneration as CG;
use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec, StreamDetect};

use super::{POST, billed, ing};

pub(super) const REVIEW: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("guardian")],
        CG(OpenAiResponses),
        StreamDetect::Always,
    )],
    Affinity::Session,
);

pub(super) const CLASSIFY: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("guardian-classifier")],
        CG(OpenAiResponses),
        StreamDetect::Always,
    )],
    Affinity::Session,
);
