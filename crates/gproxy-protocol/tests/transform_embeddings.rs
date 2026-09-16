use gproxy_protocol::Rest;
use gproxy_protocol::transform::embeddings::*;
use gproxy_protocol::wire::{
    gemini, gemini::embeddings as gemini_embeddings, openai::embeddings as openai,
};

fn number(value: f64) -> serde_json::Number {
    serde_json::Number::from_f64(value).unwrap()
}

fn openai_request(input: openai::EmbeddingInput) -> openai::CreateEmbeddingRequestBody {
    {
        let mut fixture =
            openai::CreateEmbeddingRequestBody::builder(input, "source-model".into()).build();
        fixture.dimensions = Some(2);
        fixture.encoding_format = Some(openai::EmbeddingEncodingFormat::Float);
        fixture.user = None;
        fixture.rest = {
            let mut rest = Rest::new();
            rest.insert("foreign".into(), serde_json::json!(true));
            rest
        };
        fixture
    }
}

fn gemini_request(model: &str, text: &str) -> gemini_embeddings::BatchEmbedContentRequest {
    {
        let mut fixture = gemini_embeddings::BatchEmbedContentRequest::builder(model.into(), {
            let mut fixture = gemini::Content::builder().build();
            fixture.parts = Some(vec![{
                let mut fixture = gemini::Part::builder().build();
                fixture.thought = None;
                fixture.thought_signature = None;
                fixture.part_metadata = None;
                fixture.media_resolution = None;
                fixture.text = Some(text.into());
                fixture.inline_data = None;
                fixture.function_call = None;
                fixture.function_response = None;
                fixture.file_data = None;
                fixture.executable_code = None;
                fixture.code_execution_result = None;
                fixture.tool_call = None;
                fixture.tool_response = None;
                fixture.video_metadata = None;
                fixture.rest = Rest::new();
                fixture
            }]);
            fixture.role = None;
            fixture.rest = Rest::new();
            fixture
        })
        .build();
        fixture.task_type = None;
        fixture.title = None;
        fixture.output_dimensionality = Some(2);
        fixture.embed_content_config = None;
        fixture.rest = Rest::new();
        fixture
    }
}

fn openai_response() -> openai::CreateEmbeddingResponseBody {
    {
        let mut fixture = openai::CreateEmbeddingResponseBody::builder(
            vec![{
                let mut fixture = openai::OpenAiEmbedding::builder(
                    openai::EmbeddingVector::Floats(vec![number(1.0), number(-2.5)]),
                    0,
                    openai::EmbeddingObject::Embedding,
                )
                .build();
                fixture.rest = Rest::new();
                fixture
            }],
            "source-model".into(),
            openai::EmbeddingListObject::List,
            {
                let mut fixture = openai::EmbeddingUsage::builder(3, 3).build();
                fixture.rest = Rest::new();
                fixture
            },
        )
        .build();
        fixture.rest = Rest::new();
        fixture
    }
}

fn gemini_response(
    values: Vec<serde_json::Number>,
    usage: Option<i64>,
) -> gemini_embeddings::EmbedContentResponseBody {
    {
        let mut fixture = gemini_embeddings::EmbedContentResponseBody::builder().build();
        fixture.embedding = Some({
            let mut fixture = gemini_embeddings::ContentEmbedding::builder().build();
            fixture.values = Some(values);
            fixture.shape = Some(vec![2]);
            fixture.rest = Rest::new();
            fixture
        });
        fixture.usage_metadata = usage.map(|prompt_token_count| {
            let mut fixture = gemini_embeddings::EmbeddingUsageMetadata::builder().build();
            fixture.prompt_token_count = Some(prompt_token_count);
            fixture.prompt_token_details = None;
            fixture.rest = Rest::new();
            fixture
        });
        fixture.rest = Rest::new();
        fixture
    }
}

