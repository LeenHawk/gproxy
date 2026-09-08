use gproxy_store::records::{SettingInput, SettingRecord};
use serde_json::{Map, Value};

use crate::dto::RuntimeSettingsDto;

pub fn read(values: &[SettingRecord]) -> Result<RuntimeSettingsDto, gproxy_store::StoreError> {
    let values = values
        .iter()
        .filter(|setting| !setting.value.is_null())
        .map(|setting| (setting.key.clone(), setting.value.clone()))
        .collect::<Map<_, _>>();
    let mut settings = serde_json::from_value(Value::Object(values)).map_err(|error| {
        gproxy_store::StoreError::InvalidData {
            field: "runtime settings",
            message: error.to_string(),
        }
    })?;
    normalize(&mut settings).map_err(|message| gproxy_store::StoreError::InvalidData {
        field: "runtime settings",
        message,
    })?;
    Ok(settings)
}

pub(crate) fn writes(settings: &RuntimeSettingsDto) -> Vec<SettingInput> {
    let value = serde_json::to_value(settings).expect("runtime settings serialize");
    value
        .as_object()
        .expect("runtime settings are an object")
        .iter()
        .map(|(key, value)| SettingInput {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
}

pub fn normalize(settings: &mut RuntimeSettingsDto) -> Result<(), String> {
    if settings.max_attempts == 0 {
        return Err("maximum upstream attempts must be positive".into());
    }
    if settings.max_in_flight == 0 {
        return Err("request concurrency must be positive".into());
    }
    usize::try_from(settings.max_in_flight)
        .map_err(|_| "request concurrency exceeds this runtime's limit")?;
    usize::try_from(settings.file_upload_max_in_flight)
        .map_err(|_| "file upload concurrency exceeds this runtime's limit")?;
    settings.cors_origins = normalize_origins(&settings.cors_origins)?;
    let mut proxies = Vec::new();
    for value in &settings.trusted_proxies {
        let ip = value
            .trim()
            .parse::<std::net::IpAddr>()
            .map_err(|_| "trusted proxies must be IP addresses")?
            .to_string();
        if !proxies.contains(&ip) {
            proxies.push(ip);
        }
    }
    settings.trusted_proxies = proxies;
    settings.proxy = settings
        .proxy
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if let Some(proxy) = &settings.proxy {
        let uri = proxy
            .parse::<http::Uri>()
            .map_err(|_| "outbound proxy must be an absolute proxy URL")?;
        if !matches!(
            uri.scheme_str(),
            Some("http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h")
        ) || uri.authority().is_none()
        {
            return Err("outbound proxy must be an HTTP, HTTPS, or SOCKS proxy URL".into());
        }
    }
    Ok(())
}

pub fn normalize_origins(values: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        let uri = value
            .parse::<http::Uri>()
            .map_err(|_| "CORS origins must be HTTP or HTTPS origins")?;
        let scheme = uri.scheme_str().unwrap_or_default();
        let authority = uri.authority().ok_or("CORS origins must include a host")?;
        if !matches!(scheme, "http" | "https")
            || !matches!(uri.path(), "" | "/")
            || uri.query().is_some()
            || value.contains('#')
            || authority.as_str().contains('@')
            || authority.host().contains('*')
        {
            return Err(
                "CORS origins cannot include credentials, paths, queries, fragments, or wildcards"
                    .into(),
            );
        }
        let host = authority.host().to_ascii_lowercase();
        let port = authority.port_u16();
        let suffix = &authority.as_str()[authority.host().len()..];
        if !suffix.is_empty() && port.is_none() {
            return Err("CORS origins must use a valid port".into());
        }
        let default_port = match scheme {
            "http" => Some(80),
            "https" => Some(443),
            _ => None,
        };
        let origin = match port.filter(|_| port != default_port) {
            Some(port) => format!("{scheme}://{host}:{port}"),
            None => format!("{scheme}://{host}"),
        };
        if !normalized.contains(&origin) {
            normalized.push(origin);
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    #[test]
    fn origins_normalize_browser_serialization_and_reject_url_components() {
        let values = [
            " https://EXAMPLE.test:443/ ",
            "https://example.test",
            "http://localhost:8787/",
            "http://[::1]:80/",
        ]
        .map(str::to_owned);
        assert_eq!(
            super::normalize_origins(&values).unwrap(),
            [
                "https://example.test",
                "http://localhost:8787",
                "http://[::1]"
            ]
        );
        for value in [
            "*",
            "null",
            "https://*.example.test",
            "https://user@example.test",
            "https://example.test/path",
            "https://example.test/?q=1",
            "https://example.test/#fragment",
            "https://example.test:99999",
            "https://example.test:invalid",
        ] {
            assert!(
                super::normalize_origins(&[value.into()]).is_err(),
                "{value}"
            );
        }
    }
}
