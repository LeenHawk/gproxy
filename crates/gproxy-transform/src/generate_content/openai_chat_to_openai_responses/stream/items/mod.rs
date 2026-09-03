mod complete;
mod content;
mod tools;
mod typed;
mod typed_tools;

use crate::TransformError;

pub(super) fn suffix(current: &str, full: &str, name: &str) -> Result<String, TransformError> {
    full.strip_prefix(current)
        .map(str::to_owned)
        .ok_or_else(|| {
            TransformError::shape(
                "Responses stream",
                format!("{name} done value does not extend prior deltas"),
            )
        })
}
