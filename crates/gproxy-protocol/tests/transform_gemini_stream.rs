use gproxy_protocol::transform::generate::stream::gemini::*;
use gproxy_protocol::wire::gemini::generate_content::GenerateContentResponseBody;
use serde_json::json;

#[test]
fn collector_rejects_conflicting_ids_and_requires_terminal_candidate() {
    let mut collector = GeminiStreamCollector::new(GeminiStreamLimits::default());
    let first: GenerateContentResponseBody = serde_json::from_value(json!({"responseId":"r","modelVersion":"m","candidates":[{"index":0,"finishReason":"STOP","content":{"parts":[{"text":"ok"}]}}]})).unwrap();
    collector.push(first).unwrap();
    let merged = collector.finish().unwrap().value;
    assert_eq!(merged.response_id.as_deref(), Some("r"));
    let mut collector = GeminiStreamCollector::new(GeminiStreamLimits::default());
    let first: GenerateContentResponseBody =
        serde_json::from_value(json!({"responseId":"a","candidates":[]})).unwrap();
    collector.push(first).unwrap();
    let second: GenerateContentResponseBody =
        serde_json::from_value(json!({"responseId":"b","candidates":[]})).unwrap();
    assert!(collector.push(second).is_err());
}

#[test]
fn limits_and_unfinished_stream_are_explicit() {
    let chunk: GenerateContentResponseBody = serde_json::from_value(
        json!({"candidates":[{"index":0,"content":{"parts":[{"text":"x"}]}}]}),
    )
    .unwrap();
    let mut collector = GeminiStreamCollector::new(GeminiStreamLimits {
        max_bytes: serde_json::to_vec(&chunk).unwrap().len(),
    });
    collector.push(chunk.clone()).unwrap();
    assert!(collector.push(chunk).is_err());
    let mut collector = GeminiStreamCollector::new(GeminiStreamLimits::default());
    let chunk: GenerateContentResponseBody =
        serde_json::from_value(json!({"candidates":[]})).unwrap();
    collector.push(chunk).unwrap();
    assert!(collector.finish().is_err());
}

