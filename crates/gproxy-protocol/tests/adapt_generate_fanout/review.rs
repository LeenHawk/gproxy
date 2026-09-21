use super::*;
use gproxy_protocol::transform::identity::{IdentityRole, KnownIdPrefix};
#[test]
fn aggregate_response_id_obeys_the_explicit_client_generation_policy() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let mut target = setup(Dialect::OpenAiChat, Dialect::Claude);
    target.options.response_policy = target
        .options
        .response_policy
        .with_generated_prefix(IdentityRole::Response, KnownIdPrefix::Message);
    let mut prepared = ready(ChatViaClaudeFanout::prepare(
        chat_input(),
        target,
        &state,
        codec_limits(),
    ))
    .unwrap();
    assert!(
        prepared.response_id().starts_with("msg_"),
        "aggregate ignored selected generated prefix: {}",
        prepared.response_id()
    );
    let expected = prepared.response_id().to_owned();
    let host = Host::many(two("c"));
    let mut progress = FanoutProgress::default();
    let result =
        ready(prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, clock)).unwrap();
    assert_eq!(result.value.id, expected);
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn aggregate_policy_rejects_impossible_length_and_inconsistent_client_policies() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let mut target = setup(Dialect::OpenAiChat, Dialect::Claude);
    target.options.response_policy = target.options.response_policy.with_max_len(8);
    assert!(
        ready(ChatViaClaudeFanout::prepare(
            chat_input(),
            target,
            &state,
            codec_limits()
        ))
        .is_err(),
        "impossible aggregate ID policy accepted"
    );
    assert!(store.entries.lock().unwrap().is_empty());
}
