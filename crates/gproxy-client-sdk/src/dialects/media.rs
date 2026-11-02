//! Small native conversions shared by dialects, not a media transport layer.
use super::*;

#[cfg(feature = "chat")]
pub(crate) fn clock() -> Result<u64> {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(Error::invalid)
}
#[cfg(feature = "gemini")]
pub(crate) fn mime<'a>(value: Option<&'a str>, source: &Asset) -> Result<&'a str> {
    match source {
        Asset::Inline { .. } => {
            value.ok_or_else(|| Error::invalid("inline media requires media_type"))
        }
        _ => Ok(value.unwrap_or("application/octet-stream")),
    }
}

pub(crate) fn check_file_vendor(source: &Asset, vendor: Vendor) -> Result<()> {
    if let Asset::ProviderFile { vendor: owner, .. } = source
        && *owner != vendor
    {
        return Err(Error::invalid(
            "provider file IDs cannot be reused across vendors; prepare portable bytes or a URL",
        ));
    }
    Ok(())
}

#[cfg(any(feature = "claude", feature = "openai", feature = "chat"))]
#[derive(Default, Serialize, Deserialize)]
struct CacheMetadata {
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    cache: Rest,
}
#[cfg(any(feature = "claude", feature = "openai", feature = "chat"))]
fn cache_rest(block: &ContentBlock<impl Sized>, vendor: Vendor) -> Result<Rest> {
    Ok(read_metadata::<CacheMetadata>(block, vendor)?
        .unwrap_or_default()
        .cache)
}
#[cfg(any(feature = "claude", feature = "openai"))]
fn save_cache<C>(block: &mut ContentBlock<C>, vendor: Vendor, rest: Rest) -> Result<()> {
    if !rest.is_empty() {
        block
            .provider_metadata
            .entry(vendor)
            .or_default()
            .extend(encode_rest(CacheMetadata { cache: rest })?);
    }
    Ok(())
}
#[cfg(feature = "claude")]
pub(crate) fn claude_cache(
    block: &ContentBlock<impl Sized>,
) -> Result<Option<gproxy_protocol::claude::content::CacheControl>> {
    use gproxy_protocol::claude::content as c;
    block
        .cache
        .as_ref()
        .map(|cache| {
            let mut native = c::CacheControl::builder(c::CacheControlType::Ephemeral)
                .rest(cache_rest(block, Vendor::Claude)?)
                .build();
            native.ttl = match cache.ttl {
                Some(CacheTtl::FiveMinutes) => Some(c::CacheTtl::FiveMinutes),
                Some(CacheTtl::OneHour) => Some(c::CacheTtl::OneHour),
                _ => None,
            };
            Ok(native)
        })
        .transpose()
}
#[cfg(feature = "claude")]
pub(crate) fn parse_claude_cache<C>(
    block: &mut ContentBlock<C>,
    cache: Option<gproxy_protocol::claude::content::CacheControl>,
) -> Result<()> {
    use gproxy_protocol::claude::content as c;
    if let Some(cache) = cache {
        block.cache = Some(CacheBreakpoint {
            ttl: cache.ttl.map(|v| match v {
                c::CacheTtl::FiveMinutes => CacheTtl::FiveMinutes,
                c::CacheTtl::OneHour => CacheTtl::OneHour,
            }),
        });
        save_cache(block, Vendor::Claude, cache.rest)?;
    }
    Ok(())
}
#[cfg(feature = "openai")]
pub(crate) fn responses_cache(
    block: &ContentBlock<impl Sized>,
) -> Result<Option<gproxy_protocol::openai::responses::input::PromptCacheBreakpoint>> {
    use gproxy_protocol::openai::responses::input as i;
    block
        .cache
        .as_ref()
        .map(|cache| {
            let mut value = i::PromptCacheBreakpoint::builder(i::PromptCacheMode::Explicit)
                .rest(cache_rest(block, Vendor::Openai)?)
                .build();
            value.ttl = match cache.ttl {
                Some(CacheTtl::ThirtyMinutes) => Some(i::PromptCacheBreakpointTtl::ThirtyMinutes),
                Some(CacheTtl::TwelveHours) => Some(i::PromptCacheBreakpointTtl::TwelveHours),
                _ => None,
            };
            Ok(value)
        })
        .transpose()
}
#[cfg(feature = "openai")]
pub(crate) fn parse_responses_cache<C>(
    block: &mut ContentBlock<C>,
    cache: Option<gproxy_protocol::openai::responses::input::PromptCacheBreakpoint>,
) -> Result<()> {
    use gproxy_protocol::openai::responses::input::PromptCacheBreakpointTtl as T;
    if let Some(cache) = cache {
        block.cache = Some(CacheBreakpoint {
            ttl: cache.ttl.map(|ttl| match ttl {
                T::ThirtyMinutes => CacheTtl::ThirtyMinutes,
                T::TwelveHours => CacheTtl::TwelveHours,
            }),
        });
        save_cache(block, Vendor::Openai, cache.rest)?;
    }
    Ok(())
}
#[cfg(feature = "chat")]
pub(crate) fn chat_cache(
    block: &ContentBlock<impl Sized>,
) -> Result<Option<gproxy_protocol::openai::chat::content::PromptCacheBreakpoint>> {
    use gproxy_protocol::openai::chat::content as i;
    block
        .cache
        .as_ref()
        .map(|cache| {
            let mut value = i::PromptCacheBreakpoint::builder(i::PromptCacheMode::Explicit)
                .rest(cache_rest(block, Vendor::Openai)?)
                .build();
            value.ttl = match cache.ttl {
                Some(CacheTtl::ThirtyMinutes) => Some(i::PromptCacheBreakpointTtl::ThirtyMinutes),
                Some(CacheTtl::TwelveHours) => Some(i::PromptCacheBreakpointTtl::TwelveHours),
                _ => None,
            };
            Ok(value)
        })
        .transpose()
}
#[cfg(feature = "openai")]
pub(crate) fn responses_detail(
    detail: Detail,
) -> Option<gproxy_protocol::openai::responses::input::ImageDetail> {
    use gproxy_protocol::openai::responses::input::ImageDetail as D;
    match detail {
        Detail::Low => Some(D::Low),
        Detail::High => Some(D::High),
        Detail::Original => Some(D::Original),
        Detail::Auto => Some(D::Auto),
        Detail::Medium => None,
    }
}
#[cfg(feature = "chat")]
pub(crate) fn chat_detail(
    detail: Detail,
) -> Option<gproxy_protocol::openai::chat::content::ImageDetail> {
    use gproxy_protocol::openai::chat::content::ImageDetail as D;
    match detail {
        Detail::Low => Some(D::Low),
        Detail::High => Some(D::High),
        Detail::Auto => Some(D::Auto),
        Detail::Medium | Detail::Original => None,
    }
}
#[cfg(feature = "gemini")]
pub(crate) fn gemini_detail(detail: Detail) -> Option<gproxy_protocol::gemini::MediaResolution> {
    use gproxy_protocol::gemini as g;
    let level = match detail {
        Detail::Low => g::MediaResolutionLevel::Low,
        Detail::Medium => g::MediaResolutionLevel::Medium,
        Detail::High => g::MediaResolutionLevel::High,
        Detail::Original => g::MediaResolutionLevel::UltraHigh,
        Detail::Auto => return None,
    };
    Some(g::MediaResolution::builder().level(level).build())
}
#[cfg(feature = "gemini")]
pub(crate) fn parse_gemini_detail(
    value: Option<&gproxy_protocol::gemini::MediaResolution>,
) -> Detail {
    use gproxy_protocol::gemini::MediaResolutionLevel as D;
    match value.and_then(|v| v.level.as_ref()) {
        Some(D::Low) => Detail::Low,
        Some(D::Medium) => Detail::Medium,
        Some(D::High) => Detail::High,
        Some(D::UltraHigh) => Detail::Original,
        _ => Detail::Auto,
    }
}

