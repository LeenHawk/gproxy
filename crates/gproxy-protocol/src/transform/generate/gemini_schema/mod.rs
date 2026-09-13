//! Explicit Gemini OpenAPI-subset / JSON Schema mappings. This is a schema
//! conversion, not a guarantee that a particular model enforces every keyword.
//! Callers must separately check the selected endpoint's supported schema subset.

mod from_json;
mod to_json;

pub use from_json::from_json;
pub use to_json::to_json;

use crate::transform::{TransformError, TransformErrorKind};

/// Bound recursive schema conversion independently of the HTTP decoder's limits.
#[derive(Debug, Clone, Copy)]
pub struct SchemaLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
}

impl Default for SchemaLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 16_384,
        }
    }
}

struct Budget {
    limits: SchemaLimits,
    nodes: usize,
}

impl Budget {
    fn enter(&mut self, depth: usize, path: &str) -> Result<(), TransformError> {
        if depth > self.limits.max_depth || self.nodes >= self.limits.max_nodes {
            return Err(TransformError::new(
                TransformErrorKind::Limit,
                path,
                "schema conversion budget exceeded",
            ));
        }
        self.nodes += 1;
        Ok(())
    }
}
