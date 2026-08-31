use bytes::Bytes;
use gproxy_protocol::ContentGenerationKind as Kind;

use crate::{BufferedResponse, ResponseCollector};

#[test]
fn chat_collector_keeps_all_choices_refusal_and_legacy_calls() {
    let mut collector = ResponseCollector::new(Kind::OpenAiChat).unwrap();
    collector
        .push(Bytes::from_static(
            b"data: {\"id\":\"multi\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt\",\"choices\":[{\"index\":1,\"delta\":{\"content\":\"second\",\"refusal\":\"no\"},\"finish_reason\":\"stop\"},{\"index\":0,\"delta\":{\"function_call\":{\"name\":\"legacy\",\"arguments\":\"{}\"}},\"finish_reason\":\"function_call\"}]}\n\ndata: [DONE]\n\n",
        ))
        .unwrap();
    let BufferedResponse::OpenAiChat(response) = collector.finish().unwrap() else {
        panic!("wrong response family");
    };
    assert_eq!(response.choices.len(), 2);
    assert_eq!(response.choices[0].index, 0);
    assert_eq!(
        response.choices[0]
            .message
            .function_call
            .as_ref()
            .unwrap()
            .name,
        "legacy"
    );
    assert_eq!(response.choices[1].index, 1);
    assert_eq!(
        response.choices[1].message.content.as_deref(),
        Some("second")
    );
    assert_eq!(response.choices[1].message.refusal.as_deref(), Some("no"));
}
