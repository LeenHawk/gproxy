use gproxy_protocol::gemini::generate_content::{
    BlockReason, Candidate, FinishReason, GenerateContentRequestBody, GenerateContentResponseBody,
    ModelStage, SearchEntryPoint,
};
use gproxy_protocol::gemini::{Content, TextMimeType};

#[test]
fn generate_content_request_is_path_independent_and_typed() {
    let value = serde_json::json!({
        "contents": [{"role":"user", "parts":[{"text":"hello"}]}],
        "tools": [{"functionDeclarations":[{"name":"lookup","description":"find","parameters":{"type":"OBJECT"}}]}],
        "toolConfig": {"functionCallingConfig":{"mode":"AUTO"}},
        "safetySettings": [{"category":"HARM_CATEGORY_HATE_SPEECH","threshold":"BLOCK_LOW_AND_ABOVE"}],
        "systemInstruction": {"parts":[{"text":"be concise"}]},
        "generationConfig": {"responseMimeType":"application/json","responseSchema":{"type":"OBJECT"}},
        "cachedContent":"cachedContents/abc",
        "serviceTier":"priority",
        "store":true,
        "future_request": {"enabled":true}
    });
    let parsed: GenerateContentRequestBody = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        parsed.contents[0].parts.as_ref().unwrap()[0]
            .text
            .as_deref(),
        Some("hello")
    );
    assert_eq!(
        parsed
            .generation_config
            .as_ref()
            .unwrap()
            .response_mime_type
            .as_deref(),
        Some("application/json")
    );
    assert!(parsed.rest.contains_key("future_request"));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

fn response_fixture() -> serde_json::Value {
    serde_json::json!({
        "candidates": [{
            "content":{"parts":[{"text":"answer"}],"role":"model"},
            "finishReason":"STOP",
            "safetyRatings":[{"category":"HARM_CATEGORY_HATE_SPEECH","probability":"NEGLIGIBLE","blocked":false}],
            "citationMetadata":{"citationSources":[{"startIndex":0,"endIndex":6,"uri":"https://example.test","license":"MIT"}]},
            "tokenCount":3,
            "groundingAttributions":[{"sourceId":{"groundingPassage":{"passageId":"p1","partIndex":0}},"content":{"parts":[{"text":"source"}]}}],
            "groundingMetadata":{
                "groundingChunks":[{"web":{"uri":"https://example.test","title":"Example"}},{"image":{"sourceUri":"https://source","imageUri":"https://image","title":"Image","domain":"example.test"}},{"retrievedContext":{"customMetadata":[{"key":"kind","stringListValue":{"values":["one","two"]}}],"uri":"gs://doc","title":"Doc","text":"context","fileSearchStore":"fileSearchStores/1","pageNumber":2,"mediaId":"media/1"}},{"maps":{"uri":"maps/1","title":"Place","text":"details","placeId":"place/1","placeAnswerSources":{"reviewSnippets":[{"reviewId":"review/1","googleMapsUri":"https://maps","title":"Great"}]}}}],
                "groundingSupports":[{"groundingChunkIndices":[0],"confidenceScores":[0.9],"renderedParts":[0],"segment":{"partIndex":0,"startIndex":0,"endIndex":6,"text":"answer"}}],
                "webSearchQueries":["query"],"imageSearchQueries":["image query"],"searchEntryPoint":{"renderedContent":"<a>","sdkBlob":"blob"},"retrievalMetadata":{"googleSearchDynamicRetrievalScore":0.5},"googleMapsWidgetContextToken":"widget"
            },
            "avgLogprobs":-0.1,
            "logprobsResult":{"topCandidates":[{"candidates":[{"token":"answer","tokenId":1,"logProbability":-0.1}]}],"chosenCandidates":[{"token":"answer","tokenId":1,"logProbability":-0.1}],"logProbabilitySum":-0.1},
            "urlContextMetadata":{"urlMetadata":[{"retrievedUrl":"https://example.test","urlRetrievalStatus":"URL_RETRIEVAL_STATUS_SUCCESS"}]},
            "index":0,"finishMessage":"done","future_candidate":7
        }],
        "promptFeedback":{"blockReason":"SAFETY","safetyRatings":[{"category":"HARM_CATEGORY_HATE_SPEECH","probability":"LOW"}]},
        "usageMetadata":{"promptTokenCount":10,"cachedContentTokenCount":2,"candidatesTokenCount":3,"toolUsePromptTokenCount":1,"thoughtsTokenCount":4,"totalTokenCount":20,"promptTokensDetails":[{"modality":"TEXT","tokenCount":10}],"cacheTokensDetails":[{"modality":"TEXT","tokenCount":2}],"candidatesTokensDetails":[{"modality":"TEXT","tokenCount":3}],"toolUsePromptTokensDetails":[{"modality":"TEXT","tokenCount":1}],"serviceTier":"priority"},
        "modelVersion":"gemini-test","responseId":"resp-1","modelStatus":{"modelStage":"STABLE","retirementTime":"2030-01-01T00:00:00Z","message":"healthy"},"future_response":true
    })
}

