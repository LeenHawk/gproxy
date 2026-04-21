//! Config builder for the prepare `p` field and PoW payloads.
//!
//! Translated from `target/samples/output/prepare_p.js`. The server accepts
//! any structurally valid 25-slot config array; the individual values
//! (UA, timestamps, uuid, screen size, etc.) are not cross-checked.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

pub const DEFAULT_BUILD_ID: &str = "prod-d7545204e22cb990d0245281e6550977d93b6a81";
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36 Edg/147.0.0.0";
pub const DEFAULT_LANGUAGES: &[&str] = &["en", "zh-CN", "zh"];
pub const DEFAULT_TQT_CANDIDATES: &[&str] = &[
    "vendor−Google Inc.",
    "language−en",
    "wakeLock−[object WakeLock]",
    "maxTouchPoints−0",
    "deviceMemory−8",
];
pub const DEFAULT_DOCUMENT_KEYS: &[&str] = &["location", "_reactListening7emk2nodhb"];
pub const DEFAULT_WINDOW_KEYS: &[&str] = &[
    "outerWidth",
    "__oai_so_kp",
    "localStorage",
    "visualViewport",
];

/// Options for building the base config array. Fields roughly correspond to
/// JS `buildBaseConfig(options)`; defaults mirror the browser.
#[derive(Debug, Clone)]
pub struct ConfigOptions {
    pub user_agent: String,
    pub build_id: String,
    pub language: String,
    pub languages: Vec<String>,
    pub screen_width: u32,
    pub screen_height: u32,
    pub hardware_concurrency: u32,
    pub js_heap_size_limit: u64,
    pub tqt_value: String,
    pub document_key: String,
    pub window_key: String,
    pub sid: String,
    pub search_keys: String,
    pub date_string: String,
    pub performance_now: f64,
    pub time_origin: f64,
    pub rand3: f64,
    pub rand9: f64,
    pub attempt: f64,
    pub elapsed_ms: f64,
}

impl ConfigOptions {
    /// Build with realistic browser-like defaults + a fresh uuid.
    pub fn browser_default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let time_origin = now.as_secs_f64() * 1000.0;
        Self {
            user_agent: DEFAULT_USER_AGENT.to_string(),
            build_id: DEFAULT_BUILD_ID.to_string(),
            language: "en".to_string(),
            languages: DEFAULT_LANGUAGES.iter().map(|s| s.to_string()).collect(),
            screen_width: 1366,
            screen_height: 1408,
            hardware_concurrency: 32,
            js_heap_size_limit: 4_294_967_296,
            tqt_value: DEFAULT_TQT_CANDIDATES[0].to_string(),
            document_key: DEFAULT_DOCUMENT_KEYS[0].to_string(),
            window_key: DEFAULT_WINDOW_KEYS[0].to_string(),
            sid: uuid::Uuid::new_v4().to_string(),
            search_keys: String::new(),
            date_string: format_browser_date(now.as_secs() as i64, 480, "中国标准时间"),
            performance_now: 30412.5,
            time_origin,
            rand3: rand::random::<f64>(),
            rand9: rand::random::<f64>(),
            attempt: 0.0,
            elapsed_ms: 0.0,
        }
    }

    /// Deterministic values for unit tests.
    #[cfg(test)]
    pub fn fixed_for_tests() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.to_string(),
            build_id: DEFAULT_BUILD_ID.to_string(),
            language: "en".to_string(),
            languages: DEFAULT_LANGUAGES.iter().map(|s| s.to_string()).collect(),
            screen_width: 1366,
            screen_height: 1408,
            hardware_concurrency: 32,
            js_heap_size_limit: 4_294_967_296,
            tqt_value: DEFAULT_TQT_CANDIDATES[0].to_string(),
            document_key: DEFAULT_DOCUMENT_KEYS[0].to_string(),
            window_key: DEFAULT_WINDOW_KEYS[0].to_string(),
            sid: "ee7b3426-19ed-4541-868a-ae24e57837ba".to_string(),
            search_keys: String::new(),
            date_string: "Tue Apr 21 2026 17:25:57 GMT+0800 (中国标准时间)".to_string(),
            performance_now: 30412.5,
            time_origin: 1776763524501.3,
            rand3: 0.12345,
            rand9: 0.67890,
            attempt: 0.0,
            elapsed_ms: 0.0,
        }
    }
}

/// Format a unix-seconds timestamp as a browser `Date.toString()` string.
fn format_browser_date(unix_secs: i64, offset_minutes: i32, zone_label: &str) -> String {
    let shifted = unix_secs + (offset_minutes as i64) * 60;
    let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, m, d, hh, mm, ss, wd) = unix_to_ymd(shifted);
    let sign = if offset_minutes >= 0 { '+' } else { '-' };
    let abs = offset_minutes.abs();
    format!(
        "{wday} {mon} {day:02} {year} {hh:02}:{mm:02}:{ss:02} GMT{sign}{oh:02}{om:02} ({zone})",
        wday = weekdays[wd as usize],
        mon = months[(m - 1) as usize],
        day = d,
        year = y,
        hh = hh,
        mm = mm,
        ss = ss,
        sign = sign,
        oh = abs / 60,
        om = abs % 60,
        zone = zone_label,
    )
}

