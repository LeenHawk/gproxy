//! Claude Code 2.1.311 reset eligibility and explicit redemption.
//! These GETs deliberately do not run as part of the plain usage query.
//! Sources under samples/claude-code-2.1.311/readable/: chunk-fe3j6xq2.js
//! (grant schema, redemption) and chunk-mjxed380.js (grant kinds, offers,
//! unavailable reasons, weekly reset).

use super::{
    Claudecode, account, base_url, fact, header_value, invalid_response, send, unix_now_ms,
};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaReset, QuotaResetCredits,
    QuotaResetOption, QuotaResetOutcome, QuotaResetRequest, QuotaResetResult,
};
use http::{HeaderMap, HeaderValue, Method, header};
use serde::Deserialize;
use serde_json::{Value, json};

fn instant(value: Option<&str>) -> Option<i64> {
    value
        .and_then(|v| {
            time::OffsetDateTime::parse(v, &time::format_description::well_known::Rfc3339).ok()
        })
        .map(|v| (v.unix_timestamp_nanos() / 1_000_000) as i64)
}

async fn call(
    context: CredentialContext<'_>,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, ChannelError> {
    let account = account(&context.credential)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {}", account.access_token))?,
    );
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(super::CLI_USER_AGENT),
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(super::OAUTH_BETA),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let body = body
        .map(|body| serde_json::to_vec(&body))
        .transpose()
        .map_err(|e| invalid_response(e.to_string()))?;
    let (status, _, bytes) = send(
        context.client,
        method,
        &format!("{}{path}", base_url(context.provider)),
        headers,
        body,
    )
    .await?;
    if !status.is_success() {
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))
}

#[derive(Deserialize)]
struct Weekly {
    eligible: bool,
    ineligible_reason: Option<String>,
    arm: Option<String>,
    #[serde(default)]
    available: bool,
    next_available_at: Option<String>,
}

#[derive(Deserialize)]
struct Grants {
    eligible: bool,
    ineligible_reason: Option<String>,
    #[serde(default)]
    at_limit: bool,
    #[serde(default)]
    exhausted: Vec<String>,
    #[serde(default)]
    grants: Vec<Grant>,
    cooldown_until: Option<String>,
}

#[derive(Deserialize)]
struct Grant {
    id: String,
    label: Option<String>,
    resets_left: u64,
    ends_at: Option<String>,
    #[serde(default)]
    clears: Vec<String>,
    #[serde(default)]
    blocking: Vec<String>,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    usable_now: bool,
    #[serde(default = "requires_limit")]
    use_requires_limit: bool,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    gives_back_weekly: bool,
    unavailable_reason: Option<String>,
}
fn requires_limit() -> bool {
    true
}

/// Server `unavailable_reason` values the CLI accepts; others fall back to
/// its own derivation.
const UNAVAILABLE_REASONS: &[&str] = &[
    "cooldown",
    "blocked_five_hour",
    "blocked_seven_day",
    "blocked_opus",
    "blocked_sonnet",
    "blocked_other",
    "cannot_clear",
    "needs_limit",
    "unavailable",
];

impl Grant {
    /// The reset kind the CLI groups grants by (`qe`): a Fable give-back,
    /// the server's type, else what the grant clears.
    fn kind(&self) -> &str {
        if self.gives_back_weekly {
            return "fable";
        }
        if let Some(kind) = self
            .kind
            .as_deref()
            .filter(|v| matches!(*v, "session" | "weekly" | "full"))
        {
            return kind;
        }
        let session = self.clears.iter().any(|v| v == "five_hour");
        let other = self.clears.iter().any(|v| v != "five_hour");
        match (session, other) {
            (true, false) => "session",
            (false, true) => "weekly",
            _ => "full",
        }
    }