#[test]
fn generate_content_response_types_all_documented_nested_metadata() {
    let value = response_fixture();
    let parsed: GenerateContentResponseBody = serde_json::from_value(value.clone()).unwrap();
    let candidate: &Candidate = &parsed.candidates.as_ref().unwrap()[0];
    assert!(matches!(candidate.finish_reason, Some(FinishReason::Stop)));
    assert_eq!(
        candidate
            .grounding_metadata
            .as_ref()
            .unwrap()
            .grounding_chunks
            .as_ref()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(candidate.rest["future_candidate"], 7);
    assert!(matches!(
        parsed.prompt_feedback.as_ref().unwrap().block_reason,
        Some(BlockReason::Safety)
    ));
    assert!(matches!(
        parsed.model_status.as_ref().unwrap().model_stage,
        Some(ModelStage::Stable)
    ));
    assert!(parsed.rest.contains_key("future_response"));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn generate_content_aliases_and_direct_builders_use_canonical_wire_names() {
    let content: Content = serde_json::from_value(serde_json::json!({
        "parts":[{"text":"hello"}]
    }))
    .unwrap();
    let body: GenerateContentRequestBody = serde_json::from_value(serde_json::json!({
        "contents":[{"parts":[{"text":"hello"}]}],
        "generation_config":{"response_mime_type":"text/plain","response_modalities":["TEXT"]}
    }))
    .unwrap();
    assert_eq!(
        content.parts.as_ref().unwrap()[0].text.as_deref(),
        Some("hello")
    );
    assert_eq!(
        body.generation_config
            .unwrap()
            .response_mime_type
            .as_deref(),
        Some("text/plain")
    );
    assert_eq!(
        serde_json::to_value(TextMimeType::TextPlain).unwrap(),
        "TEXT_PLAIN"
    );
    let entry = SearchEntryPoint::builder().build();
    assert_eq!(serde_json::to_value(entry).unwrap(), serde_json::json!({}));
    assert!(serde_json::from_value::<FinishReason>(serde_json::json!("FUTURE")).is_err());
}

fn snake_case_keys(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| {
                    let mut snake = String::new();
                    for ch in key.chars() {
                        if ch.is_ascii_uppercase() {
                            snake.push('_');
                            snake.push(ch.to_ascii_lowercase());
                        } else {
                            snake.push(ch);
                        }
                    }
                    (snake, snake_case_keys(value))
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(snake_case_keys).collect()),
        value => value,
    }
}

#[test]
fn response_aliases_are_typed_and_emit_only_canonical_names() {
    let mut canonical = response_fixture();
    canonical.as_object_mut().unwrap().remove("future_response");
    canonical["candidates"][0]
        .as_object_mut()
        .unwrap()
        .remove("future_candidate");
    let parsed: GenerateContentResponseBody =
        serde_json::from_value(snake_case_keys(canonical.clone())).unwrap();
    assert!(
        format!("{parsed:?}")
            .split("rest: ")
            .skip(1)
            .all(|rest| rest.starts_with("{}")),
        "known fields entered rest: {parsed:?}"
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), canonical);
}