#[cfg(feature = "gemini")]
pub(crate) fn pcm_mime(format: &AudioFormat) -> Result<String> {
    match format {
        AudioFormat::Encoded { media_type } => {
            if is_pcm(media_type) {
                return Err(Error::invalid("raw PCM requires sample_rate and channels"));
            }
            Ok(media_type.clone())
        }
        AudioFormat::Pcm {
            media_type,
            sample_rate,
            channels,
        } => {
            if *sample_rate == 0 || *channels == 0 {
                return Err(Error::invalid(
                    "PCM sample rate and channels must be positive",
                ));
            }
            Ok(format!(
                "{media_type};rate={sample_rate};channels={channels}"
            ))
        }
    }
}
#[cfg(feature = "gemini")]
pub(crate) fn is_pcm(mime: &str) -> bool {
    mime.starts_with("audio/pcm") || mime.starts_with("audio/L16") || mime.starts_with("audio/l16")
}
#[cfg(feature = "gemini")]
pub(crate) fn parse_audio_format(mime: &str) -> Result<AudioFormat> {
    if !is_pcm(mime) {
        return Ok(AudioFormat::Encoded {
            media_type: mime.into(),
        });
    }
    let mut rate = None;
    let mut channels = None;
    for parameter in mime.split(';').skip(1) {
        if let Some(value) = parameter.trim().strip_prefix("rate=") {
            rate = Some(value.parse::<u32>().map_err(Error::invalid)?);
        }
        if let Some(value) = parameter.trim().strip_prefix("channels=") {
            channels = Some(value.parse::<u16>().map_err(Error::invalid)?);
        }
    }
    // Gemini's documented raw output is mono; its MIME commonly reports only rate.
    Ok(AudioFormat::Pcm {
        media_type: mime.split(';').next().unwrap_or(mime).into(),
        sample_rate: rate.ok_or_else(|| Error::invalid("PCM MIME is missing rate"))?,
        channels: channels.unwrap_or(1),
    })
}

#[cfg(any(feature = "openai", feature = "chat", feature = "gemini"))]
pub(crate) fn document_bytes(
    document: &Document,
) -> Result<(std::borrow::Cow<'_, Asset>, Option<&str>)> {
    Ok(match &document.source {
        DocumentSource::Asset { source } => (
            std::borrow::Cow::Borrowed(source),
            document.media_type.as_deref(),
        ),
        DocumentSource::Text { text } => (
            std::borrow::Cow::Owned(Asset::Inline {
                bytes: text.as_bytes().to_vec(),
            }),
            Some("text/plain"),
        ),
        DocumentSource::Chunks { .. } => {
            return Err(Error::invalid(
                "custom document chunks are supported by Claude Messages",
            ));
        }
    })
}