    /// Why a grant the server marks unusable cannot be used now (`tr`).
    fn unavailable_reason(&self, status: &Grants) -> String {
        if let Some(reason) = self
            .unavailable_reason
            .as_deref()
            .filter(|v| UNAVAILABLE_REASONS.contains(v))
        {
            return reason.into();
        }
        if status.cooldown_until.is_some() {
            return "cooldown".into();
        }
        for limit in &self.blocking {
            match limit.as_str() {
                "five_hour" => return "blocked_five_hour".into(),
                "seven_day" => return "blocked_seven_day".into(),
                "seven_day_opus" => return "blocked_opus".into(),
                "seven_day_sonnet" => return "blocked_sonnet".into(),
                _ => {}
            }
        }
        if !self.blocking.is_empty() {
            return "blocked_other".into();
        }
        let refills = |limit: &String| {
            self.clears.contains(limit) || (limit == "seven_day" && self.gives_back_weekly)
        };
        if status.at_limit && !status.exhausted.iter().any(refills) {
            return "cannot_clear".into();
        }
        if self.use_requires_limit {
            "needs_limit".into()
        } else {
            "unavailable".into()
        }
    }
}

fn unavailable(program: &str, reason: &str) -> QuotaResetOption {
    QuotaResetOption {
        program: program.into(),
        grant_id: None,
        label: None,
        available_count: None,
        expires_at_ms: None,
        next_available_at_ms: None,
        usable: false,
        ineligible_reason: Some(reason.into()),
        clears: Vec::new(),
    }
}

fn weekly_option(value: Option<&Value>) -> Result<QuotaResetOption, ChannelError> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(unavailable("juniper_tide", "unavailable"));
    };
    let weekly: Weekly =
        serde_json::from_value(value.clone()).map_err(|e| invalid_response(e.to_string()))?;
    let usable = weekly.eligible && weekly.arm.as_deref() == Some("reset") && weekly.available;
    Ok(QuotaResetOption {
        program: "juniper_tide".into(),
        grant_id: None,
        label: None,
        available_count: weekly.eligible.then_some(u64::from(usable)),
        expires_at_ms: None,
        next_available_at_ms: instant(weekly.next_available_at.as_deref()),
        usable,
        ineligible_reason: if usable {
            None
        } else {
            Some(
                weekly
                    .ineligible_reason
                    .unwrap_or_else(|| "unavailable".into()),
            )
        },
        clears: vec!["five_hour".into()],
    })
}

fn grant_options(value: Option<&Value>, now: i64) -> Result<Vec<QuotaResetOption>, ChannelError> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(vec![unavailable("cedar_ember", "unavailable")]);
    };
    let status: Grants =
        serde_json::from_value(value.clone()).map_err(|e| invalid_response(e.to_string()))?;
    if status.grants.is_empty() {
        let mut option = unavailable(
            "cedar_ember",
            status.ineligible_reason.as_deref().unwrap_or("no_grant"),
        );
        option.available_count = status.eligible.then_some(0);
        return Ok(vec![option]);
    }
    // The server's `usable_now` decides; the CLI offers the first usable,
    // unexpired grant of each kind and no longer waits for `next_grant_id`.
    let mut offered = Vec::new();
    let options = status
        .grants
        .iter()
        .map(|grant| {
            let expires_at_ms = instant(grant.ends_at.as_deref());
            let reason = if !status.eligible {
                Some(
                    status
                        .ineligible_reason
                        .clone()
                        .unwrap_or_else(|| "ineligible".into()),
                )
            } else if grant.paused {
                Some("paused".into())
            } else if grant.resets_left == 0 && !grant.usable_now {
                Some("no_grant".into())
            } else if !grant.usable_now {
                Some(grant.unavailable_reason(&status))
            } else if expires_at_ms.is_some_and(|end| end <= now) {
                Some("expired".into())
            } else if offered.contains(&grant.kind()) {
                Some("not_next_grant".into())
            } else {
                offered.push(grant.kind());
                None
            };
            QuotaResetOption {
                program: "cedar_ember".into(),
                grant_id: Some(grant.id.clone()),
                label: grant.label.clone(),
                available_count: Some(grant.resets_left),
                expires_at_ms,
                next_available_at_ms: instant(status.cooldown_until.as_deref()),
                usable: reason.is_none(),
                ineligible_reason: reason,
                clears: grant.clears.clone(),
            }
        })
        .collect();
    Ok(options)
}