#[test]
fn proto_response_objects_allow_omitted_default_fields() {
    let value = serde_json::json!({
        "candidates":[{
            "content":{},
            "groundingAttributions":[{"sourceId":{"groundingPassage":{}},"content":{}},{"sourceId":{"semanticRetrieverChunk":{}}}],
            "groundingMetadata":{
                "groundingChunks":[{"web":{}},{"image":{}},{"retrievedContext":{"customMetadata":[{}, {"stringValue":""},{"stringListValue":{}},{"numericValue":0.0}]}},{"maps":{"placeAnswerSources":{"reviewSnippets":[{}]}}}],
                "groundingSupports":[{"segment":{"endIndex":1}}],
                "searchEntryPoint":{},"retrievalMetadata":{}
            },
            "logprobsResult":{"topCandidates":[{}],"chosenCandidates":[{}]},
            "urlContextMetadata":{"urlMetadata":[{}]},"citationMetadata":{}
        }],
        "usageMetadata":{"promptTokensDetails":[{},{"modality":"TEXT"},{"tokenCount":0}]},
        "modelStatus":{},"promptFeedback":{}
    });
    let parsed: GenerateContentResponseBody = serde_json::from_value(value.clone()).unwrap();
    assert!(
        format!("{parsed:?}")
            .split("rest: ")
            .skip(1)
            .all(|rest| rest.starts_with("{}"))
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn proto_null_means_unset_and_required_fields_stay_required() {
    let value = serde_json::json!({
        "candidates":[{"index":null,"content":{"parts":null},"groundingMetadata":{"groundingChunks":[{"web":null}],"webSearchQueries":null},"logprobsResult":{"logProbabilitySum":null}}],
        "usageMetadata":{"promptTokenCount":null,"promptTokensDetails":[{"modality":null,"tokenCount":null}]},
        "modelStatus":{"modelStage":null},"responseId":null
    });
    let parsed: GenerateContentResponseBody = serde_json::from_value(value).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        serde_json::json!({
            "candidates":[{"content":{},"groundingMetadata":{"groundingChunks":[{}]},"logprobsResult":{}}],
            "usageMetadata":{"promptTokensDetails":[{}]},"modelStatus":{}
        })
    );
    for value in [serde_json::json!({}), serde_json::json!({"contents":null})] {
        assert!(serde_json::from_value::<GenerateContentRequestBody>(value).is_err());
    }
    for value in [
        serde_json::json!({}),
        serde_json::json!({"category":"HARM_CATEGORY_HATE_SPEECH"}),
    ] {
        assert!(
            serde_json::from_value::<gproxy_protocol::gemini::generate_content::SafetyRating>(
                value
            )
            .is_err()
        );
    }
}

#[test]
fn custom_metadata_oneof_members_use_sibling_wire_fields() {
    use gproxy_protocol::gemini::generate_content::CustomMetadata;
    for value in [
        serde_json::json!({"key":"name","stringValue":"text"}),
        serde_json::json!({"key":"tags","stringListValue":{"values":["a","b"]}}),
        serde_json::json!({"key":"score","numericValue":1.5}),
    ] {
        let parsed: CustomMetadata =
            serde_json::from_value(snake_case_keys(value.clone())).unwrap();
        assert!(parsed.rest.is_empty());
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
}

#[test]
fn request_response_aliases_keep_http_transport_separate() {
    use gproxy_protocol::gemini::Part;
    use gproxy_protocol::gemini::generate_content::{
        GenerateContentRequest, GenerateContentResponse,
    };
    use gproxy_protocol::{WireRequest, WireResponse};
    use http::{HeaderMap, HeaderValue, Method, StatusCode};

    let body = GenerateContentRequestBody::builder(vec![
        Content::builder()
            .parts(vec![Part::builder().text("hello").build()])
            .build(),
    ])
    .build();
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    let request: GenerateContentRequest = WireRequest {
        method: Method::POST,
        path: "/v1beta/models/gemini-2.0-flash:generateContent".into(),
        query: Some("key=test".into()),
        headers: headers.clone(),
        body,
    };
    assert_eq!(request.method, Method::POST);
    assert_eq!(
        request.path,
        "/v1beta/models/gemini-2.0-flash:generateContent"
    );
    assert_eq!(request.query.as_deref(), Some("key=test"));
    assert_eq!(request.headers, headers);
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        serde_json::json!({"contents":[{"parts":[{"text":"hello"}]}]})
    );
    let response: GenerateContentResponse = WireResponse {
        status: StatusCode::OK,
        headers: headers.clone(),
        body: GenerateContentResponseBody::builder()
            .response_id("r")
            .build(),
    };
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers, headers);
    assert_eq!(
        serde_json::to_value(response.body).unwrap(),
        serde_json::json!({"responseId":"r"})
    );
}

