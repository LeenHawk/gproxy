use bytes::Bytes;
use serde_json::json;

const MODELS: &[&str] = &[
    "tasklet-standard",
    "tasklet-advanced",
    "tasklet-expert",
    "tasklet-genius",
    "gpt_5_6_luna",
    "gpt_5_6_terra",
    "gpt_5_6_sol",
    "gpt_5_5",
    "gpt_5_5_fast",
    "claude_haiku_4_5",
    "claude_sonnet_4_6",
    "claude_sonnet_5",
    "claude_opus_4_6",
    "claude_opus_4_7",
    "claude_opus_4_8",
    "claude_opus_4_8_fast",
    "claude_fable_5",
    "gemini_flash_3_preview",
    "gemini_flash_3_5",
    "gemini_flash_lite_3_1",
    "gemini_pro_3_1_preview",
    "grok_4_5",
    "kimi_k3",
    "muse_spark_1_1",
];

pub fn catalog() -> Bytes {
    let created = crate::util::time::unix_now();
    let data = MODELS
        .iter()
        .map(|id| json!({"id": id, "object": "model", "created": created, "owned_by": "tasklet"}))
        .collect::<Vec<_>>();
    Bytes::from(serde_json::to_vec(&json!({"object": "list", "data": data})).unwrap())
}
