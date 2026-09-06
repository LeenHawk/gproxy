use rust_decimal::Decimal;
use serde_json::{Map, Value};

/// Flat v3 metrics and the dimensions unwrapped from a legacy envelope.
pub(super) type SplitMetrics = (Map<String, Value>, Map<String, Value>);

/// Split a v2 `metrics_json` object into v3 metrics and dimensions.
///
/// v2 stored two shapes side by side: flat top-level quantities, and a nested
/// `{ "dimensions": {...}, "quantities": {...} }` envelope written by the
/// dimensional-usage path. v3 keeps metrics as a flat map of decimals and
/// dimensions as a separate map, so the envelope is unwrapped here and every
/// metric value is checked to be a decimal before it is written.
pub(super) fn split_legacy(metrics: &Map<String, Value>) -> Result<SplitMetrics, String> {
    let mut flat = Map::new();
    let mut dimensions = Map::new();
    for (name, value) in metrics {
        match (name.as_str(), value) {
            ("quantities", Value::Object(quantities)) => {
                for (name, value) in quantities {
                    flat.insert(name.clone(), decimal(name, value)?);
                }
            }
            ("dimensions", Value::Object(nested)) => {
                dimensions.extend(nested.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            ("quantities" | "dimensions", Value::Null) => {}
            _ => {
                flat.insert(name.clone(), decimal(name, value)?);
            }
        }
    }
    Ok((flat, dimensions))
}

fn decimal(name: &str, value: &Value) -> Result<Value, String> {
    let text = match value {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => return Err(format!("metric `{name}` is not a number")),
    };
    text.parse::<Decimal>()
        .map(|_| Value::String(text))
        .map_err(|_| format!("metric `{name}` is not a decimal"))
}