#[test]
fn response_enums_cover_every_documented_value() {
    use serde::{Serialize, de::DeserializeOwned};
    fn check<T: Serialize + DeserializeOwned>(values: &[&str]) {
        for value in values {
            let parsed: T = serde_json::from_value(serde_json::json!(value)).unwrap();
            assert_eq!(
                serde_json::to_value(parsed).unwrap(),
                serde_json::json!(value)
            );
        }
        assert!(serde_json::from_value::<T>(serde_json::json!("FUTURE_UNDOCUMENTED")).is_err());
    }
    check::<gproxy_protocol::gemini::generate_content::BlockReason>(&[
        "BLOCK_REASON_UNSPECIFIED",
        "SAFETY",
        "OTHER",
        "BLOCKLIST",
        "PROHIBITED_CONTENT",
        "IMAGE_SAFETY",
    ]);
    check::<gproxy_protocol::gemini::generate_content::ModelStage>(&[
        "MODEL_STAGE_UNSPECIFIED",
        "UNSTABLE_EXPERIMENTAL",
        "EXPERIMENTAL",
        "PREVIEW",
        "STABLE",
        "LEGACY",
        "DEPRECATED",
        "RETIRED",
    ]);
    check::<gproxy_protocol::gemini::generate_content::FinishReason>(&[
        "FINISH_REASON_UNSPECIFIED",
        "STOP",
        "MAX_TOKENS",
        "SAFETY",
        "RECITATION",
        "LANGUAGE",
        "OTHER",
        "BLOCKLIST",
        "PROHIBITED_CONTENT",
        "SPII",
        "MALFORMED_FUNCTION_CALL",
        "IMAGE_SAFETY",
        "IMAGE_PROHIBITED_CONTENT",
        "IMAGE_OTHER",
        "NO_IMAGE",
        "IMAGE_RECITATION",
        "UNEXPECTED_TOOL_CALL",
        "TOO_MANY_TOOL_CALLS",
        "MISSING_THOUGHT_SIGNATURE",
        "MALFORMED_RESPONSE",
    ]);
    check::<gproxy_protocol::gemini::generate_content::HarmProbability>(&[
        "HARM_PROBABILITY_UNSPECIFIED",
        "NEGLIGIBLE",
        "LOW",
        "MEDIUM",
        "HIGH",
    ]);
    check::<gproxy_protocol::gemini::generate_content::UrlRetrievalStatus>(&[
        "URL_RETRIEVAL_STATUS_UNSPECIFIED",
        "URL_RETRIEVAL_STATUS_SUCCESS",
        "URL_RETRIEVAL_STATUS_ERROR",
        "URL_RETRIEVAL_STATUS_PAYWALL",
        "URL_RETRIEVAL_STATUS_UNSAFE",
    ]);
    check::<gproxy_protocol::gemini::HarmCategory>(&[
        "HARM_CATEGORY_UNSPECIFIED",
        "HARM_CATEGORY_DEROGATORY",
        "HARM_CATEGORY_TOXICITY",
        "HARM_CATEGORY_VIOLENCE",
        "HARM_CATEGORY_SEXUAL",
        "HARM_CATEGORY_MEDICAL",
        "HARM_CATEGORY_DANGEROUS",
        "HARM_CATEGORY_HARASSMENT",
        "HARM_CATEGORY_HATE_SPEECH",
        "HARM_CATEGORY_SEXUALLY_EXPLICIT",
        "HARM_CATEGORY_DANGEROUS_CONTENT",
        "HARM_CATEGORY_CIVIC_INTEGRITY",
    ]);
    check::<gproxy_protocol::gemini::Modality>(&["MODALITY_UNSPECIFIED", "TEXT", "IMAGE", "AUDIO"]);
    check::<gproxy_protocol::gemini::ServiceTier>(&["unspecified", "standard", "flex", "priority"]);
}
