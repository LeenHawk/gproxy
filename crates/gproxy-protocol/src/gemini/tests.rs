use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::*;

#[test]
fn generate_and_stream_shapes_roundtrip_unknown_parts_and_fields() {
    roundtrip::<GenerateContentRequest>(json!({
        "model":"models/gemini-future",
        "contents":[{
            "role":"user",
            "parts":[
                {"text":"hello","futurePartMetadata":{"x":1}},
                {"futurePayload":{"nested":true}}
            ],
            "futureContent":1
        }],
        "generationConfig":{"temperature":0.2,"futureSampling":"adaptive"},
        "futureRequest":true
    }));

    roundtrip::<StreamGenerateContentChunk>(json!({
        "candidates":[{
            "content":{"role":"model","parts":[{"text":"answer"}]},
            "finishReason":"FUTURE_REASON","futureCandidate":{"x":1}
        }],
        "usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":1,"futureUsage":9},
        "responseId":"response_1","futureResponse":"kept"
    }));
    roundtrip::<Content>(json!({
        "role":"user",
        "parts":[{
            "inlineData":{"mimeType":"image/png","data":"AA=="},
            "videoMetadata":{"startOffset":"0s","futureVideo":1},
            "futureInline":{"quality":"original"}
        }]
    }));

    let metadata = GroundingMetadata {
        grounding_chunks: vec![GroundingChunk {
            source: Some(GroundingChunkSource::Web {
                web: WebChunk::default(),
                rest: Default::default(),
            }),
            rest: Default::default(),
        }],
        grounding_supports: vec![GroundingSupport {
            segment: Some(Segment::default()),
            ..Default::default()
        }],
        retrieval_metadata: Some(RetrievalMetadata::default()),
        ..Default::default()
    };
    let wire = serde_json::to_value(metadata).expect("encode grounding metadata");
    assert_eq!(wire["groundingChunks"][0]["web"], json!({}));
    assert_eq!(wire["groundingSupports"][0]["segment"], json!({}));
    assert_eq!(wire["retrievalMetadata"], json!({}));
    assert_eq!(
        serde_json::to_value(UrlMetadata::default()).expect("encode URL metadata"),
        json!({})
    );
}

#[test]
fn models_and_count_tokens_keep_unknown_resource_data() {
    roundtrip::<Model>(json!({
        "name":"models/gemini-future","baseModelId":"gemini-future",
        "version":"1","displayName":"Gemini Future","description":"next",
        "inputTokenLimit":1000000,"outputTokenLimit":65536,
        "supportedGenerationMethods":["generateContent","futureMethod"],
        "futureModel":{"tier":"preview"}
    }));
    roundtrip::<CountTokensRequest>(json!({
        "model":"models/gemini-future",
        "contents":[{"role":"user","parts":[{"text":"count me"}]}],
        "futureCountOption":true
    }));
    roundtrip::<CountTokensResponse>(json!({
        "totalTokens":3,
        "promptTokensDetails":[{"modality":"TEXT","tokenCount":3,"futureDetail":1}],
        "futureCount":"kept"
    }));
}

#[test]
fn remaining_family_models_preserve_documented_and_future_fields() {
    roundtrip::<BatchEmbedContentsRequest>(json!({
        "requests":[{
            "model":"models/gemini-embedding-001",
            "content":{"parts":[{"text":"embed me"}]},
            "embedContentConfig":{
                "taskType":"RETRIEVAL_DOCUMENT",
                "outputDimensionality":256,
                "futureEmbeddingOption":true
            }
        }],
        "futureBatchOption":1
    }));
    roundtrip::<ImagenPredictResponse>(json!({
        "predictions":[{
            "bytesBase64Encoded":"AA==",
            "mimeType":"image/png",
            "futureImageField":"kept"
        }]
    }));
    roundtrip::<VeoOperation>(json!({
        "name":"models/veo/operations/op-1",
        "response":{"generateVideoResponse":{"generatedSamples":[{
            "video":{"uri":"https://example.invalid/v1beta/files/file-1:download"}
        }]}},
        "futureOperation":{"progress":100}
    }));
    roundtrip::<ListFilesResponse>(json!({
        "files":[{
            "name":"files/file-1",
            "mimeType":"video/mp4",
            "state":"ACTIVE",
            "futureFileField":{"x":1}
        }],
        "nextPageToken":"next"
    }));
    roundtrip::<GenerateContentRequest>(json!({
        "contents":[{"role":"user","parts":[{"text":"say hello"}]}],
        "generationConfig":{
            "responseModalities":["AUDIO"],
            "speechConfig":{
                "multiSpeakerVoiceConfig":{"speakerVoiceConfigs":[{
                    "speaker":"Alex",
                    "voiceConfig":{"prebuiltVoiceConfig":{"voiceName":"Kore"}}
                }]},
                "languageCode":"en-US"
            },
            "responseFormat":{"audio":{
                "mimeType":"AUDIO_WAV",
                "delivery":"INLINE",
                "sampleRate":24000
            }}
        }
    }));
}

fn roundtrip<T>(wire: Value)
where
    T: DeserializeOwned + Serialize,
{
    let decoded: T = serde_json::from_value(wire.clone()).expect("decode wire");
    assert_eq!(serde_json::to_value(decoded).expect("encode wire"), wire);
}
