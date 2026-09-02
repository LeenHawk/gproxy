pub const PRICING_SERVICE_TIERS: [&str; 7] = [
    "standard",
    "priority",
    "flex",
    "scale",
    "ultrafast",
    "batch",
    "reserved",
];

pub(super) fn request_service_tier(body: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    let object = value.as_object()?;
    ["speed", "service_tier", "serviceTier"]
        .into_iter()
        .find_map(|name| object.get(name).and_then(tier_value))
}

pub fn response_service_tier(headers: &http::HeaderMap, body: &[u8]) -> Option<String> {
    headers
        .get("x-gemini-service-tier")
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_service_tier)
        .or_else(|| {
            serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .as_ref()
                .and_then(response_tier_value)
        })
}

fn response_tier_value(value: &serde_json::Value) -> Option<String> {
    if let Some(items) = value.as_array() {
        return items.iter().find_map(response_tier_value);
    }
    let object = value.as_object()?;
    ["speed", "service_tier", "serviceTier"]
        .into_iter()
        .find_map(|name| object.get(name).and_then(tier_value))
        .or_else(|| {
            ["usage", "usageMetadata", "response", "message"]
                .into_iter()
                .find_map(|name| object.get(name).and_then(response_tier_value))
        })
}

fn tier_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("type").and_then(serde_json::Value::as_str))
        .and_then(normalize_service_tier)
}

pub fn normalize_service_tier(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
    let normalized = match normalized.as_str() {
        "fast" => "priority",
        "ultra_fast" => "ultrafast",
        "default" | "on_demand" => "standard",
        value => value,
    };
    (!normalized.is_empty()).then(|| normalized.to_owned())
}
