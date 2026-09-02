pub(crate) fn normalize(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(kind) = map.get_mut("type") {
                normalize_type(kind);
            }
            map.values_mut().for_each(normalize);
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(normalize),
        _ => {}
    }
}

fn normalize_type(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(kind) => kind.make_ascii_lowercase(),
        serde_json::Value::Array(values) => values.iter_mut().for_each(normalize_type),
        _ => {}
    }
}
