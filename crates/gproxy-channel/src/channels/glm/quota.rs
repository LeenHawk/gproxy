//! Official zai-org/zai-coding-plugins:
//! plugins/glm-plan-usage/skills/usage-query-skill/scripts/query-usage.mjs
//! Authorization is the raw API key, unlike the generation API's bearer.
use super::{DEFAULT_BASE_URL, Glm};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaEntry, QuotaQuery,
    QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use crate::channels::shared::compatible::ability::{decimal, require_success, send};
use http::{HeaderMap, HeaderValue, Method, header};
use serde_json::Value;

impl QuotaQuery for Glm {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            if !self.coding_plan {
                return Err(ChannelError::InvalidConfig(
                    "GLM quota query requires the glmcode channel".into(),
                ));
            }
            let key = ctx
                .credential
                .secret
                .get("api_key")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or(ChannelError::InvalidCredential)?;
            let base = ctx
                .provider
                .base_url
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/');
            let mut headers = HeaderMap::new();
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
            );
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            let (status, _, body) = send(
                ctx.client,
                Method::GET,
                &format!("{base}/api/monitor/usage/quota/limit"),
                headers,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let value: Value = serde_json::from_slice(&body)
                .map_err(|e| ChannelError::InvalidResponse(e.to_string()))?;
            if value.get("success").and_then(Value::as_bool) == Some(false)
                || value
                    .get("code")
                    .is_some_and(|code| !matches!(code.as_i64(), Some(0 | 200)))
            {
                return Err(ChannelError::InvalidResponse(
                    "GLM quota request rejected".into(),
                ));
            }
            let limits = value
                .get("data")
                .unwrap_or(&value)
                .get("limits")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    ChannelError::InvalidResponse("GLM quota reply missing limits".into())
                })?;
            let entries = limits
                .iter()
                .filter_map(|item| {
                    let kind = item.get("type").and_then(Value::as_str)?;
                    let unit = item.get("unit").and_then(Value::as_u64);
                    let number = item.get("number").and_then(Value::as_u64);
                    let label = match (kind, unit, number) {
                        ("TOKENS_LIMIT" | "CREDIT_LIMIT", Some(3), Some(5)) => {
                            "GLM Coding Plan (5 hours)".into()
                        }
                        ("TOKENS_LIMIT" | "CREDIT_LIMIT", Some(6), _) => {
                            "GLM Coding Plan (weekly)".into()
                        }
                        ("TIME_LIMIT", _, _) => "GLM MCP (monthly)".into(),
                        _ => format!("GLM {kind}"),
                    };
                    let used = item.get("currentValue").and_then(decimal);
                    let limit = item.get("usage").and_then(decimal);
                    let id = format!(
                        "glm_{kind}_{}_{}",
                        unit.unwrap_or_default(),
                        number.unwrap_or_default()
                    );
                    Some(QuotaEntry {
                        id: id.clone(),
                        source_id: id,
                        label: Some(label),
                        subject: QuotaSubject::Account,
                        model_scope: if matches!(kind, "TOKENS_LIMIT" | "CREDIT_LIMIT") {
                            QuotaScope::All
                        } else {
                            QuotaScope::Unknown
                        },
                        value: QuotaValue::Window(QuotaAllowance {
                            used,
                            limit,
                            remaining: item
                                .get("remaining")
                                .and_then(decimal)
                                .or_else(|| limit.zip(used).map(|(l, u)| l - u)),
                            used_percent: item.get("percentage").and_then(decimal),
                            period_end_ms: item.get("nextResetTime").and_then(Value::as_i64),
                            ..Default::default()
                        }),
                    })
                })
                .collect();
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries,
            })
        })
    }
}
