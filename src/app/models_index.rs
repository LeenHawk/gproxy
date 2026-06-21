//! Compiled `provider_models` expansion (§8-B): manual model rows plus suffix
//! variants. `variants_json` is either a plain suffix array `["-thinking"]`
//! (base stays exposed) or `{expose_base: bool, suffixes: [..]}` (hide the base
//! when `expose_base=false`). Unparsable config warns and degrades to "no
//! variants" — bad rows must never take the snapshot down.

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
        let (expose_base, suffixes) = parse_variants(row);
        if expose_base {
            out.exposed.push(ExposedModel {
                full_id: row.model_id.clone(),
                base_id: row.model_id.clone(),
                display_name: row.display_name.clone(),
            });
        }
        for suffix in suffixes {
            let full = format!("{}{}", row.model_id, suffix);
            out.variant_base.insert(full.clone(), row.model_id.clone());
            out.exposed.push(ExposedModel {
                full_id: full,
                base_id: row.model_id.clone(),
                display_name: row.display_name.clone(),
            });
        }
    }
    out
}

/// `(expose_base, suffixes)` from `variants_json`. Both wire forms; anything
/// else warns and is treated as "no variants".
fn parse_variants(row: &ProviderModel) -> (bool, Vec<String>) {
    let Some(v) = &row.variants_json else {
        return (true, Vec::new());
    };
    match v {
        Value::Array(items) => match suffix_list(items) {
            Some(suffixes) => (true, suffixes),
            None => warn_unparsable(row),
        },
        Value::Object(obj) => {
            let expose_base = match obj.get("expose_base") {
                None => true,
                Some(Value::Bool(b)) => *b,
                Some(_) => return warn_unparsable(row),
            };
            match obj.get("suffixes").and_then(Value::as_array) {
                Some(items) => match suffix_list(items) {
                    Some(suffixes) => (expose_base, suffixes),
                    None => warn_unparsable(row),
                },
                None => warn_unparsable(row),
            }
        }
        _ => warn_unparsable(row),
    }
}

fn suffix_list(items: &[Value]) -> Option<Vec<String>> {
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
            pricing_json: None,
            variants_json: variants,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        })
    }

    #[test]
    fn variants_compile_simple_and_object_forms() {
        let rows = vec![
            row(1, "deepseek-v4", Some(json!(["-thinking"]))),
            row(
                2,
                "qwen-max",
                Some(json!({ "expose_base": false, "suffixes": ["-32k", "-128k"] })),
            ),
            row(3, "plain", None),
            row(4, "broken", Some(json!("nope"))), // unparsable → no variants
        ];
        let c = compile(&rows);

        let ids: Vec<&str> = c.exposed.iter().map(|m| m.full_id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "deepseek-v4",
                "deepseek-v4-thinking",
                "qwen-max-32k", // base hidden
                "qwen-max-128k",
                "plain",
                "broken",
            ]
        );
        // variant entries map to base; base entries are absent from the index
        assert_eq!(
            c.variant_base.get("deepseek-v4-thinking").unwrap(),
            "deepseek-v4"
        );
        assert_eq!(c.variant_base.get("qwen-max-32k").unwrap(), "qwen-max");
        assert!(!c.variant_base.contains_key("deepseek-v4"));
        assert_eq!(c.variant_base.len(), 3);
        // exposed entries carry the upstream base id
        assert!(c.exposed.iter().all(|m| !m.base_id.is_empty()));
        assert_eq!(
            c.exposed
                .iter()
                .find(|m| m.full_id == "qwen-max-128k")
                .unwrap()
                .base_id,
            "qwen-max"
        );
    }
}
