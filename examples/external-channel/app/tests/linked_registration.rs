use gproxy_example_channel as _;

#[test]
fn external_crate_is_retained_and_registered() {
    let registry = gproxy::channel::registry::ChannelRegistry::with_builtin_and_linked()
        .expect("linked registry");

    let channel = registry
        .get("example-openai")
        .expect("external channel was not linked");
    assert_eq!(channel.id(), "example-openai");

    let entry = registry
        .catalog()
        .into_iter()
        .find(|entry| entry.metadata.id == "example-openai")
        .expect("external channel missing from catalog");
    assert_eq!(entry.source, gproxy::channel::ChannelSource::External);
}
