//! AI Studio auth: api key carried as the `?key=` query parameter (Gemini
//! style), not a header.

use crate::channel::bulletins::common;

/// Append `key=<api_key>` to the (already allow-listed) query string.
pub(super) fn apply_query(query: Option<String>, key: &str) -> Option<String> {
    common::with_key_query(query, key)
}
