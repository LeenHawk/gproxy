use crate::{WireRequest, transform::TransformError};

pub(super) fn template(request: &WireRequest<()>) -> Result<(), TransformError> {
    if request.method != http::Method::GET {
        return Err(TransformError::shape(
            "request.method",
            "model listing requires GET",
        ));
    }
    if request.query.as_ref().is_some_and(|v| !v.is_empty()) {
        return Err(TransformError::shape(
            "request.query",
            "provide model pagination through the typed query argument",
        ));
    }
    Ok(())
}
pub(super) fn page_size(value: Option<i64>, max: i64) -> Result<(), TransformError> {
    if value.is_some_and(|n| n <= 0 || n > max) {
        return Err(TransformError::shape(
            "query.page_size",
            format!("expected 1 through {max}"),
        ));
    }
    Ok(())
}
pub(super) fn encode(pairs: &[(&str, String)]) -> Option<String> {
    if pairs.is_empty() {
        return None;
    }
    Some(
        pairs
            .iter()
            .map(|(key, value)| format!("{key}={}", component(value)))
            .collect::<Vec<_>>()
            .join("&"),
    )
}
fn component(value: &str) -> String {
    use std::fmt::Write;
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            result.push(char::from(byte));
        } else {
            write!(&mut result, "%{byte:02X}").expect("String formatting is infallible");
        }
    }
    result
}
