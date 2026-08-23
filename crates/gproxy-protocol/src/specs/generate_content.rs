use crate::operation::ContentGenerationKind::*;
use crate::operation::OperationKind::ContentGeneration as CG;
use crate::spec::Seg::{Lit, ParamAction};
use crate::spec::{Affinity, OperationSpec, StreamDetect};

use super::{BODY_STREAM, NEVER, POST, billed, ing};

pub(super) const GENERATE: OperationSpec = billed(
    &[
        ing(
            POST,
            &[Lit("v1"), Lit("chat"), Lit("completions")],
            CG(OpenAiChat),
            BODY_STREAM,
        ),
        ing(
            POST,
            &[Lit("v1"), Lit("responses")],
            CG(OpenAiResponses),
            BODY_STREAM,
        ),
        ing(
            POST,
            &[Lit("v1"), Lit("messages")],
            CG(ClaudeMessages),
            BODY_STREAM,
        ),
        ing(
            POST,
            &[
                Lit("v1beta"),
                Lit("models"),
                ParamAction("model", "generateContent"),
            ],
            CG(GeminiGenerateContent),
            NEVER,
        ),
    ],
    Affinity::Session,
);

pub(super) const STREAM_GENERATE: OperationSpec = billed(
    &[ing(
        POST,
        &[
            Lit("v1beta"),
            Lit("models"),
            ParamAction("model", "streamGenerateContent"),
        ],
        CG(GeminiGenerateContent),
        StreamDetect::Always,
    )],
    Affinity::Session,
);
