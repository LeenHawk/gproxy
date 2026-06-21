use std::collections::BTreeMap;

use serde_json::Value;

pub type ExtraFields = BTreeMap<String, Value>;
pub type JsonMap = BTreeMap<String, Value>;
