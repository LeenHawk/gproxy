//! Codex Guardian review/classifier requests and generated-result extraction.
//!
//! Supported preparation requires an explicit source policy and self-contained
//! declared evidence. Optional opaque reasoning can be omitted when readable
//! reasoning is present. Required encrypted context, active external tools and
//! access programs need host capabilities and fail explicitly here.
//!
//! Four target APIs receive native controls and media parts. Resource-assisted
//! preparation lives in `adapt::guardian`; URL-capable targets retain URLs.
//! This is contract adaptation, not empirical equality of model judgments.

mod request;
mod response;
pub use request::{
    GuardianDialectRequest, GuardianOperation, GuardianPreparedRequest, GuardianRequestContext,
    GuardianTarget, preflight_request, prepare_claude, prepare_claude_with_limits, prepare_gemini,
    prepare_gemini_with_limits, prepare_openai_chat, prepare_openai_chat_with_limits,
    prepare_openai_responses, prepare_openai_responses_with_limits,
};
pub use response::{
    GuardianClassifyResult, GuardianExtraction, GuardianOutcome, GuardianReviewResult,
    GuardianRiskLevel, GuardianUserAuthorization, extract_chat, extract_claude, extract_gemini,
    extract_responses,
};

pub(crate) use request::{finish_attachments, prepare_unattached};
