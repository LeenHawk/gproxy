use schemars::{
    JsonSchema, SchemaGenerator,
    generate::{Contract, SchemaSettings},
};
use serde_json::{Map, Value, json};

/// Serde's input and output contracts can differ. Keep their definitions in
/// separate namespaces instead of silently using request rules for responses.
pub struct Schemas {
    input: SchemaGenerator,
    output: SchemaGenerator,
}

impl Default for Schemas {
    fn default() -> Self {
        let input = SchemaSettings::draft2020_12();
        let mut output = input.clone();
        output.contract = Contract::Serialize;
        Self {
            input: input.into_generator(),
            output: output.into_generator(),
        }
    }
}

impl Schemas {
    pub fn input<T: JsonSchema>(&mut self) -> Value {
        qualify(self.input.subschema_for::<T>().to_value(), "Input")
    }

    pub fn output<T: JsonSchema>(&mut self) -> Value {
        qualify(self.output.subschema_for::<T>().to_value(), "Output")
    }

    pub fn query<T: JsonSchema>(&mut self) -> Vec<Value> {
        let schema = qualify(T::json_schema(&mut self.input).to_value(), "Input");
        let required = schema["required"].as_array();
        schema["properties"].as_object().expect("query schema must be an object")
            .iter().map(|(name, value)| json!({
                "in": "query", "name": name,
                "required": required.is_some_and(|fields| fields.contains(&json!(name))),
                "schema": value,
                "description": value.get("description").and_then(Value::as_str).unwrap_or_default(),
            })).collect()
    }

    pub fn definitions(self) -> Map<String, Value> {
        [("Input", self.input), ("Output", self.output)]
            .into_iter()
            .flat_map(|(prefix, mut generator)| {
                generator
                    .take_definitions(false)
                    .into_iter()
                    .map(move |(name, schema)| {
                        (format!("{prefix}_{name}"), qualify(schema, prefix))
                    })
            })
            .collect()
    }
}

fn qualify(mut value: Value, prefix: &str) -> Value {
    fn walk(value: &mut Value, prefix: &str) {
        match value {
            Value::Object(object) => {
                // `required_nullable` accepts null but rejects an absent field.
                // schemars(required) would also remove null, so carry the
                // wire marker to the enclosing object's required list instead.
                if let Some(Value::Object(properties)) = object.get_mut("properties") {
                    let required: Vec<_> = properties
                        .iter_mut()
                        .filter_map(|(name, property)| {
                            property
                                .as_object_mut()?
                                .remove("x-gproxy-required")
                                .and_then(|required| (required == true).then(|| json!(name)))
                        })
                        .collect();
                    if !required.is_empty() {
                        let fields = object
                            .entry("required")
                            .or_insert_with(|| json!([]))
                            .as_array_mut()
                            .unwrap();
                        for name in required {
                            if !fields.contains(&name) {
                                fields.push(name);
                            }
                        }
                    }
                }
                if let Some(Value::String(reference)) = object.get_mut("$ref")
                    && let Some(name) = reference.strip_prefix("#/$defs/")
                {
                    *reference = format!("#/components/schemas/{prefix}_{name}");
                }
                for value in object.values_mut() {
                    walk(value, prefix);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, prefix);
                }
            }
            _ => {}
        }
    }
    walk(&mut value, prefix);
    value
}

pub fn alternatives(mut schemas: Vec<Value>) -> Value {
    let mut unique = Vec::new();
    for schema in schemas.drain(..) {
        if !unique.contains(&schema) {
            unique.push(schema);
        }
    }
    assert!(!unique.is_empty(), "a payload must have a schema");
    if unique.len() == 1 {
        unique.pop().unwrap()
    } else {
        json!({"anyOf":unique})
    }
}
