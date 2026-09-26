//! Claude Code 2.1.280 reset eligibility and explicit redemption.
//! These GETs deliberately do not run as part of the plain usage query.

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
    grants: Vec<Grant>,
    next_grant_id: Option<String>,
    cooldown_until: Option<String>,
}

#[derive(Deserialize)]
struct Grant {
    id: String,
    label: Option<String>,
    resets_left: u64,
    starts_at: Option<String>,
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
}
fn requires_limit() -> bool {
    true
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
    Ok(status
        .grants
        .into_iter()
        .map(|grant| {
            let expires_at_ms = instant(grant.ends_at.as_deref());
            let next_available_at_ms = instant(status.cooldown_until.as_deref());
            let reason = if !status.eligible {
                Some(
                    status
                        .ineligible_reason
                        .clone()
                        .unwrap_or_else(|| "ineligible".into()),
                )
            } else if grant.resets_left == 0 {
                Some("no_grant".into())
            } else if grant.paused {
                Some("paused".into())
            } else if expires_at_ms.is_some_and(|end| end <= now) {
                Some("expired".into())
            } else if instant(grant.starts_at.as_deref()).is_some_and(|start| start > now) {
                Some("not_started".into())
            } else if next_available_at_ms.is_some_and(|end| end > now) {
                Some("cooldown".into())
            } else if status.next_grant_id.as_deref() != Some(grant.id.as_str()) {
                Some("not_next_grant".into())
            } else if !grant.blocking.is_empty() {
                Some("uncovered_limit".into())
            } else if grant.use_requires_limit && !status.at_limit {
                Some("not_limited".into())
            } else if !grant.usable_now {
                Some("unavailable".into())
            } else {
                None
            };
            QuotaResetOption {
                program: "cedar_ember".into(),
                grant_id: Some(grant.id),
                label: grant.label,
                available_count: Some(grant.resets_left),
                expires_at_ms,
                next_available_at_ms,
                usable: reason.is_none(),
                ineligible_reason: reason,
                clears: grant.clears,
            }
        })
        .collect())
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
            // Revalidate exactly what the user confirmed, never switch grants.
            let status = self.credits(context).await?;
            let choice = status.options.iter().find(|option| {
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