impl QuotaReset for Claudecode {
    fn credits<'a>(
        &'a self,
        context: CredentialContext<'a>,
    ) -> OperationFuture<'a, QuotaResetCredits> {
        Box::pin(async move {
            let wall = call(
                context,
                Method::GET,
                "/api/oauth/usage?at_wall=1&skip_spend=1",
                None,
            )
            .await?;
            let grants = call(
                context,
                Method::GET,
                "/api/oauth/usage?cedar_ember=1&skip_spend=1",
                None,
            )
            .await?;
            let mut options = vec![weekly_option(wall.get("juniper_tide"))?];
            options.extend(grant_options(grants.get("cedar_ember"), unix_now_ms())?);
            // Weekly eligibility and account grants are different offers, not
            // one interchangeable balance. Keep their counts on each option.
            Ok(QuotaResetCredits {
                credit_expirations_ms: Vec::new(),
                available_count: None,
                expires_at_ms: None,
                options,
            })
        })
    }

    fn reset<'a>(
        &'a self,
        context: CredentialContext<'a>,
        request: QuotaResetRequest<'a>,
    ) -> OperationFuture<'a, QuotaResetResult> {
        Box::pin(async move {
            let program = request
                .program
                .ok_or_else(|| ChannelError::InvalidConfig("select a reset program".into()))?;
            if !matches!(program, "juniper_tide" | "cedar_ember") {
                return Err(ChannelError::InvalidConfig("unknown reset program".into()));
            }
            // Revalidate only the selected program. An unrelated offer's
            // rate limit or malformed response must not block this redemption.
            let options = if program == "juniper_tide" {
                let status = call(
                    context,
                    Method::GET,
                    "/api/oauth/usage?at_wall=1&skip_spend=1",
                    None,
                )
                .await?;
                vec![weekly_option(status.get("juniper_tide"))?]
            } else {
                let status = call(
                    context,
                    Method::GET,
                    "/api/oauth/usage?cedar_ember=1&skip_spend=1",
                    None,
                )
                .await?;
                grant_options(status.get("cedar_ember"), unix_now_ms())?
            };
            let choice = options.iter().find(|option| {
                option.program == program && option.grant_id.as_deref() == request.grant_id
            });
            let Some(choice) = choice.filter(|option| option.usable) else {
                return Ok(QuotaResetResult {
                    outcome: QuotaResetOutcome::Ineligible,
                    windows_reset: None,
                    clears: Vec::new(),
                    reason: Some(
                        choice
                            .and_then(|option| option.ineligible_reason.clone())
                            .unwrap_or_else(|| "unavailable".into()),
                    ),
                });
            };
            let org = match fact(&context.credential, "organization_uuid") {
                Some(org) => org.to_owned(),
                None => call(context, Method::GET, "/api/oauth/profile", None)
                    .await?
                    .pointer("/organization/uuid")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid_response("profile has no organization uuid"))?
                    .to_owned(),
            };
            if org.is_empty() || !org.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return Err(ChannelError::InvalidCredential);
            }
            let mut body = json!({"program": program});
            if program == "cedar_ember" {
                body["grant_id"] = json!(choice.grant_id);
                body["request_id"] = json!(request.redeem_request_id);
            }
            let result = call(
                context,
                Method::POST,
                &format!("/api/organizations/{org}/reset_rate_limits"),
                Some(body),
            )
            .await?;
            let outcome = match result.get("result").and_then(Value::as_str) {
                Some("reset") => QuotaResetOutcome::Reset,
                Some("already_used") => QuotaResetOutcome::AlreadyRedeemed,
                Some("not_limited") => QuotaResetOutcome::NothingToReset,
                Some("ineligible") => QuotaResetOutcome::Ineligible,
                Some("unavailable") => QuotaResetOutcome::Unavailable,
                Some("cooldown") => QuotaResetOutcome::Cooldown,
                _ => return Err(invalid_response("unknown reset outcome")),
            };
            Ok(QuotaResetResult {
                outcome,
                windows_reset: result
                    .get("cleared")
                    .and_then(Value::as_array)
                    .map(|cleared| cleared.len() as u64),
                clears: choice.clears.clone(),
                reason: result
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
    }
}
