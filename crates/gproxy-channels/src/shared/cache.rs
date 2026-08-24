use serde_json::{Value, json};

const MAGIC: &[(&str, Option<&str>)] = &[
    (
        "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH",
        None,
    ),
    (
        "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF",
        Some("5m"),
    ),
    (
        "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY",
        Some("1h"),
    ),
];

pub(crate) fn claude(body: &mut Value) {
    crate::shared::claude::cache::sanitize(body);
    let mut remaining = 4_usize.saturating_sub(count(body, "cache_control"));
    visit_maps(body, &mut |map| {
        let Some(text) = map
            .get_mut("text")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
        else {
            return;
        };
        let (text, ttl, matched) = strip(text);
        if !matched {
            return;
        }
        map.insert("text".into(), Value::String(text));
        if remaining > 0 && !map.contains_key("cache_control") {
            map.insert(
                "cache_control".into(),
                ttl.map_or_else(
                    || json!({"type":"ephemeral"}),
                    |ttl| json!({"type":"ephemeral","ttl":ttl}),
                ),
            );
            remaining -= 1;
        }
    });
    crate::shared::claude::cache::sanitize(body);
}

pub(crate) fn openai(body: &mut Value) {
    let mut remaining = 4_usize.saturating_sub(count(body, "prompt_cache_breakpoint"));
    convert_string_content(body, &mut remaining);
    visit_maps(body, &mut |map| {
        for name in ["text", "input_text", "content"] {
            let Some(text) = map.get(name).and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            let (text, _, matched) = strip(text);
            if !matched {
                continue;
            }
            map.insert(name.into(), Value::String(text));
            if remaining > 0 && !map.contains_key("prompt_cache_breakpoint") {
                map.insert("prompt_cache_breakpoint".into(), json!({"mode":"explicit"}));
                remaining -= 1;
            }
        }
    });
}

fn convert_string_content(value: &mut Value, remaining: &mut usize) {
    match value {
        Value::Array(values) => {
            for value in values {
                convert_string_content(value, remaining);
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(text)) = map.get_mut("content") {
                let (clean, _, matched) = strip(std::mem::take(text));
                if matched {
                    let mut block = json!({"type":"text","text":clean});
                    if *remaining > 0 {
                        block["prompt_cache_breakpoint"] = json!({"mode":"explicit"});
                        *remaining -= 1;
                    }
                    map.insert("content".into(), Value::Array(vec![block]));
                }
            }
            for value in map.values_mut() {
                convert_string_content(value, remaining);
            }
        }
        _ => {}
    }
}

fn strip(mut text: String) -> (String, Option<&'static str>, bool) {
    let mut ttl = None;
    let mut matched = false;
    for (token, token_ttl) in MAGIC {
        if text.contains(token) {
            text = text.replace(token, "");
            ttl = ttl.or(*token_ttl);
            matched = true;
        }
    }
    (text, ttl, matched)
}

fn visit_maps(value: &mut Value, visit: &mut impl FnMut(&mut serde_json::Map<String, Value>)) {
    match value {
        Value::Array(values) => {
            for value in values {
                visit_maps(value, visit);
            }
        }
        Value::Object(map) => {
            visit(map);
            for value in map.values_mut() {
                visit_maps(value, visit);
            }
        }
        _ => {}
    }
}

fn count(value: &Value, name: &str) -> usize {
    match value {
        Value::Array(values) => values.iter().map(|value| count(value, name)).sum(),
        Value::Object(map) => {
            usize::from(map.contains_key(name))
                + map.values().map(|value| count(value, name)).sum::<usize>()
        }
        _ => 0,
    }
}
