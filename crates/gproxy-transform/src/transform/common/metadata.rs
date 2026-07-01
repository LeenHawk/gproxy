use std::collections::BTreeMap;

use serde_json::Value;

pub type JsonFields = BTreeMap<String, Value>;

pub fn empty_fields() -> JsonFields {
    JsonFields::new()
}

pub fn discard_fields(_: JsonFields) {}
