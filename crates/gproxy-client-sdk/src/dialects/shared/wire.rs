use super::*;
pub(crate) fn read_metadata<T: DeserializeOwned>(
    block: &ContentBlock<impl Sized>,
    vendor: Vendor,
) -> Result<Option<T>> {
    block
        .provider_metadata
        .get(&vendor)
        .map(|rest| serde_json::from_value(Value::Object(rest.clone())).map_err(Error::invalid))
        .transpose()
}

#[derive(Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub(crate) struct Native<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<T>,
}

#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
pub(crate) fn read_native<T: DeserializeOwned>(
    block: &ContentBlock<impl Sized>,
    vendor: Vendor,
) -> Result<Option<T>> {
    Ok(read_metadata::<Native<T>>(block, vendor)?.and_then(|m| m.native))
}

pub(crate) fn encode_rest(value: impl Serialize) -> Result<Rest> {
    match serde_json::to_value(value).map_err(Error::invalid)? {
        Value::Object(rest) => Ok(rest),
        _ => Err(Error::invalid("metadata must serialize to an object")),
    }
}

pub(crate) fn with_metadata<C>(
    content: C,
    vendor: Vendor,
    metadata: impl Serialize,
) -> Result<ContentBlock<C>> {
    Ok(ContentBlock {
        content,
        cache: None,
        provider_metadata: [(vendor, encode_rest(metadata)?)].into(),
    })
}

#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
pub(crate) fn with_native<C>(
    content: C,
    vendor: Vendor,
    native: impl Serialize,
) -> Result<ContentBlock<C>> {
    with_metadata(
        content,
        vendor,
        Native {
            native: Some(native),
        },
    )
}

pub(crate) fn count(value: i64) -> Result<u64> {
    value.try_into().map_err(Error::invalid)
}
#[cfg(any(feature = "claude", feature = "gemini"))]
pub(crate) fn optional_count(value: Option<i64>) -> Result<Option<u64>> {
    value.map(count).transpose()
}

#[cfg(any(feature = "openai", feature = "chat"))]
pub(crate) fn data_url(source: &Asset, media_type: Option<&str>) -> Result<String> {
    match source {
        Asset::Inline { bytes } => Ok(format!(
            "data:{};base64,{}",
            media_type.ok_or_else(|| Error::invalid("inline media requires media_type"))?,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )),
        Asset::Url { url } => Ok(url.clone()),
        Asset::Unknown(_) => Err(Error::invalid("unknown persisted media source")),
        Asset::Reference { .. } => Err(Error::invalid(
            "resolve application asset references before sending",
        )),
        Asset::ProviderFile { .. } => {
            Err(Error::invalid("this native field requires bytes or a URL"))
        }
    }
}

#[cfg(feature = "gemini")]
pub(crate) fn decode_inline(data: &str) -> Result<Asset> {
    Ok(Asset::Inline {
        bytes: base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(Error::invalid)?,
    })
}

#[cfg(feature = "gemini")]
pub(crate) fn tool_name<'a>(
    name: &'a str,
    id: &str,
    calls: &BTreeMap<&str, &'a str>,
) -> Result<&'a str> {
    if !name.is_empty() {
        return Ok(name);
    }
    calls.get(id).copied().ok_or_else(|| {
        Error::invalid(format!(
            "no earlier tool call supplies a name for result {id}"
        ))
    })
}

#[derive(Serialize)]
#[serde(untagged)]
#[cfg(feature = "gemini")]
pub(crate) enum ToolOutputJson<'a> {
    Result { result: &'a Value },
    Error { error: &'a Value },
}

#[cfg(any(feature = "claude", feature = "openai"))]
pub(crate) fn index(value: i64) -> Result<u32> {
    value.try_into().map_err(Error::invalid)
}
