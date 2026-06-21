use crate::protocol::openai;

pub(in crate::transform::images) fn openai_model_string(
    model: Option<openai::OpenAiModelId>,
) -> Option<String> {
    model.and_then(|model| {
        serde_json::to_value(model)
            .ok()?
            .as_str()
            .map(str::to_owned)
    })
}
