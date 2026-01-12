use toml_edit::{Array, Item, Value};

use crate::providers::credential_status::CredentialStatusList;

pub(crate) fn states_to_item(states: &CredentialStatusList) -> Option<Item> {
    if states.0.is_empty() {
        return None;
    }
    let mut list = Array::new();
    for entry in &states.0 {
        let mut tuple = Array::new();
        tuple.push(entry.0.as_str());
        tuple.push(entry.1.as_str());
        tuple.push(entry.2);
        list.push(Value::Array(tuple));
    }
    Some(Item::Value(Value::Array(list)))
}
