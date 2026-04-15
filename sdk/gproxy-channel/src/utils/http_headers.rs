use http::{HeaderValue, header::HeaderName};

use crate::response::UpstreamError;

pub fn replace_header<K, V>(
    builder: &mut http::request::Builder,
    key: K,
    value: V,
) -> Result<(), UpstreamError>
where
    K: TryInto<HeaderName>,
    K::Error: std::fmt::Display,
    V: TryInto<HeaderValue>,
    V::Error: std::fmt::Display,
{
    let name = key
        .try_into()
        .map_err(|e| UpstreamError::RequestBuild(format!("invalid header name: {e}")))?;
    let value = value.try_into().map_err(|e| {
        UpstreamError::RequestBuild(format!("invalid value for header {}: {e}", name.as_str()))
    })?;
    let headers = builder.headers_mut().ok_or_else(|| {
        UpstreamError::RequestBuild("request builder missing headers".to_string())
    })?;
    headers.insert(name, value);
    Ok(())
}
