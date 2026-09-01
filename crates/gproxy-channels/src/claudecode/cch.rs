use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const CLI_VERSION: &str = "2.1.252";
const SUFFIX_SALT: &str = "59cf53e54c78";

pub(super) fn inject(body: &mut Value, secret: &Value, session_id: &str) {
    let suffix = version_suffix(first_user_text(body));
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let user_id = json!({
        "device_id": super::auth::device_id(secret),
        "account_uuid": secret
            .get("account_uuid")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "session_id": session_id,
    })
    .to_string();
    let metadata = root
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    metadata
        .as_object_mut()
        .expect("metadata was made an object")
        .insert("user_id".into(), Value::String(user_id));

    let system = root
        .entry("system")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !system.is_array() {
        let previous = std::mem::take(system);
        *system = Value::Array(vec![previous]);
    }
    let blocks = system.as_array_mut().expect("system was made an array");
    let existing = blocks.iter().position(|block| {
        block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.starts_with("x-anthropic-billing-header:"))
    });
    let entrypoint = existing
        .and_then(|index| blocks[index].get("text"))
        .and_then(Value::as_str)
        .and_then(|text| billing_field(text, "cc_entrypoint"))
        .filter(|value| {
            value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .unwrap_or("cli");
    let billing = json!({
        "type": "text",
        "text": format!(
            "x-anthropic-billing-header: cc_version={CLI_VERSION}.{suffix}; cc_entrypoint={entrypoint}; cch=00000;"
        ),
    });
    if let Some(index) = existing {
        blocks[index] = billing;
    } else {
        blocks.insert(0, billing);
    }
}

fn billing_field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split(';')
        .map(str::trim)
        .find_map(|field| field.strip_prefix(name)?.strip_prefix('='))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn first_user_text(body: &Value) -> &str {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return "";
    };
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(content) = message.get("content") else {
            continue;
        };
        if let Some(text) = content.as_str() {
            return text;
        }
        if let Some(text) = content.as_array().and_then(|blocks| {
            blocks.iter().find_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| block.get("text").and_then(Value::as_str))
                    .flatten()
            })
        }) {
            return text;
        }
    }
    ""
}

fn version_suffix(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let selected = [4_usize, 7, 20]
        .into_iter()
        .map(|index| chars.get(index).copied().unwrap_or('0'))
        .collect::<String>();
    let mut hasher = Sha256::new();
    hasher.update(SUFFIX_SALT.as_bytes());
    hasher.update(selected.as_bytes());
    hasher.update(CLI_VERSION.as_bytes());
    let digest = hasher.finalize();
    format!("{:02x}{:02x}", digest[0], digest[1])
        .chars()
        .take(3)
        .collect()
}
