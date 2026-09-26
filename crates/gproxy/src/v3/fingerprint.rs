//! v3's TLS fingerprints as v4 connection profiles.
//!
//! A v3 provider or credential could carry `tls_fingerprint`: a TLS layer, an
//! HTTP/2 layer and default headers (`v3:crates/gproxy-app/src/control/
//! fingerprint`). v4 says the same with a connection profile on the wreq
//! backend whose emulation is a custom fingerprint, and the fields line up one
//! for one apart from their spelling. The result is checked against
//! `gproxy_client::EmulationConfig` itself, because import does not look inside
//! the object and a malformed one would only fail when the client is built.
//!
//! Not carried, and reported: `tls.extension_permutation`, which v4's
//! fingerprint has no field for.

use gproxy_sdk::dto::ConnectionProfileDto;
use serde_json::{Map, Value, json};

use super::Report;

/// The v4 profile a v3 fingerprint becomes, or why it cannot.
pub fn profile(
    id: String,
    name: String,
    fingerprint: &Value,
    owner: &str,
    report: &mut Report,
) -> Result<ConnectionProfileDto, String> {
    let emulation = emulation(fingerprint, owner, report)?;
    serde_json::from_value::<gproxy_sdk::EmulationConfig>(emulation.clone())
        .map_err(|error| format!("it does not describe a usable client: {error}"))?;
    Ok(ConnectionProfileDto {
        id,
        name,
        backend: "wreq".into(),
        emulation: Some(emulation),
        // A captured client advertises compressed encodings; the pool has to
        // undo whichever one the upstream picks.
        gzip: true,
        brotli: true,
        deflate: true,
        zstd: true,
        redirect_max_hops: 0,
        retry: "never".into(),
        pool_idle_timeout_ms: 90_000,
        pool_max_idle_per_host: 32,
        created_at_ms: 0,
    })
}

