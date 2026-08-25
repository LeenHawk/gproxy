use http::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

pub(super) fn parse(value: Option<&Value>) -> Result<HeaderMap, String> {
    let Some(value) = value else {
        return Ok(HeaderMap::new());
    };
    if value == &Value::Bool(false) {
        return Ok(HeaderMap::new());
    }
    let object = value
        .as_object()
        .ok_or_else(|| "fingerprint headers must be an object or false".to_owned())?;
    let mut headers = HeaderMap::with_capacity(object.len());
    for (name, value) in object {
        let name = HeaderName::try_from(name).map_err(|_| "invalid fingerprint header name")?;
        let value = value
            .as_str()
            .ok_or("fingerprint header values must be strings")?;
        let value = HeaderValue::try_from(value).map_err(|_| "invalid fingerprint header value")?;
        headers.insert(name, value);
    }
    Ok(headers)
}
