use serde_json::{Map, Value};

pub(super) fn drop_openai_only(fields: &mut Map<String, Value>) {
    for name in [
        "background",
        "input_fidelity",
        "mask",
        "moderation",
        "output_compression",
        "output_format",
        "partial_images",
        "quality",
        "response_format",
        "stream",
        "style",
        "user",
    ] {
        fields.remove(name);
    }
}
