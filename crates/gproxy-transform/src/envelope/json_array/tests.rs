use serde_json::json;

use super::*;
use crate::ResponseStream;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, StreamFraming};

fn stream(kind: Kind) -> OperationKey {
    OperationKey::content(Operation::StreamGenerateContent, kind)
}

fn append(output: &mut Vec<u8>, chunks: Vec<Bytes>) {
    for chunk in chunks {
        output.extend_from_slice(&chunk);
    }
}

#[test]
fn decodes_incremental_array() {
    let input = b" \n[{\"text\":\"a,]}\"}, [1,true,null], 42]\t";
    let mut decoder = JsonArrayDecoder::default();
    let mut frames = Vec::new();
    for byte in input.chunks(1) {
        frames.extend(decoder.push(byte).unwrap());
    }
    frames.extend(decoder.finish().unwrap());
    let values = frames
        .iter()
        .map(|frame| serde_json::from_str::<Value>(&frame.data).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![json!({"text": "a,]}"}), json!([1, true, null]), json!(42)]
    );

    let mut decoder = JsonArrayDecoder::default();
    assert_eq!(decoder.push(b"[{},true]").unwrap().len(), 2);
    assert!(decoder.finish().unwrap().is_empty());
    let mut decoder = JsonArrayDecoder::default();
    assert!(decoder.push(b"[]").unwrap().is_empty());
    assert!(decoder.finish().unwrap().is_empty());
}

#[test]
fn rejects_invalid_framing_and_truncation() {
    for input in [
        b"{}".as_slice(),
        b"[1 2]",
        b"[1,]",
        b"[1,,2]",
        b"[1]x",
        b"[[DONE]]",
        b"[1\x0b]",
    ] {
        let mut decoder = JsonArrayDecoder::default();
        assert!(matches!(
            decoder.push(input),
            Err(TransformError::InvalidShape { .. })
        ));
    }
    for input in [b"".as_slice(), b" ", b"[", b"[{\"a\":", b"[1,"] {
        let mut decoder = JsonArrayDecoder::default();
        let _ = decoder.push(input).unwrap();
        assert!(matches!(
            decoder.finish(),
            Err(TransformError::IncompleteStream)
        ));
    }

    let mut large_batch = b"[0,1,2,".to_vec();
    large_batch.resize(MAX_BUFFER_BYTES + 128, b' ');
    large_batch.extend_from_slice(b"3]");
    let mut decoder = JsonArrayDecoder::default();
    assert_eq!(decoder.push(&large_batch).unwrap().len(), 4);
    assert!(decoder.finish().unwrap().is_empty());

    let mut oversized_element = b"[\"".to_vec();
    oversized_element.resize(MAX_BUFFER_BYTES + 2, b'x');
    let mut decoder = JsonArrayDecoder::default();
    assert!(matches!(
        decoder.push(&oversized_element),
        Err(TransformError::InvalidShape { .. })
    ));
}

#[test]
fn encodes_array_and_rejects_done() {
    let mut empty = JsonArrayEncoder::default();
    assert_eq!(empty.finish().unwrap(), Bytes::from_static(b"[]"));
    assert!(matches!(
        empty.finish(),
        Err(TransformError::InvalidShape { .. })
    ));

    let mut encoder = JsonArrayEncoder::default();
    assert_eq!(
        encoder.push(" {\"a\":1} ").unwrap(),
        Bytes::from_static(b"[{\"a\":1}")
    );
    assert_eq!(encoder.push("2").unwrap(), Bytes::from_static(b",2"));
    assert_eq!(encoder.finish().unwrap(), Bytes::from_static(b"]"));
    assert!(matches!(
        encoder.push("3"),
        Err(TransformError::InvalidShape { .. })
    ));

    let mut encoder = JsonArrayEncoder::default();
    assert!(matches!(
        encoder.push("[DONE]"),
        Err(TransformError::InvalidShape { .. })
    ));

    let key = stream(Kind::GeminiGenerateContent);
    let mut reframer =
        ResponseStream::new_framed(key, key, StreamFraming::JsonArray, StreamFraming::Sse).unwrap();
    let mut output = reframer
        .push(Bytes::from_static(b"data: {\"candidates\":[]}\n\n"))
        .unwrap();
    output.extend(reframer.finish().unwrap());
    assert_eq!(
        output.into_iter().flatten().collect::<Vec<_>>(),
        br#"[{"candidates":[]}]"#
    );
}

#[test]
fn gemini_array_to_chat_sse_pair_closes_with_done() {
    let wire = concat!(
        "[{\"responseId\":\"gemini_1\",\"modelVersion\":\"gemini\",",
        "\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",",
        "\"parts\":[{\"text\":\"ok\"}]},\"finishReason\":\"STOP\"}],",
        "\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,",
        "\"totalTokenCount\":2}}]"
    );
    let mut stream = ResponseStream::new_framed(
        stream(Kind::OpenAiChat),
        stream(Kind::GeminiGenerateContent),
        StreamFraming::Sse,
        StreamFraming::JsonArray,
    )
    .unwrap();
    let mut output = Vec::new();
    for chunk in wire.as_bytes().chunks(11) {
        append(
            &mut output,
            stream.push(Bytes::copy_from_slice(chunk)).unwrap(),
        );
    }
    append(&mut output, stream.finish().unwrap());
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("data: {"));
    assert!(output.contains("\"content\":\"ok\""));
    assert!(output.ends_with("data: [DONE]\n\n"));
}

#[test]
fn chat_sse_to_gemini_array_pair_emits_plain_json() {
    let wire = concat!(
        "data: {\"id\":\"chat_1\",\"object\":\"chat.completion.chunk\",",
        "\"created\":0,\"model\":\"gpt\",\"choices\":[{\"index\":0,",
        "\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},",
        "\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let mut stream = ResponseStream::new_framed(
        stream(Kind::GeminiGenerateContent),
        stream(Kind::OpenAiChat),
        StreamFraming::JsonArray,
        StreamFraming::Sse,
    )
    .unwrap();
    let mut output = Vec::new();
    for chunk in wire.as_bytes().chunks(13) {
        append(
            &mut output,
            stream.push(Bytes::copy_from_slice(chunk)).unwrap(),
        );
    }
    append(&mut output, stream.finish().unwrap());
    let text = String::from_utf8(output.clone()).unwrap();
    assert!(!text.contains("data:"));
    assert!(!text.contains("[DONE]"));
    let responses: Vec<gproxy_protocol::gemini::GenerateContentResponse> =
        serde_json::from_slice(&output).unwrap();
    assert_eq!(responses.len(), 1);
    assert!(matches!(
        responses[0].candidates[0].finish_reason,
        Some(gproxy_protocol::gemini::FinishReason::Known(
            gproxy_protocol::gemini::FinishReasonKnown::Stop
        ))
    ));
}
