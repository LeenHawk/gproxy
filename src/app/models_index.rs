//! Compiled `provider_models` expansion (§8-B): manual model rows plus named
//! variants. `variants_json` is a bare name array `["gpt-image-2"]` (base stays
//! exposed) or `{expose_base: bool, variants: ["gpt-image-2"]}` (hide the base
//! when `expose_base=false`). Variant names are ABSOLUTE model ids, independent
//! of the base id. Unparsable config warns and degrades to "no variants" — bad
//! rows must never take the snapshot down.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::store::persistence::records::ProviderModel;

/// One exposed (listable) model id for a provider.
pub struct ExposedModel {
    /// e.g. `"deepseek-v4-thinking"`
    pub full_id: String,
    /// upstream id; `== full_id` for base entries.
    pub base_id: String,
    pub display_name: Option<String>,
}

/// Output of [`compile`]: list-side expansion + request-side strip index.
#[derive(Default)]
pub struct CompiledModels {
    /// Listable entries (enabled rows, variants applied, expose_base honored).
    pub exposed: Vec<ExposedModel>,
    /// variant full id → base id. Base ids are NOT in this map.
    pub variant_base: HashMap<String, String>,
}

/// Expand enabled rows into exposed entries + the variant→base index.
pub fn compile(rows: &[Arc<ProviderModel>]) -> CompiledModels {
    let mut out = CompiledModels::default();
    for row in rows.iter().filter(|r| r.enabled) {
        let (expose_base, names) = parse_variants(row);
        if expose_base {
            out.exposed.push(ExposedModel {
                full_id: row.model_id.clone(),
                base_id: row.model_id.clone(),
                display_name: row.display_name.clone(),
            });
        }
        for name in names {
            // Absolute name; skip empties and a name colliding with the base
            // (avoids a self-loop in variant_base / a duplicate exposed entry).
            if name.is_empty() || name == row.model_id {
                continue;
            }
            out.variant_base.insert(name.clone(), row.model_id.clone());
            out.exposed.push(ExposedModel {
                full_id: name,
                base_id: row.model_id.clone(),
                display_name: row.display_name.clone(),
            });
        }
    }
    out
}

/// `(expose_base, names)` from `variants_json`. Bare array → names (base stays
/// exposed). Object → `expose_base` (default true) + `variants` array. Anything
/// else warns and is treated as "no variants".
fn parse_variants(row: &ProviderModel) -> (bool, Vec<String>) {
    let Some(v) = &row.variants_json else {
        return (true, Vec::new());
    };
    match v {
        Value::Array(items) => match name_list(items) {
            Some(names) => (true, names),
            None => warn_unparsable(row),
        },
        Value::Object(obj) => {
            let expose_base = match obj.get("expose_base") {
                None => true,
                Some(Value::Bool(b)) => *b,
                Some(_) => return warn_unparsable(row),
            };
            match obj.get("variants").and_then(Value::as_array) {
                Some(items) => match name_list(items) {
                    Some(names) => (expose_base, names),
                    None => warn_unparsable(row),
                },
                None => warn_unparsable(row),
            }
        }
        _ => warn_unparsable(row),
    }
}

fn name_list(items: &[Value]) -> Option<Vec<String>> {
    items
        .iter()
        .map(|i| i.as_str().map(str::to_owned))
        .collect()
}

fn warn_unparsable(row: &ProviderModel) -> (bool, Vec<String>) {
    tracing::warn!(
        model_row = row.id,
        provider_id = row.provider_id,
        model_id = %row.model_id,
        "unparsable variants_json; treating as no variants"
    );
    (true, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: i64, model_id: &str, variants: Option<Value>) -> Arc<ProviderModel> {
        Arc::new(ProviderModel {
            id,
            provider_id: 1,
            model_id: model_id.into(),
            display_name: Some(format!("{model_id} (display)")),
            variants_json: variants,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        })
    }

    #[test]
    fn named_variants_compile_bare_and_object_forms() {
        let rows = vec![
            row(1, "gpt-5.5", Some(json!(["gpt-image-2"]))),
            row(
                2,
                "qwen-max",
                Some(json!({ "expose_base": false, "variants": ["qwen-fast", "qwen-max"] })),
            ),
            row(3, "plain", None),
            row(4, "broken", Some(json!("nope"))), // unparsable → no variants
        ];
        let c = compile(&rows);

        let ids: Vec<&str> = c.exposed.iter().map(|m| m.full_id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gpt-5.5",
                "gpt-image-2",
                "qwen-fast", // base hidden; "qwen-max" == base → skipped
                "plain",
                "broken",
            ]
        );
        // variant entries map to base; base / self-named entries are absent
        assert_eq!(c.variant_base.get("gpt-image-2").unwrap(), "gpt-5.5");
        assert_eq!(c.variant_base.get("qwen-fast").unwrap(), "qwen-max");
        assert!(!c.variant_base.contains_key("gpt-5.5"));
        assert!(!c.variant_base.contains_key("qwen-max")); // self-name skipped
        assert_eq!(c.variant_base.len(), 2);
        // exposed entries carry the upstream base id
        assert!(c.exposed.iter().all(|m| !m.base_id.is_empty()));
        assert_eq!(
            c.exposed
                .iter()
                .find(|m| m.full_id == "gpt-image-2")
                .unwrap()
                .base_id,
            "gpt-5.5"
        );
    }
}
