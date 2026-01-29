use http::HeaderMap;
use serde::Serialize;

pub fn headers_to_json(headers: &HeaderMap) -> String {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        if let Ok(value) = value.to_str() {
            map.insert(name.to_string(), serde_json::Value::String(value.to_string()));
        }
    }
    serde_json::Value::Object(map).to_string()
}

pub fn json_body_to_string<T: Serialize>(body: &T) -> String {
    serde_json::to_string(body).unwrap_or_default()
}
