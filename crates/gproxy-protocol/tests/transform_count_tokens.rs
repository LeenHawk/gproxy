use gproxy_protocol::{
    transform::{TransformErrorKind, count_tokens::*},
    wire::{claude::count_tokens as c, gemini::count_tokens as g, openai::count_tokens as o},
};
use serde_json::json;

#[test]
fn six_directions_preserve_counts_above_old_u32_limits_and_empty_extensions() {
    let count = i64::MAX;
    let openai: o::CountTokensResponseBody = serde_json::from_value(
        json!({"object":"response.input_tokens","input_tokens":count,"extra":{"bad":true}}),
    )
    .unwrap();
    let gemini = openai_to_gemini_response(openai.clone()).unwrap().value;
    assert_eq!(gemini.total_tokens, Some(count));
    assert!(gemini.rest.is_empty());
    assert_eq!(
        gemini_to_openai_response(gemini.clone())
            .unwrap()
            .value
            .input_tokens,
        count
    );
    let claude = openai_to_claude_response(openai, ClaudeCountContext::Unedited)
        .unwrap()
        .value;
    assert_eq!(claude.context_management.original_input_tokens, count);
    assert!(claude.rest.is_empty());
    assert!(claude.context_management.rest.is_empty());
    assert_eq!(
        claude_to_openai_response(claude.clone())
            .unwrap()
            .value
            .input_tokens,
        count
    );
    assert_eq!(
        claude_to_gemini_response(claude)
            .unwrap()
            .value
            .total_tokens,
        Some(count)
    );
    assert_eq!(
        gemini_to_claude_response(gemini, ClaudeCountContext::OriginalInputTokens(123))
            .unwrap()
            .value
            .context_management
            .original_input_tokens,
        123
    );
}

#[test]
fn unknown_and_invalid_counts_do_not_turn_into_zero_or_saturation() {
    assert_eq!(
        gemini_to_openai_response(g::CountTokensResponseBody::builder().build())
            .unwrap_err()
            .kind(),
        TransformErrorKind::MissingMetadata
    );
    assert_eq!(
        gemini_to_openai_response(
            g::CountTokensResponseBody::builder()
                .total_tokens(-1)
                .build()
        )
        .unwrap_err()
        .kind(),
        TransformErrorKind::InvalidResult
    );
    let zero =
        o::CountTokensResponseBody::builder(0, o::CountTokensObject::ResponseInputTokens).build();
    assert_eq!(
        openai_to_claude_response(zero.clone(), ClaudeCountContext::Unknown)
            .unwrap_err()
            .kind(),
        TransformErrorKind::MissingMetadata
    );
    assert_eq!(
        openai_to_claude_response(zero, ClaudeCountContext::Unedited)
            .unwrap()
            .value
            .input_tokens,
        0
    );
}

#[test]
fn original_count_comes_from_request_or_actual_context_edit_facts() {
    let request: c::CountTokensRequestBody = serde_json::from_value(
        json!({"model":"source","messages":[],"rest":{"context_management":{"edits":[{}]}}}),
    )
    .unwrap();
    assert_eq!(
        ClaudeCountContext::for_request(&request),
        ClaudeCountContext::Unedited
    );
    let request: c::CountTokensRequestBody = serde_json::from_value(json!({"model":"source","messages":[],"context_management":{"edits":[{"type":"clear_thinking_20251015"}]}})).unwrap();
    assert_eq!(
        ClaudeCountContext::for_request(&request),
        ClaudeCountContext::Unknown
    );
    let response = g::CountTokensResponseBody::builder()
        .total_tokens(42)
        .build();
    let converted =
        gemini_to_claude_response(response, ClaudeCountContext::OriginalInputTokens(84)).unwrap();
    assert_eq!(converted.value.input_tokens, 42);
    assert_eq!(converted.value.context_management.original_input_tokens, 84);
}

#[test]
fn gemini_details_are_validated_and_omitted_with_diagnostics() {
    let response: g::CountTokensResponseBody = serde_json::from_value(json!({"totalTokens":12,"cachedContentTokenCount":4,"promptTokensDetails":[{"modality":"TEXT","tokenCount":12,"unknown":true}],"cacheTokensDetails":[{"modality":"TEXT","tokenCount":4}],"unknown":true})).unwrap();
    let result = gemini_to_openai_response(response.clone()).unwrap();
    assert_eq!(result.report.diagnostics.len(), 3);
    assert!(result.value.rest.is_empty());
    let mut invalid = response;
    invalid.cached_content_token_count = Some(13);
    assert_eq!(
        gemini_to_openai_response(invalid).unwrap_err().kind(),
        TransformErrorKind::InvalidResult
    );
}
