//! Explicit Gemini OpenAPI-subset / JSON Schema mappings. This is a schema
//! conversion, not a guarantee that a particular model enforces every keyword.
//! Callers must separately check the selected endpoint's supported schema subset.

mod from_json;
mod to_json;

pub use from_json::from_json;
pub use to_json::to_json;
