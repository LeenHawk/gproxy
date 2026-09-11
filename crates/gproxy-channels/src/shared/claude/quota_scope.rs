use gproxy_channel_api::QuotaScope;

pub(crate) fn window(key: &str) -> QuotaScope {
    match key {
        "five_hour" | "seven_day" => QuotaScope::All,
        key => key
            .strip_prefix("seven_day_")
            .map(family)
            .unwrap_or_default(),
    }
}

pub(crate) fn model(id: Option<&str>, display: Option<&str>) -> QuotaScope {
    let id = id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(id) = id {
        // A concrete upstream model ID must not expand to its whole family.
        if id.starts_with("claude-") {
            return QuotaScope::Models(vec![id.to_owned()]);
        }
        return family(id);
    }
    display.map(family).unwrap_or_default()
}

fn family(value: &str) -> QuotaScope {
    let value = value.trim().to_ascii_lowercase();
    let family = value.strip_prefix("claude ").unwrap_or(&value);
    if family.is_empty() || !family.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return QuotaScope::Unknown;
    }
    QuotaScope::ModelPrefixes(vec![format!("claude-{family}")])
}