/// Convert unix seconds to (year, month[1-12], day, hour, min, sec, weekday[0=Sun]).
/// Handles only dates after 1970-01-01.
fn unix_to_ymd(secs: i64) -> (i32, u32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86400);
    let time = secs.rem_euclid(86400) as u32;
    let hh = time / 3600;
    let mm = (time % 3600) / 60;
    let ss = time % 60;
    // 1970-01-01 was a Thursday (weekday 4)
    let wd = ((days + 4).rem_euclid(7)) as u32;
    // Civil from days algorithm (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d, hh, mm, ss, wd)
}

/// 25-slot config array matching the JS layout.
pub fn build_base_config(opts: &ConfigOptions) -> Config {
    Config {
        screen_sum: opts.screen_width + opts.screen_height,
        date_string: opts.date_string.clone(),
        js_heap_size_limit: opts.js_heap_size_limit,
        attempt: opts.rand3,
        user_agent: opts.user_agent.clone(),
        script_source: Value::Null,
        build_id: opts.build_id.clone(),
        language: opts.language.clone(),
        languages_joined: opts.languages.join(","),
        elapsed_ms: opts.rand9,
        tqt_value: opts.tqt_value.clone(),
        document_key: opts.document_key.clone(),
        window_key: opts.window_key.clone(),
        performance_now: opts.performance_now,
        sid: opts.sid.clone(),
        search_keys: opts.search_keys.clone(),
        hardware_concurrency: opts.hardware_concurrency,
        time_origin: opts.time_origin,
    }
}

/// 25-slot config. Serialized as an array in the order shown by
/// `to_json_array()`.
#[derive(Debug, Clone)]
pub struct Config {
    pub screen_sum: u32,
    pub date_string: String,
    pub js_heap_size_limit: u64,
    /// Slot 3: random float for prepare, integer attempt for PoW.
    pub attempt: f64,
    pub user_agent: String,
    pub script_source: Value,
    pub build_id: String,
    pub language: String,
    pub languages_joined: String,
    /// Slot 9: random float for prepare, elapsed ms for PoW.
    pub elapsed_ms: f64,
    pub tqt_value: String,
    pub document_key: String,
    pub window_key: String,
    pub performance_now: f64,
    pub sid: String,
    pub search_keys: String,
    pub hardware_concurrency: u32,
    pub time_origin: f64,
}

impl Config {
    fn to_json_array(&self) -> Vec<Value> {
        vec![
            json!(self.screen_sum),
            json!(self.date_string),
            json!(self.js_heap_size_limit),
            json!(self.attempt),
            json!(self.user_agent),
            self.script_source.clone(),
            json!(self.build_id),
            json!(self.language),
            json!(self.languages_joined),
            json!(self.elapsed_ms),
            json!(self.tqt_value),
            json!(self.document_key),
            json!(self.window_key),
            json!(self.performance_now),
            json!(self.sid),
            json!(self.search_keys),
            json!(self.hardware_concurrency),
            json!(self.time_origin),
            json!(0),
            json!(0),
            json!(0),
            json!(0),
            json!(0),
            json!(0),
            json!(0),
        ]
    }
}

/// `base64(JSON.stringify(config))`.
pub fn encode_config(cfg: &Config) -> String {
    let arr = cfg.to_json_array();
    let s = serde_json::to_string(&arr).unwrap();
    STANDARD.encode(s.as_bytes())
}

/// Build the prepare request `p` field: `gAAAAAC<base64(json(config))>`.
/// Slot 3 is forced to `1` and slot 9 to `prepare_duration_ms`.
pub fn build_prepare_p(opts: &ConfigOptions) -> String {
    let mut cfg = build_base_config(opts);
    cfg.attempt = 1.0;
    cfg.elapsed_ms = opts.performance_now.max(0.0);
    format!("gAAAAAC{}", encode_config(&cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_p_has_expected_prefix_and_length() {
        let opts = ConfigOptions::fixed_for_tests();
        let p = build_prepare_p(&opts);
        assert!(p.starts_with("gAAAAAC"), "prefix: {}", &p[..15]);
        // Sample HAR length was 575/603; ours should be in the same ballpark.
        assert!(p.len() > 400 && p.len() < 900, "len={}", p.len());
    }

    #[test]
    fn config_decodes_to_25_slots() {
        let opts = ConfigOptions::fixed_for_tests();
        let p = build_prepare_p(&opts);
        let body = p.strip_prefix("gAAAAAC").unwrap();
        let raw = STANDARD.decode(body).unwrap();
        let arr: Vec<Value> = serde_json::from_slice(&raw).unwrap();
        assert_eq!(arr.len(), 25);
        assert_eq!(arr[3], json!(1.0));
    }

    #[test]
    fn unix_to_ymd_known_dates() {
        assert_eq!(unix_to_ymd(0), (1970, 1, 1, 0, 0, 0, 4));
        // 2026-04-21 00:00:00 UTC -> unix 1776729600
        assert_eq!(unix_to_ymd(1_776_729_600), (2026, 4, 21, 0, 0, 0, 2));
    }
}
