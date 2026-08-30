use serde_json::{Value, json};

const MAGIC_TRIGGER_AUTO_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_TRIGGER_5M_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
const MAGIC_TRIGGER_1H_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";

const MAGIC: &[(&str, Option<&str>)] = &[
    (MAGIC_TRIGGER_AUTO_ID, None),
    (MAGIC_TRIGGER_5M_ID, Some("5m")),
    (MAGIC_TRIGGER_1H_ID, Some("1h")),
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

pub(crate) fn strip_magic(text: &mut String) -> bool {
    let (clean, _, matched) = strip(std::mem::take(text));
    *text = clean;
    matched
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
