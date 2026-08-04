use gproxy_protocol::openai::{PromptCacheBreakpoint, PromptCacheBreakpointMode};

#[test]
fn downstream_builders_are_checked_and_wire_macro_is_available() {
    let error = PromptCacheBreakpoint::builder().build().unwrap_err();
    assert_eq!(error.type_name(), "PromptCacheBreakpoint");
    assert_eq!(error.field(), "mode");

    let built = PromptCacheBreakpoint::builder()
        .mode(PromptCacheBreakpointMode::Explicit)
        .build()
        .unwrap();
    assert_eq!(built.mode, PromptCacheBreakpointMode::Explicit);

    let via_macro = gproxy_protocol::wire!(gproxy_protocol::openai::PromptCacheBreakpoint {
        mode: PromptCacheBreakpointMode::Explicit,
        extra: Default::default(),
    });
    assert_eq!(via_macro.mode, PromptCacheBreakpointMode::Explicit);
}
