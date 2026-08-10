//! Claude Web account usage.
//!
//! Clewdr introduced and retained this endpoint in commit `306af92`:
//! `GET /api/organizations/{org_uuid}/usage`. It uses the same browser cookie
//! and organization discovered during credential bootstrap as inference.

use bytes::Bytes;
use http::{Method, Request, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, exact_url, join_url};
use crate::channel::usage::{
    UsageSnapshot, UsageWindow, UsageWindowDescriptor, UsageWindowMeter, UsageWindowScope,
};
use crate::channel::usage_descriptor::with_known_duration;

pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let session_key = auth::session_key(secret)?;
    let organization = auth::organization_uuid(secret)?;
    let base = settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(auth::DEFAULT_BASE_URL)
        .trim_end_matches('/');
    let uri = match crate::channel::settings::endpoint_by_key(settings, "usage", "") {
        Some(url) => exact_url(&url.replace("{organization}", organization), None)?,
        None => {
            let path = format!("/api/organizations/{organization}/usage");
            join_url(base, &path, None)?
        }
    };
    let mut request = build_request(Method::GET, uri, http::HeaderMap::new(), Bytes::new())?;
    request.headers_mut().insert(
        header::ACCEPT,
        http::HeaderValue::from_static("application/json"),
    );
    auth::apply_browser_headers(&mut request, session_key, base, &format!("{base}/new"))?;
    Ok(Some(request))
}

pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let usage: WebUsage = serde_json::from_value(raw.clone()).ok()?;
    // The live endpoint always carries the primary session + weekly windows.
    // Missing/null primary windows mean the payload is invalid, not "no usage".
    let mut windows = vec![
        usage.five_hour.to_window("five_hour"),
        usage.seven_day.to_window("seven_day"),
    ];
    let mut scoped_seen = HashSet::new();
    // Plan/model-specific windows really are nullable. Normalize stable model
    // windows when present; experimental windows remain intact in `raw`.
    if let Some(window) = usage.seven_day_opus {
        scoped_seen.insert("model:opus".to_owned());
        windows.push(window.to_window("seven_day_opus").label("Opus"));
    }
    if let Some(window) = usage.seven_day_sonnet {
        scoped_seen.insert("model:sonnet".to_owned());
        windows.push(window.to_window("seven_day_sonnet").label("Sonnet"));
    }
    for limit in usage
        .limits
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|limit| limit.kind.as_deref() == Some("weekly_scoped"))
    {
        let Some(scope) = limit.normalized_scope() else {
            continue;
        };
        if !scoped_seen.insert(scope.identity()) {
            continue;
        }
        windows.push(
            limit
                .to_window(scope.window_name())
                .label(scope.label().to_owned()),
        );
    }
    Some(UsageSnapshot {
        plan: raw
            .get("plan")
            .or_else(|| raw.get("rate_limit_tier"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        windows,
        credits: None,
        rate_limit_reset_credits: None,
        raw,
    })
}

pub(super) fn describe(snapshot: &UsageSnapshot, index: usize) -> UsageWindowDescriptor {
    let Some(window) = snapshot.windows.get(index) else {
        return UsageWindowDescriptor::from_window(&UsageWindow {
            name: format!("window_{index}"),
            ..Default::default()
        });
    };
    let scope = match window.name.as_str() {
        "five_hour" | "seven_day" => UsageWindowScope::All,
        "seven_day_opus" => UsageWindowScope::Models {
            models: vec!["opus".into()],
        },
        "seven_day_sonnet" => UsageWindowScope::Models {
            models: vec!["sonnet".into()],
        },
        name if name.starts_with("weekly_model:") || name.starts_with("weekly_surface:") => {
            describe_scoped(snapshot, name).unwrap_or(UsageWindowScope::Unknown)
        }
        _ => UsageWindowScope::Unknown,
    };
    let descriptor = UsageWindowDescriptor::from_window(window)
        .scope(scope)
        .meter(UsageWindowMeter::Opaque);
    match window.name.as_str() {
        "five_hour" => with_known_duration(descriptor, window, 5 * 60 * 60),
        "seven_day" | "seven_day_opus" | "seven_day_sonnet" => {
            with_known_duration(descriptor, window, 7 * 24 * 60 * 60)
        }
        name if name.starts_with("weekly_") => {
            with_known_duration(descriptor, window, 7 * 24 * 60 * 60)
        }
        _ => descriptor,
    }
}

fn describe_scoped(snapshot: &UsageSnapshot, name: &str) -> Option<UsageWindowScope> {
    let usage: WebUsage = serde_json::from_value(snapshot.raw.clone()).ok()?;
    usage
        .limits?
        .into_iter()
        .filter_map(|limit| limit.normalized_scope())
        .find(|scope| scope.window_name() == name)
        .map(WebNormalizedScope::scope)
}

#[derive(Deserialize)]
struct WebUsage {
    five_hour: WebWindow,
    seven_day: WebWindow,
    seven_day_opus: Option<WebWindow>,
    seven_day_sonnet: Option<WebWindow>,
    limits: Option<Vec<WebLimit>>,
}

#[derive(Deserialize)]
struct WebWindow {
    utilization: Option<f64>,
    resets_at: String,
}

impl WebWindow {
    fn to_window(&self, name: &str) -> UsageWindow {
        UsageWindow {
            name: name.to_owned(),
            used_percent: self.utilization,
            ..Default::default()
        }
        .resets_iso(self.resets_at.clone())
    }
}

#[derive(Deserialize)]
struct WebLimit {
    kind: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<WebLimitScope>,
}

impl WebLimit {
    fn to_window(&self, name: impl Into<String>) -> UsageWindow {
        let mut window = UsageWindow {
            name: name.into(),
            used_percent: self.percent,
            ..Default::default()
        };
        if let Some(reset) = &self.resets_at {
            window = window.resets_iso(reset.clone());
        }
        window
    }

    fn normalized_scope(&self) -> Option<WebNormalizedScope> {
        let scope = self.scope.as_ref()?;
        if let Some(model) = &scope.model {
            let id = non_empty(model.id.as_deref());
            let display = non_empty(model.display_name.as_deref());
            let selector = id.or(display)?.to_owned();
            return Some(WebNormalizedScope::Model {
                key: scope_key(&selector),
                label: display.unwrap_or(&selector).to_owned(),
                selector,
            });
        }
        let surface = non_empty(scope.surface.as_deref())?.to_owned();
        Some(WebNormalizedScope::Surface {
            key: scope_key(&surface),
            surface,
        })
    }
}

#[derive(Deserialize)]
struct WebLimitScope {
    model: Option<WebLimitModel>,
    surface: Option<String>,
}

#[derive(Deserialize)]
struct WebLimitModel {
    display_name: Option<String>,
    id: Option<String>,
}

enum WebNormalizedScope {
    Model {
        key: String,
        label: String,
        selector: String,
    },
    Surface {
        key: String,
        surface: String,
    },
}

impl WebNormalizedScope {
    fn identity(&self) -> String {
        match self {
            Self::Model { label, .. } => format!("model:{}", scope_key(label)),
            Self::Surface { key, .. } => format!("surface:{key}"),
        }
    }

    fn window_name(&self) -> String {
        match self {
            Self::Model { key, .. } => format!("weekly_model:{key}"),
            Self::Surface { key, .. } => format!("weekly_surface:{key}"),
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Model { label, .. } => label,
            Self::Surface { surface, .. } => surface,
        }
    }

    fn scope(self) -> UsageWindowScope {
        match self {
            Self::Model { selector, .. } => UsageWindowScope::Models {
                models: vec![selector],
            },
            Self::Surface { surface, .. } => UsageWindowScope::Feature { feature: surface },
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn scope_key(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "scoped".to_owned()
    } else {
        out.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_cookie_authenticated_organization_usage_request() {
        let request = request(
            &json!({"cookie":"sk-ant-sid-example","account_uuid":"org-123"}),
            &json!({}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            request.uri(),
            "https://claude.ai/api/organizations/org-123/usage"
        );
        assert_eq!(
            request.headers().get(header::COOKIE).unwrap(),
            "sessionKey=sk-ant-sid-example"
        );
    }

    #[test]
    fn parses_live_web_usage_response() {
        let body = Bytes::from_static(
            br#"{
              "five_hour":{"utilization":3.0,"resets_at":"2026-07-12T16:29:59.581984+00:00","limit_dollars":null,"used_dollars":null,"remaining_dollars":null},
              "seven_day":{"utilization":0.0,"resets_at":"2026-07-17T21:59:59.582004+00:00","limit_dollars":null,"used_dollars":null,"remaining_dollars":null},
              "seven_day_oauth_apps":null,
              "seven_day_opus":null,
              "seven_day_sonnet":null,
              "seven_day_cowork":null,
              "seven_day_omelette":null,
              "tangelo":null,
              "iguana_necktie":null,
              "omelette_promotional":null,
              "nimbus_quill":null,
              "cinder_cove":null,
              "amber_ladder":null,
              "extra_usage":null,
              "limits":[
                {"kind":"session","group":"session","percent":3,"severity":"normal","resets_at":"2026-07-12T16:29:59.581984+00:00","scope":null,"is_active":true},
                {"kind":"weekly_all","group":"weekly","percent":0,"severity":"normal","resets_at":"2026-07-17T21:59:59.582004+00:00","scope":null,"is_active":false}
              ],
              "spend":null,
              "member_dashboard_available":false
            }"#,
        );
        let snapshot = parse(StatusCode::OK, &body).unwrap();
        let names: Vec<_> = snapshot
            .windows
            .iter()
            .map(|window| window.name.as_str())
            .collect();
        assert_eq!(names, ["five_hour", "seven_day"]);
        assert_eq!(snapshot.windows[0].used_percent, Some(3.0));
        assert_eq!(snapshot.windows[1].used_percent, Some(0.0));
        assert_eq!(snapshot.raw["limits"].as_array().unwrap().len(), 2);
        assert_eq!(snapshot.raw["member_dashboard_available"], false);
    }

    #[test]
    fn rejects_usage_without_required_primary_windows() {
        let body = Bytes::from_static(br#"{"five_hour":null,"seven_day":null}"#);
        assert!(parse(StatusCode::OK, &body).is_none());
    }
}
