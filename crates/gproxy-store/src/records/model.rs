use std::collections::BTreeSet;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelVariants {
    pub expose_base: bool,
    pub names: Vec<String>,
}

pub fn parse_model_variants(value: Option<&Value>) -> Result<ModelVariants, String> {
    let Some(value) = value else {
        return Ok(ModelVariants {
            expose_base: true,
            names: Vec::new(),
        });
    };
    let (expose_base, values) = match value {
        Value::Array(values) => (true, values),
        Value::Object(object) => {
            let expose_base = match object.get("expose_base") {
                None => true,
                Some(Value::Bool(value)) => *value,
                Some(_) => return Err("expose_base must be a boolean".into()),
            };
            let values = object
                .get("variants")
                .and_then(Value::as_array)
                .ok_or("variants must be an array")?;
            (expose_base, values)
        }
        _ => return Err("variants must be an array or object".into()),
    };
    let mut seen = BTreeSet::new();
    let mut names = Vec::with_capacity(values.len());
    for value in values {
        let name = value
            .as_str()
            .filter(|name| !name.is_empty())
            .ok_or("variant names must be non-empty strings")?;
        if !seen.insert(name) {
            return Err(format!("duplicate variant `{name}`"));
        }
        names.push(name.to_owned());
    }
    Ok(ModelVariants { expose_base, names })
}
