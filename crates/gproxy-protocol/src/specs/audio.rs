use crate::spec::Seg::Lit;
use crate::spec::{Affinity, OperationSpec, StreamDetect};

use super::{BODY_OR_FORM_STREAM, FAM_OAI, NEVER, POST, billed, ing};

pub(super) const SPEECH: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("speech")],
        FAM_OAI,
        StreamDetect::BodyValue("stream_format", "sse"),
    )],
    Affinity::None,
);

pub(super) const TRANSCRIBE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("transcriptions")],
        FAM_OAI,
        BODY_OR_FORM_STREAM,
    )],
    Affinity::None,
);

pub(super) const TRANSLATE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("translations")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::None,
);
