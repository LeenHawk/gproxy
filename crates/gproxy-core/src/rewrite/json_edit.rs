use super::{RewriteError, RuleAction};
use crate::PathSegment;
use serde_json::{Value, json};

/// Report actual mutations without cloning the document to compare afterward.
pub(super) fn apply(
    current: &mut Value,
    path: &[PathSegment],
    action: &RuleAction,
) -> Result<bool, RewriteError> {
    let Some((segment, rest)) = path.split_first() else {
        return match action {
            RuleAction::Set(value) => {
                let changed = current != value;
                if changed {
                    *current = value.clone();
                }
                Ok(changed)
            }
            RuleAction::Merge(value) => merge(current, value),
            _ => unreachable!("delete handled by parent"),
        };
    };
    let delete = matches!(action, RuleAction::Delete);
    let mut changed = false;
    if current.is_null() && !delete {
        *current = match segment {
            PathSegment::Index(_) => json!([]),
            _ => json!({}),
        };
        changed = true;
    }
    match segment {
        PathSegment::Key(key) => {
            if delete {
                if let Some(object) = current.as_object_mut() {
                    if rest.is_empty() {
                        changed |= object.remove(key).is_some();
                    } else if let Some(child) = object.get_mut(key) {
                        changed |= apply(child, rest, action)?;
                    }
                }
            } else {
                let object = current.as_object_mut().ok_or_else(|| {
                    RewriteError::InvalidJson(format!("expected object at {key}"))
                })?;
                changed |= !object.contains_key(key);
                changed |= apply(
                    object.entry(key.clone()).or_insert(Value::Null),
                    rest,
                    action,
                )?;
            }
        }
        PathSegment::Index(index) => {
            if delete {
                if let Some(array) = current.as_array_mut()
                    && *index < array.len()
                {
                    if rest.is_empty() {
                        array.remove(*index);
                        changed = true;
                    } else {
                        changed |= apply(&mut array[*index], rest, action)?;
                    }
                }
            } else {
                let array = current
                    .as_array_mut()
                    .ok_or_else(|| RewriteError::InvalidJson("expected array".into()))?;
                if *index > array.len() {
                    return Err(RewriteError::InvalidJson(
                        "array index out of bounds".into(),
                    ));
                }
                if *index == array.len() {
                    array.push(Value::Null);
                    changed = true;
                }
                changed |= apply(&mut array[*index], rest, action)?;
            }
        }
        PathSegment::Wildcard => match current {
            Value::Array(array) => {
                if delete && rest.is_empty() {
                    changed |= !array.is_empty();
                    array.clear();
                } else {
                    for child in array {
                        changed |= apply(child, rest, action)?;
                    }
                }
            }
            Value::Object(object) => {
                if delete && rest.is_empty() {
                    changed |= !object.is_empty();
                    object.clear();
                } else {
                    for child in object.values_mut() {
                        changed |= apply(child, rest, action)?;
                    }
                }
            }
            _ if delete => {}
            _ => {
                return Err(RewriteError::InvalidJson(
                    "wildcard requires object or array".into(),
                ));
            }
        },
    }
    Ok(changed)
}
fn merge(current: &mut Value, value: &Value) -> Result<bool, RewriteError> {
    let mut changed = false;
    if current.is_null() {
        *current = json!({});
        changed = true;
    }
    let object = current
        .as_object_mut()
        .ok_or_else(|| RewriteError::InvalidJson("merge target must be an object".into()))?;
    for (key, value) in value.as_object().expect("compiled merge object") {
        if value.is_object() && object.get(key).is_some_and(Value::is_object) {
            changed |= merge(object.get_mut(key).unwrap(), value)?;
        } else if object.get(key) != Some(value) {
            object.insert(key.clone(), value.clone());
            changed = true;
        }
    }
    Ok(changed)
}