#[test]
fn openai_single_request_and_response_preserve_text_dimensions_and_values() {
    let converted = openai_to_gemini_single(
        openai_request(openai::EmbeddingInput::Text("hello".into())),
        "text-embedding-gemini",
    )
    .unwrap()
    .value;
    let converted = converted.body;
    assert_eq!(converted.output_dimensionality, Some(2));
    assert_eq!(
        converted.content.parts.unwrap()[0].text.as_deref(),
        Some("hello")
    );
    assert!(converted.rest.is_empty());

    let converted = openai_single_response_to_gemini(openai_response())
        .unwrap()
        .value;
    assert_eq!(
        converted.embedding.unwrap().values.unwrap(),
        vec![number(1.0), number(-2.5)]
    );
    assert_eq!(
        converted.usage_metadata.unwrap().prompt_token_count,
        Some(3)
    );
    assert!(converted.rest.is_empty());
}

#[test]
fn openai_batch_and_gemini_batch_preserve_order_and_model_resources() {
    let request = openai_request(openai::EmbeddingInput::Texts(vec!["a".into(), "b".into()]));
    let converted = openai_to_gemini_batch(request, "text-embedding-gemini")
        .unwrap()
        .value;
    let converted = converted.body;
    assert_eq!(converted.requests.len(), 2);
    assert_eq!(converted.requests[0].model, "models/text-embedding-gemini");
    assert_eq!(
        converted.requests[1].content.parts.as_ref().unwrap()[0]
            .text
            .as_deref(),
        Some("b")
    );
    assert!(converted.rest.is_empty());

    let request = {
        let mut fixture = gemini_embeddings::BatchEmbedContentsRequestBody::builder(vec![
            gemini_request("models/gemini-embedding", "a"),
            gemini_request("models/gemini-embedding", "b"),
        ])
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    let converted = gemini_batch_to_openai(request, "text-embedding-openai")
        .unwrap()
        .value;
    assert_eq!(converted.model, "text-embedding-openai");
    assert_eq!(
        converted.input,
        openai::EmbeddingInput::Texts(vec!["a".into(), "b".into()])
    );
}

#[test]
fn missing_usage_and_unsupported_input_semantics_are_explicit() {
    let response = gemini_response(vec![number(1.0), number(2.0)], None);
    let missing = gemini_single_response_to_openai(
        response.clone(),
        &OpenAiResponseSupplement {
            model: "target".into(),
            usage: None,
            encoding_format: openai::EmbeddingEncodingFormat::Float,
        },
    );
    assert!(missing.is_err());
    let converted = gemini_single_response_to_openai(
        response,
        &OpenAiResponseSupplement {
            model: "target".into(),
            usage: Some(OpenAiUsageFacts {
                prompt_tokens: 2,
                total_tokens: 2,
            }),
            encoding_format: openai::EmbeddingEncodingFormat::Float,
        },
    )
    .unwrap()
    .value;
    assert_eq!(converted.usage.prompt_tokens, 2);

    let tokens = openai_to_gemini_single(
        openai_request(openai::EmbeddingInput::Tokens(vec![1, 2])),
        "target",
    );
    assert!(tokens.is_ok());
    let many = openai_to_gemini_single(
        openai_request(openai::EmbeddingInput::Texts(vec!["a".into(), "b".into()])),
        "target",
    );
    assert!(many.is_ok());
}

#[test]
fn base64_is_binary32_little_endian_and_batch_models_must_match() {
    let response = gemini_response(vec![number(1.0), number(-2.5)], Some(4));
    let converted = gemini_single_response_to_openai(
        response,
        &OpenAiResponseSupplement {
            model: "target".into(),
            usage: None,
            encoding_format: openai::EmbeddingEncodingFormat::Base64,
        },
    )
    .unwrap()
    .value;
    let openai::EmbeddingVector::Base64(encoded) = &converted.data[0].embedding else {
        panic!("expected base64 embedding");
    };
    assert_eq!(encoded, "AACAPwAAIMA=");

    let mut second = gemini_request("models/other", "b");
    second.output_dimensionality = Some(2);
    let request = {
        let mut fixture = gemini_embeddings::BatchEmbedContentsRequestBody::builder(vec![
            gemini_request("models/one", "a"),
            second,
        ])
        .build();
        fixture.rest = Rest::new();
        fixture
    };
    assert!(gemini_batch_to_openai(request, "target").is_ok());
}

#[test]
fn empty_and_invalid_dimensions_are_rejected() {
    let empty = openai_request(openai::EmbeddingInput::Texts(Vec::new()));
    assert!(openai_to_gemini_batch(empty, "target").is_ok());
    let mut invalid = openai_request(openai::EmbeddingInput::Text("x".into()));
    invalid.dimensions = Some(0);
    assert!(openai_to_gemini_single(invalid, "target").is_ok());
    let mut invalid_shape = gemini_response(vec![number(1.0), number(2.0)], Some(1));
    invalid_shape.embedding.as_mut().unwrap().shape = Some(vec![1]);
    assert!(
        gemini_single_response_to_openai(
            invalid_shape,
            &OpenAiResponseSupplement {
                model: "target".into(),
                usage: None,
                encoding_format: openai::EmbeddingEncodingFormat::Float,
            }
        )
        .is_ok()
    );
}

#[test]
fn prepared_request_preserves_encoding_and_validates_return_shape() {
    let mut source = openai_request(openai::EmbeddingInput::Text("hello".into()));
    source.encoding_format = Some(openai::EmbeddingEncodingFormat::Base64);
    let prepared = openai_to_gemini_single(source, "selected-model")
        .unwrap()
        .value;
    assert_eq!(prepared.response.requested_model, "source-model");
    assert_eq!(prepared.response.upstream_model, "models/selected-model");
    let output = prepared
        .response
        .finish_single(
            gemini_response(vec![number(1.0), number(-2.5)], Some(3)),
            None,
        )
        .unwrap()
        .value;
    assert_eq!(output.model, "models/selected-model");
    assert_eq!(
        output.data[0].embedding,
        openai::EmbeddingVector::Base64("AACAPwAAIMA=".into())
    );
    assert!(
        prepared
            .response
            .finish_single(gemini_response(vec![number(1.0)], Some(3)), None)
            .is_ok()
    );
    assert!(
        prepared
            .response
            .finish_single(gemini_response(vec![number(1.0), number(2.0)], None), None)
            .is_err()
    );

    let mut source = openai_request(openai::EmbeddingInput::Texts(vec!["a".into(), "b".into()]));
    source.encoding_format = None;
    let prepared = openai_to_gemini_batch(source, "selected-model")
        .unwrap()
        .value;
    assert_eq!(
        prepared.response.encoding_format,
        openai::EmbeddingEncodingFormat::Float
    );
    let one = gemini_response(vec![number(1.0), number(2.0)], Some(4));
    let mut batch = gemini_embeddings::BatchEmbedContentsResponseBody::builder().build();
    batch.embeddings = Some(vec![one.embedding.clone().unwrap()]);
    batch.usage_metadata = one.usage_metadata;
    assert!(prepared.response.finish_batch(batch.clone(), None).is_ok());
    batch
        .embeddings
        .as_mut()
        .unwrap()
        .push(one.embedding.unwrap());
    let converted = prepared.response.finish_batch(batch, None).unwrap().value;
    assert_eq!(converted.data.len(), 2);
    assert_eq!(converted.data[0].index, 0);
    assert_eq!(converted.data[1].index, 1);
    let roundtrip = openai_batch_response_to_gemini(converted).unwrap().value;
    assert_eq!(roundtrip.embeddings.unwrap().len(), 2);
}

#[test]
fn batch_dimension_presence_is_checked_in_both_orders() {
    for reverse in [false, true] {
        let mut first = gemini_request("models/one", "a");
        first.output_dimensionality = None;
        let second = gemini_request("models/one", "b");
        let mut requests = vec![first, second];
        if reverse {
            requests.reverse();
        }
        let request = gemini_embeddings::BatchEmbedContentsRequestBody::builder(requests).build();
        assert!(gemini_batch_to_openai(request, "target").is_ok());
    }
}

#[test]
fn role_and_shape_annotations_allow_empty_vectors_while_binary_decoding_errors_fail() {
    let mut request = gemini_request("models/one", "a");
    request.content.role = Some("user".into());
    let request = gemini_embeddings::BatchEmbedContentsRequestBody::builder(vec![request]).build();
    assert!(gemini_batch_to_openai(request, "target").is_ok());
    let supplement = OpenAiResponseSupplement {
        model: "target".into(),
        usage: None,
        encoding_format: openai::EmbeddingEncodingFormat::Float,
    };
    let mut response = gemini_response(vec![number(1.0), number(2.0)], Some(1));
    response.embedding.as_mut().unwrap().shape = Some(vec![1, 2]);
    assert!(gemini_single_response_to_openai(response, &supplement).is_ok());
    for vector in [
        openai::EmbeddingVector::Floats(vec![]),
        openai::EmbeddingVector::Base64("".into()),
    ] {
        let mut response = openai_response();
        response.data[0].embedding = vector;
        let converted = openai_single_response_to_gemini(response).unwrap().value;
        assert!(converted.embedding.unwrap().values.unwrap().is_empty());
    }
    for vector in [
        openai::EmbeddingVector::Base64("AAAA".into()),
        openai::EmbeddingVector::Base64("AACAfw==".into()),
    ] {
        let mut response = openai_response();
        response.data[0].embedding = vector;
        assert!(openai_single_response_to_gemini(response).is_err());
    }
    let mut response = openai_response();
    response.data[0].embedding = openai::EmbeddingVector::Base64("AACAPwAAIMA=".into());
    let converted = openai_single_response_to_gemini(response).unwrap().value;
    assert_eq!(
        converted.embedding.unwrap().values.unwrap(),
        vec![number(1.0), number(-2.5)]
    );
    let mut response = openai_response();
    response.data[0].index = 1;
    assert!(openai_batch_response_to_gemini(response).is_err());
}

#[test]
fn semantic_config_and_usage_conflicts_are_not_silently_discarded() {
    let mut request = gemini_request("models/one", "a");
    request.task_type = Some(gemini_embeddings::GeminiTaskType::RetrievalQuery);
    assert!(
        gemini_batch_to_openai(
            gemini_embeddings::BatchEmbedContentsRequestBody::builder(vec![request]).build(),
            "target"
        )
        .is_ok()
    );
    let mut request = gemini_request("models/one", "a");
    request.title = Some("title".into());
    assert!(
        gemini_batch_to_openai(
            gemini_embeddings::BatchEmbedContentsRequestBody::builder(vec![request]).build(),
            "target"
        )
        .is_ok()
    );
    let request = gemini_request("models/one", "a");
    let mut single = gemini_embeddings::EmbedContentRequestBody::builder(request.content).build();
    let mut config = gemini_embeddings::EmbedContentConfig::builder().build();
    config.auto_truncate = Some(false);
    single.embed_content_config = Some(config);
    assert!(gemini_single_to_openai(single, "target").is_ok());
    let response = gemini_response(vec![number(1.0), number(2.0)], Some(4));
    let facts = OpenAiResponseSupplement {
        model: "target".into(),
        usage: Some(OpenAiUsageFacts {
            prompt_tokens: 3,
            total_tokens: 3,
        }),
        encoding_format: openai::EmbeddingEncodingFormat::Float,
    };
    assert!(gemini_single_response_to_openai(response, &facts).is_err());
    let mut response = openai_response();
    response.usage.total_tokens = 2;
    assert!(openai_single_response_to_gemini(response).is_err());
}
