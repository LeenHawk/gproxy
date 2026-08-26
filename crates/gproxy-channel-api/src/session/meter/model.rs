use gproxy_protocol::openai::common::OpenAiModelId;
use gproxy_protocol::openai::realtime::RealtimeSession;

pub(super) fn transcription(session: &RealtimeSession) -> Option<String> {
    session
        .audio
        .as_ref()?
        .input
        .as_ref()?
        .transcription
        .as_ref()?
        .model
        .clone()
}

pub(super) fn name(model: &OpenAiModelId) -> Option<String> {
    serde_json::to_value(model)
        .ok()?
        .as_str()
        .map(str::to_owned)
}
