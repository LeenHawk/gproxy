use gproxy_channel_api::ChannelError;
use http::{HeaderMap, Uri};

pub(crate) fn join(base: &str, path: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    parse(
        &format!("{}{}", base.trim_end_matches('/'), path),
        query,
        "upstream URL",
    )
}

pub(crate) fn exact(url: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    parse(url, query, "endpoint override")
}

fn parse(url: &str, query: Option<&str>, label: &str) -> Result<Uri, ChannelError> {
    let uri = url
        .parse::<Uri>()
        .map_err(|error| ChannelError::Prepare(format!("bad {label}: {error}")))?;
    with_query(uri, query)
}

fn with_query(uri: Uri, query: Option<&str>) -> Result<Uri, ChannelError> {
    if uri.scheme().is_none() || uri.authority().is_none() {
        return Err(ChannelError::Prepare(
            "upstream URL must be absolute".into(),
        ));
    }
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Ok(uri);
    };
    let path_and_query = match uri.query() {
        Some(existing) => format!("{}?{existing}&{query}", uri.path()),
        None => format!("{}?{query}", uri.path()),
    };
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(
        path_and_query
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("bad endpoint query: {error}")))?,
    );
    Uri::from_parts(parts).map_err(|error| ChannelError::Prepare(format!("bad endpoint: {error}")))
}

pub(crate) fn strip_userinfo(uri: Uri) -> Result<Uri, ChannelError> {
    let Some(authority) = uri.authority() else {
        return Ok(uri);
    };
    if !authority.as_str().contains('@') {
        return Ok(uri);
    }
    let clean = authority.port_u16().map_or_else(
        || authority.host().to_owned(),
        |port| format!("{}:{port}", authority.host()),
    );
    let mut parts = uri.into_parts();
    parts.authority = Some(
        clean
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("bad authority: {error}")))?,
    );
    Uri::from_parts(parts).map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(crate) fn allow_headers(source: &HeaderMap, allowed: &[&str]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in source {
        if allowed.contains(&name.as_str()) {
            headers.append(name.clone(), value.clone());
        }
    }
    headers
}

pub(crate) fn allow_query(query: Option<&str>, allowed: &[&str]) -> Option<String> {
    let kept = query?
        .split('&')
        .filter(|pair| {
            !pair.is_empty() && allowed.contains(&pair.split('=').next().unwrap_or_default())
        })
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

pub(crate) fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}
