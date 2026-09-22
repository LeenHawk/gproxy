use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

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
    let sent = existing
        .and_then(|index| blocks[index].get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let field = |name| billing_field(sent, name);
    let entrypoint = field("cc_entrypoint")
        .filter(|value| is_token(value))
        .unwrap_or("cli");
    let mut text = format!(
        "x-anthropic-billing-header: cc_version={}.{suffix}; cc_entrypoint={entrypoint}; cch=00000;",
        super::auth::CLI_VERSION,
    );
    if let Some(workload) = field("cc_workload").filter(|value| is_token(value)) {
        text.push_str(&format!(" cc_workload={workload};"));
    }
    if field("cc_is_subagent") == Some("true") {
        text.push_str(" cc_is_subagent=true;");
    }
    if let Some(request) = field("cc_prev_req").filter(|value| is_request_id(value)) {
        text.push_str(&format!(" cc_prev_req={request};"));
    }
    if let Some(prompt) = field("cc_prompt_id").filter(|value| is_uuid(value)) {
        text.push_str(&format!(" cc_prompt_id={prompt};"));
    }
    if let Some(origin) = field("cc_turn_origin").filter(|value| is_turn_origin(value)) {
        text.push_str(&format!(" cc_turn_origin={origin};"));
    }
    let billing = json!({"type": "text", "text": text});
    if let Some(index) = existing {
        blocks[index] = billing;
    } else {
        blocks.insert(0, billing);
    }
}

fn billing_field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.strip_prefix("x-anthropic-billing-header:")
        .unwrap_or(text)
        .split(';')
        .map(str::trim)
        .find_map(|field| field.strip_prefix(name)?.strip_prefix('='))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn is_turn_origin(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

fn is_request_id(value: &str) -> bool {
    value
        .strip_prefix("req_")
        .is_some_and(|rest| (1..=36).contains(&rest.len()) && is_token(rest))
}

fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
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
    let code_units = text.encode_utf16().collect::<Vec<_>>();
    let selected = [4_usize, 7, 20]
        .into_iter()
        .map(|index| code_units.get(index).copied().unwrap_or(u16::from(b'0')))
        .collect::<Vec<_>>();
    let selected = String::from_utf16_lossy(&selected);
    let mut hasher = Sha256::new();
    hasher.update(SUFFIX_SALT.as_bytes());
    hasher.update(selected.as_bytes());
    hasher.update(super::auth::CLI_VERSION.as_bytes());
    let digest = hasher.finalize();
    format!("{:02x}{:02x}", digest[0], digest[1])
        .chars()
        .take(3)
        .collect()
}