fn emulation(fingerprint: &Value, owner: &str, report: &mut Report) -> Result<Value, String> {
    let root = fingerprint
        .as_object()
        .ok_or("the fingerprint is not a JSON object")?;
    let mut out = Map::new();
    out.insert("kind".into(), json!("custom"));
    if let Some(tls) = root.get("tls").filter(|tls| !tls.is_null()) {
        let tls = tls.as_object().ok_or("tls is not an object")?;
        if let Some(alpn) = tls.get("alpn_protocols") {
            let names = alpn
                .as_array()
                .ok_or("tls.alpn_protocols is not an array")?
                .iter()
                .map(|value| match value.as_str() {
                    Some("http/1.1") => Ok("http1"),
                    Some("h2") => Ok("http2"),
                    Some("h3") => Ok("http3"),
                    _ => Err(format!("tls.alpn_protocols has `{value}`")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            out.insert("alpn".into(), json!(names));
        }
        for (v3, v4) in [
            ("min_tls_version", "min_tls"),
            ("max_tls_version", "max_tls"),
        ] {
            if let Some(value) = tls.get(v3) {
                let version = match value.as_str().map(str::to_ascii_lowercase).as_deref() {
                    Some("tls1" | "tls1.0") => "tls10",
                    Some("tls1.1") => "tls11",
                    Some("tls1.2") => "tls12",
                    Some("tls1.3") => "tls13",
                    _ => return Err(format!("tls.{v3} is `{value}`")),
                };
                out.insert(v4.into(), json!(version));
            }
        }
        for (v3, v4) in [
            ("cipher_list", "cipher_list"),
            ("curves_list", "curves_list"),
            ("sigalgs_list", "sigalgs_list"),
            ("preserve_tls13_cipher_list", "preserve_tls13_cipher_list"),
            ("grease_enabled", "grease"),
        ] {
            if let Some(value) = tls.get(v3) {
                out.insert(v4.into(), value.clone());
            }
        }
        if tls
            .get("extension_permutation")
            .and_then(Value::as_array)
            .is_some_and(|ids| !ids.is_empty())
        {
            report.warn(format!(
                "{owner}: its TLS fingerprint's extension_permutation has no v4 field and was \
                 not carried"
            ));
        }
    }
    if let Some(http2) = root.get("http2").filter(|http2| !http2.is_null()) {
        let http2 = http2.as_object().ok_or("http2 is not an object")?;
        let mut settings = Map::new();
        for key in [
            "enable_push",
            "initial_window_size",
            "initial_connection_window_size",
            "max_frame_size",
            "max_header_list_size",
            "header_table_size",
            "max_concurrent_streams",
        ] {
            if let Some(value) = http2.get(key) {
                settings.insert(key.into(), value.clone());
            }
        }
        if let Some(order) = http2.get("headers_pseudo_order") {
            let names = order
                .as_array()
                .ok_or("http2.headers_pseudo_order is not an array")?
                .iter()
                .map(|value| match value.as_str() {
                    Some(":method") => Ok("method"),
                    Some(":scheme") => Ok("scheme"),
                    Some(":authority") => Ok("authority"),
                    Some(":path") => Ok("path"),
                    _ => Err(format!("http2.headers_pseudo_order has `{value}`")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            settings.insert("pseudo_header_order".into(), json!(names));
        }
        if let Some(order) = http2.get("settings_order") {
            let names = order
                .as_array()
                .ok_or("http2.settings_order is not an array")?
                .iter()
                .map(|value| match value.as_u64() {
                    Some(1) => Ok("header_table_size"),
                    Some(2) => Ok("enable_push"),
                    Some(3) => Ok("max_concurrent_streams"),
                    Some(4) => Ok("initial_window_size"),
                    Some(5) => Ok("max_frame_size"),
                    Some(6) => Ok("max_header_list_size"),
                    _ => Err(format!("http2.settings_order has `{value}`")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            settings.insert("settings_order".into(), json!(names));
        }
        out.insert("http2".into(), Value::Object(settings));
    }
    match root.get("headers") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => {}
        Some(Value::Object(headers)) => {
            let pairs = headers
                .iter()
                .map(|(name, value)| {
                    value
                        .as_str()
                        .map(|value| json!([name, value]))
                        .ok_or_else(|| format!("header `{name}` is not a string"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            out.insert("headers".into(), Value::Array(pairs));
        }
        Some(_) => return Err("headers is neither an object nor false".into()),
    }
    if out.len() == 1 {
        return Err("it has no TLS, HTTP/2 or header layer".into());
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_v3_fingerprint_becomes_a_custom_wreq_profile() {
        let mut report = Report::default();
        let out = profile(
            "p".into(),
            "n".into(),
            &json!({
                "tls": {"alpn_protocols": ["h2", "http/1.1"], "min_tls_version": "tls1.2",
                        "max_tls_version": "tls1.3", "cipher_list": "A:B", "grease_enabled": true,
                        "extension_permutation": [0, 10]},
                "http2": {"initial_window_size": 6291456,
                          "headers_pseudo_order": [":method", ":authority", ":scheme", ":path"],
                          "settings_order": [1, 2, 4, 6]},
                "headers": {"user-agent": "x/1"}
            }),
            "provider 1",
            &mut report,
        )
        .unwrap();
        assert_eq!(out.backend, "wreq");
        let emulation = out.emulation.unwrap();
        assert_eq!(emulation["alpn"], json!(["http2", "http1"]));
        assert_eq!(emulation["min_tls"], "tls12");
        assert_eq!(emulation["grease"], true);
        assert_eq!(
            emulation["http2"]["settings_order"][2],
            "initial_window_size"
        );
        assert_eq!(emulation["headers"], json!([["user-agent", "x/1"]]));
        assert_eq!(report.warnings.len(), 1, "extension_permutation");
    }

    #[test]
    fn an_unusable_fingerprint_is_refused() {
        let mut report = Report::default();
        for bad in [
            json!({}),
            json!({"headers": false}),
            json!({"tls": {"min_tls_version": "ssl3"}}),
        ] {
            assert!(
                profile("p".into(), "n".into(), &bad, "o", &mut report).is_err(),
                "{bad}"
            );
        }
    }
}
