//! Offline token counting for provider-native LLM request bodies.

mod count;
mod extract;
mod model;
#[cfg(feature = "hf-registry")]
mod registry;
mod types;

pub use count::{RegistryHandle, count, count_detailed, count_text, try_count};
pub use extract::{harvest, try_harvest};
pub use model::is_gpt_family;
#[cfg(feature = "hf-registry")]
pub use registry::*;
pub use types::{CountError, CountMethod, CountResult, CountWarning};

#[cfg(test)]
mod tests;
