//! Claude Web account usage.
//!
//! Clewdr introduced and retained this endpoint in commit `306af92`:
//! `GET /api/organizations/{org_uuid}/usage`. It uses the same browser cookie
//! and organization discovered during credential bootstrap as inference.

use bytes::Bytes;
use http::{Method, Request, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{UsageSnapshot, UsageWindow};

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
    let path = format!("/api/organizations/{organization}/usage");
    let uri = join_url(base, &path, None)?;
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
    // Plan/model-specific windows really are nullable. Normalize the stable
    // Sonnet window when present; experimental windows remain intact in `raw`.
    if let Some(window) = usage.seven_day_sonnet {
        windows.push(window.to_window("seven_day_sonnet").label("Sonnet"));
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

#[derive(Deserialize)]
struct WebUsage {
    five_hour: WebWindow,
    seven_day: WebWindow,
    seven_day_sonnet: Option<WebWindow>,
}

#[derive(Deserialize)]
struct WebWindow {
    utilization: f64,
    resets_at: String,
}

impl WebWindow {
    fn to_window(&self, name: &str) -> UsageWindow {
        UsageWindow::percent(name, self.utilization).resets_iso(self.resets_at.clone())
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
