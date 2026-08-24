use gproxy_channel_api::Channel;
use gproxy_protocol::{ContentGenerationKind as C, Operation as O, OperationKey, WireFamily};

use super::super::KiroChannel;

fn family(operation: O, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

fn content(operation: O, kind: C) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_models_and_forced_stream_responses_for_all_four_sources() {
    let expected = [
        family(O::ListModels, WireFamily::OpenAi),
        family(O::ListModels, WireFamily::Claude),
        content(O::GenerateContent, C::OpenAiResponses),
        content(O::GenerateContent, C::OpenAiChat),
        content(O::GenerateContent, C::ClaudeMessages),
        content(O::GenerateContent, C::GeminiGenerateContent),
        content(O::StreamGenerateContent, C::OpenAiResponses),
        content(O::StreamGenerateContent, C::OpenAiChat),
        content(O::StreamGenerateContent, C::ClaudeMessages),
        content(O::StreamGenerateContent, C::GeminiGenerateContent),
    ];
    let supports = KiroChannel.descriptor().supports;
    assert_eq!(supports.len(), expected.len());
    assert!(
        expected
            .iter()
            .all(|key| supports.iter().any(|row| row.source == *key))
    );
    assert!(supports.iter().all(|row| {
        row.target == family(O::ListModels, WireFamily::OpenAi)
            || row.target == content(O::StreamGenerateContent, C::OpenAiResponses)
    }));
}