#[test]
fn accumulates_text_calls_signatures_late_ids_and_usage_after_terminal() {
    let mut collector = GeminiStreamCollector::new(Default::default());
    for chunk in [
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"first","x-extra":1},{"thought":true,"text":"why","thoughtSignature":"native-signature"}]},"x-extra":2}],"usageMetadata":{"promptTokenCount":10},"x-extra":3}),
        json!({"responseId":"late-id","modelVersion":"actual-model","candidates":[{"index":0,"content":{"parts":[{"text":"second"},{"functionCall":{"name":"f","id":"native-call","args":{"x-extra":"formal"}}}]},"finishReason":"STOP"}]}),
        json!({"usageMetadata":{"candidatesTokenCount":4,"thoughtsTokenCount":2,"totalTokenCount":16}}),
    ] {
        collector
            .push(serde_json::from_value(chunk).unwrap())
            .unwrap();
    }
    let out = collector.finish().unwrap().value;
    assert_eq!(out.response_id.as_deref(), Some("late-id"));
    let parts = out.candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0].text.as_deref(), Some("first"));
    assert_eq!(parts[2].text.as_deref(), Some("second"));
    assert_eq!(
        parts[1].thought_signature.as_deref(),
        Some("native-signature")
    );
    assert_eq!(
        parts[3]
            .function_call
            .as_ref()
            .unwrap()
            .args
            .as_ref()
            .unwrap()["x-extra"],
        "formal"
    );
    assert_eq!(
        out.usage_metadata.as_ref().unwrap().prompt_token_count,
        Some(10)
    );
    let val = serde_json::to_value(out).unwrap();
    assert!(val.get("x-extra").is_none());
    assert!(val["candidates"][0].get("x-extra").is_none());
    assert!(
        val["candidates"][0]["content"]["parts"][0]
            .get("x-extra")
            .is_none()
    );
}
#[test]
fn every_candidate_must_finish_and_no_content_after_finish() {
    let mut collector = GeminiStreamCollector::new(Default::default());
    collector.push(serde_json::from_value(json!({"candidates":[{"index":0,"finishReason":"STOP"},{"index":1,"content":{"parts":[{"text":"partial"}]}}]})).unwrap()).unwrap();
    assert!(collector.finish().is_err());
    let mut collector = GeminiStreamCollector::new(Default::default());
    collector
        .push(
            serde_json::from_value(json!({"candidates":[{"index":0,"finishReason":"STOP"}]}))
                .unwrap(),
        )
        .unwrap();
    assert!(
        collector
            .push(
                serde_json::from_value(
                    json!({"candidates":[{"index":0,"content":{"parts":[{"text":"late"}]}}]})
                )
                .unwrap()
            )
            .is_err()
    );
    assert!(collector.finish().is_err());
}
#[test]
fn decreasing_usage_is_rejected() {
    let mut collector = GeminiStreamCollector::new(Default::default());
    collector
        .push(serde_json::from_value(json!({"usageMetadata":{"totalTokenCount":10}})).unwrap())
        .unwrap();
    assert!(
        collector
            .push(serde_json::from_value(json!({"usageMetadata":{"totalTokenCount":9}})).unwrap())
            .is_err()
    );
}
#[test]
fn synthesis_is_native_terminal_chunk_bounded_and_rest_clean() {
    let input:GenerateContentResponseBody=serde_json::from_value(json!({"responseId":"r","modelVersion":"m","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"ok","x-unknown":true}]},"finishReason":"STOP"}],"usageMetadata":{"totalTokenCount":5},"x-unknown":true})).unwrap();
    let chunks = synthesize_gemini_stream(input.clone(), Default::default())
        .unwrap()
        .value;
    assert_eq!(chunks.len(), 1);
    assert!(
        !serde_json::to_value(&chunks)
            .unwrap()
            .to_string()
            .contains("x-unknown")
    );
    let mut collector = GeminiStreamCollector::new(Default::default());
    for chunk in chunks.clone() {
        collector.push(chunk).unwrap();
    }
    assert_eq!(collector.finish().unwrap().value, chunks[0]);
    assert!(synthesize_gemini_stream(input, GeminiStreamLimits { max_bytes: 1 }).is_err());
    let blocked =
        serde_json::from_value(json!({"promptFeedback":{"blockReason":"SAFETY"}})).unwrap();
    assert!(synthesize_gemini_stream(blocked, Default::default()).is_ok());
}

#[test]
fn signature_only_parts_stay_distinct_and_partial_metadata_does_not_erase_fields() {
    let mut collector = GeminiStreamCollector::new(Default::default());
    for chunk in [
        json!({"candidates":[{"content":{"parts":[{"text":"first"}]},"citationMetadata":{"citationSources":[{"uri":"https://first"}]},"logprobsResult":{"chosenCandidates":[{"token":"first","logProbability":-0.1}],"logProbabilitySum":-0.1}}]}),
        json!({"candidates":[{"content":{"parts":[{"text":"","thoughtSignature":"native-sig"}]},"citationMetadata":{"citationSources":[{"uri":"https://final"}]},"logprobsResult":{"topCandidates":[{"candidates":[{"token":"first","logProbability":-0.1}]}]},"finishReason":"STOP"}]}),
    ] {
        collector
            .push(serde_json::from_value(chunk).unwrap())
            .unwrap();
    }
    let output = collector.finish().unwrap().value;
    let candidates = output.candidates.unwrap();
    let candidate = &candidates[0];
    assert_eq!(candidate.index, Some(0));
    let parts = candidate.content.as_ref().unwrap().parts.as_ref().unwrap();
    assert_eq!(parts.len(), 2);
    assert!(parts[0].thought_signature.is_none());
    assert_eq!(parts[1].text.as_deref(), Some(""));
    assert_eq!(parts[1].thought_signature.as_deref(), Some("native-sig"));
    assert_eq!(
        candidate
            .citation_metadata
            .as_ref()
            .unwrap()
            .citation_sources
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        candidate
            .citation_metadata
            .as_ref()
            .unwrap()
            .citation_sources
            .as_ref()
            .unwrap()[0]
            .uri
            .as_deref(),
        Some("https://final")
    );
    let logs = candidate.logprobs_result.as_ref().unwrap();
    assert!(logs.chosen_candidates.is_some());
    assert!(logs.top_candidates.is_some());
    assert_eq!(logs.log_probability_sum, Some(-0.1));
}

#[test]
fn byte_budget_applies_to_encoded_event_and_poison_prevents_partial_finish() {
    let mut collector = GeminiStreamCollector::new(GeminiStreamLimits { max_bytes: 32 });
    let chunk=serde_json::from_value(json!({"candidates":[{"content":{"parts":[{"text":"x".repeat(8192)}]},"finishReason":"STOP"}]})).unwrap();
    assert!(collector.push(chunk).is_err());
    assert!(collector.finish().is_err());
}
