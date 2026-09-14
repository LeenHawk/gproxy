use super::*;
use gproxy_protocol::adapt::generate::chat_responses::{ChatViaResponses, ResponsesViaChat};

#[test]
fn custom_stream_rejection_precedes_reservation_and_buffered_synthesis_stays_available() {
    let store = Store::default();
    let mut access = state(&store);
    let mut target = selected();
    access.target = IdentityTarget::new("selected", Dialect::OpenAi)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    target.identities = GenerationIdentity::new(
        IdNamespace([81; 16]),
        IdNamespace([82; 16]),
        Dialect::OpenAiChat,
        Dialect::OpenAi,
    )
    .unwrap();
    let source = json!({"model":"source","stream":true,"messages":[{"role":"user","content":"edit"}],"tools":[{"type":"custom","custom":{"name":"edit"}}]});
    let result = ready(ChatViaResponses::prepare_stream(
        serde_json::from_value(source.clone()).unwrap(),
        StreamTarget {
            model: target.model.clone(),
            endpoint: target.endpoint.clone(),
            identities: target.identities.clone(),
        },
        settings(),
        &access,
    ));
    assert_eq!(result.err().unwrap().context(), "stream.custom_tools");
    assert!(store.entries.lock().unwrap().is_empty());
    assert!(
        ready(ChatViaResponses::prepare_for_stream_synthesis(
            serde_json::from_value(source).unwrap(),
            "selected",
            target.endpoint.clone(),
            target.identities,
            &access
        ))
        .is_err()
    );

    access.target = IdentityTarget::new("selected", Dialect::OpenAiChat)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    target.identities = GenerationIdentity::new(
        IdNamespace([83; 16]),
        IdNamespace([84; 16]),
        Dialect::OpenAi,
        Dialect::OpenAiChat,
    )
    .unwrap();
    let source = json!({"model":"source","stream":true,"input":"edit","tools":[{"type":"custom","name":"edit"}]});
    let result = ready(ResponsesViaChat::prepare_stream(
        serde_json::from_value(source.clone()).unwrap(),
        StreamTarget {
            model: target.model.clone(),
            endpoint: target.endpoint.clone(),
            identities: target.identities.clone(),
        },
        all_pairs::chat_response_context(),
        settings(),
        &access,
    ));
    assert_eq!(result.err().unwrap().context(), "stream.custom_tools");
    assert!(store.entries.lock().unwrap().is_empty());
    // A buffered Chat custom result can still be synthesized as native Responses events.
    let prepared = ready(ResponsesViaChat::prepare_for_stream_synthesis(
        serde_json::from_value(source).unwrap(),
        "selected",
        target.endpoint,
        target.identities,
        &access,
    ))
    .unwrap();
    assert_eq!(prepared.target_request().stream, Some(Some(false)));
}
