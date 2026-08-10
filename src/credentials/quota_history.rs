//! Persistent local accounting for live upstream quota-window observations.
//!
//! The upstream is contacted only by the existing explicit credential usage
//! action. Each observation checkpoints locally settled requests into one open
//! row per stable window key. When a reset boundary changes, the old row is
//! finalized and kept permanently; there is deliberately no intra-cycle time
//! series.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::channel::{
    Channel, RateLimitResetCredits, UsageCredits, UsageSnapshot, UsageWindow,
    UsageWindowBoundaryConfidence, UsageWindowBoundarySource, UsageWindowDescriptor,
    UsageWindowMeter, UsageWindowScope,
};
use crate::credentials::history::{CredentialUsageModelTotals, CredentialUsageTotals};
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{
    Credential, CredentialQuotaCycle, CredentialQuotaCycleInput, CredentialQuotaCycleModel,
    CredentialQuotaCycleModelInput, Provider, UsageModelSummary,
};
use crate::store::persistence::{CredentialQuotaCycleQuery, UsageQuery};

/// Live upstream response augmented with the latest permanent local aggregate
/// for each normalized window.
#[derive(Debug, Serialize)]
pub struct CredentialUsageSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub windows: Vec<CredentialUsageWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<UsageCredits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_reset_credits: Option<RateLimitResetCredits>,
    pub raw: Value,
}

#[derive(Debug, Serialize)]
pub struct CredentialUsageWindow {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at_unix: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_usage: Option<UsageWindowLocalUsage>,
}

#[derive(Debug, Serialize)]
pub struct UsageWindowLocalUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<i64>,
    pub observed_at: i64,
    pub coverage: String,
    pub scope: String,
    pub totals: CredentialUsageTotals,
    pub by_model: Vec<CredentialUsageModelTotals>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_capacity: Option<EquivalentCapacity>,
}

#[derive(Debug, Serialize)]
pub struct EquivalentCapacity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<String>,
    pub basis: &'static str,
}

/// Persist one explicit live snapshot and return its enriched wire view.
/// Persistence failures are returned so the caller can log them while still
/// returning the upstream snapshot without local annotations.
pub async fn observe_snapshot(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    channel: &Arc<dyn Channel>,
    snapshot: UsageSnapshot,
) -> anyhow::Result<CredentialUsageSnapshot> {
    let now = crate::util::time::unix_now();
    let mut cycles = Vec::with_capacity(snapshot.windows.len());
    for (index, window) in snapshot.windows.iter().enumerate() {
        let descriptor = channel.describe_usage_window(&snapshot, index);
        cycles.push(observe_window(state, provider, credential, window, descriptor, now).await?);
    }

    let mut enriched = Vec::with_capacity(snapshot.windows.len());
    for (window, cycle) in snapshot.windows.into_iter().zip(cycles) {
        let local_usage = cycle_local_usage(state.persistence.as_ref(), &cycle).await?;
        enriched.push(window_with_local(window, Some(local_usage)));
    }
    Ok(CredentialUsageSnapshot {
        plan: snapshot.plan,
        windows: enriched,
        credits: snapshot.credits,
        rate_limit_reset_credits: snapshot.rate_limit_reset_credits,
        raw: snapshot.raw,
    })
}

/// Preserve the upstream response when the history store is unavailable.
pub fn without_local(snapshot: UsageSnapshot) -> CredentialUsageSnapshot {
    CredentialUsageSnapshot {
        plan: snapshot.plan,
        windows: snapshot
            .windows
            .into_iter()
            .map(|window| window_with_local(window, None))
            .collect(),
        credits: snapshot.credits,
        rate_limit_reset_credits: snapshot.rate_limit_reset_credits,
        raw: snapshot.raw,
    }
}

fn window_with_local(
    window: UsageWindow,
    local_usage: Option<UsageWindowLocalUsage>,
) -> CredentialUsageWindow {
    CredentialUsageWindow {
        name: window.name,
        label: window.label,
        used_percent: window.used_percent,
        used: window.used,
        limit: window.limit,
        resets_at: window.resets_at,
        resets_at_unix: window.resets_at_unix,
        window_seconds: window.window_seconds,
        local_usage,
    }
}

async fn observe_window(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    window: &UsageWindow,
    mut descriptor: UsageWindowDescriptor,
    now: i64,
) -> anyhow::Result<CredentialQuotaCycle> {
    if descriptor.key.trim().is_empty() {
        descriptor.key = window.name.clone();
    }
    let period_end = reset_unix(window).or_else(|| {
        descriptor
            .period_start_unix
            .zip(window.window_seconds)
            .map(|(start, duration)| start.saturating_add(duration))
    });
    let used_percent = upstream_percent(window);
    let upstream_used = window.used.and_then(decimal_from_f64);
    let upstream_limit = window.limit.and_then(decimal_from_f64);

    let mut existing = state
        .persistence
        .get_open_credential_quota_cycle(credential.id, &descriptor.key)
        .await?;
    let mut continued_from_previous_cycle = false;
    if let Some(open) = existing.as_ref()
        && crossed_boundary(open, descriptor.period_start_unix, period_end, now)
    {
        let boundary = descriptor
            .period_start_unix
            .or(open.period_end)
            .unwrap_or(now)
            .min(now);
        let checkpointed = checkpoint_cycle(state.persistence.as_ref(), open, boundary).await?;
        state
            .persistence
            .finalize_credential_quota_cycle(checkpointed.id, Some(boundary), "natural_reset", now)
            .await?;
        existing = None;
        continued_from_previous_cycle = true;
    }

    let (scope_kind, scope_json) = scope_parts(&descriptor.scope);
    let meter_kind = meter_name(descriptor.meter).to_owned();
    let boundary_source = boundary_source_name(descriptor.boundary_source).to_owned();
    let boundary_confidence = boundary_confidence_name(descriptor.boundary_confidence).to_owned();
    let scope_is_local = matches!(
        descriptor.scope,
        UsageWindowScope::All | UsageWindowScope::Models { .. }
    );

    let base = if let Some(open) = existing {
        checkpoint_cycle(state.persistence.as_ref(), &open, now).await?
    } else {
        let aggregate_from = descriptor
            .period_start_unix
            .filter(|start| *start <= now)
            .unwrap_or(now);
        let input = CredentialQuotaCycleInput {
            credential_id: credential.id,
            provider_id: provider.id,
            channel: provider.channel.clone(),
            window_key: descriptor.key.clone(),
            name: window.name.clone(),
            label: window.label.clone(),
            scope_kind: scope_kind.clone(),
            scope_json: scope_json.clone(),
            meter_kind: meter_kind.clone(),
            period_start: descriptor.period_start_unix,
            period_end,
            boundary_source: boundary_source.clone(),
            boundary_confidence: boundary_confidence.clone(),
            last_observed_at: Some(now),
            used_percent,
            upstream_used,
            upstream_limit,
            coverage: if scope_is_local {
                if continued_from_previous_cycle {
                    "complete".to_owned()
                } else {
                    "partial".to_owned()
                }
            } else {
                "unknown".to_owned()
            },
            requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            image_output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_5m_tokens: 0,
            cache_creation_30m_tokens: 0,
            cache_creation_1h_tokens: 0,
            cost: Decimal::ZERO,
            estimated_tokens: None,
            estimated_cost: None,
            aggregated_through: Some(aggregate_from),
        };
        let created = state
            .persistence
            .upsert_credential_quota_cycle(input)
            .await?;
        checkpoint_cycle(state.persistence.as_ref(), &created, now).await?
    };

    let mut input = cycle_input(&base);
    input.provider_id = provider.id;
    input.channel = provider.channel.clone();
    input.window_key = descriptor.key;
    input.name = window.name.clone();
    input.label = window.label.clone();
    input.scope_kind = scope_kind;
    input.scope_json = scope_json;
    input.meter_kind = meter_kind;
    input.period_start = descriptor.period_start_unix;
    input.period_end = period_end;
    input.boundary_source = boundary_source;
    input.boundary_confidence = boundary_confidence;
    input.last_observed_at = Some(now);
    input.used_percent = used_percent;
    input.upstream_used = upstream_used;
    input.upstream_limit = upstream_limit;
    set_equivalent_capacity(&mut input);
    state.persistence.upsert_credential_quota_cycle(input).await
}

fn crossed_boundary(
    open: &CredentialQuotaCycle,
    next_start: Option<i64>,
    next_end: Option<i64>,
    now: i64,
) -> bool {
    if let (Some(old_start), Some(new_start)) = (open.period_start, next_start)
        && old_start != new_start
        && new_start <= now
    {
        return true;
    }
    if let (Some(old_end), Some(new_end)) = (open.period_end, next_end)
        && old_end != new_end
        && now >= old_end
    {
        return true;
    }
    false
}

/// Checkpoint every open window through an exclusive unix-second boundary.
/// Retention and explicit raw-usage clearing call this before deleting rows.
pub async fn checkpoint_open_cycles(state: &AppState, through: i64) -> anyhow::Result<()> {
    let cycles = state
        .persistence
        .query_credential_quota_cycles(&CredentialQuotaCycleQuery {
            status: Some("open".to_owned()),
            ..Default::default()
        })
        .await?;
    for cycle in cycles {
        checkpoint_cycle(state.persistence.as_ref(), &cycle, through).await?;
    }
    Ok(())
}

/// Close all currently open windows after an upstream manual reset. The next
/// explicit live refresh starts the new cycles; no upstream polling is added.
pub async fn finalize_after_manual_reset(
    state: &AppState,
    credential_id: i64,
    through: i64,
) -> anyhow::Result<()> {
    let cycles = state
        .persistence
        .query_credential_quota_cycles(&CredentialQuotaCycleQuery {
            credential_id: Some(credential_id),
            status: Some("open".to_owned()),
            ..Default::default()
        })
        .await?;
    for cycle in cycles {
        let checkpointed = checkpoint_cycle(state.persistence.as_ref(), &cycle, through).await?;
        state
            .persistence
            .finalize_credential_quota_cycle(
                checkpointed.id,
                Some(through),
                "manual_reset",
                through,
            )
            .await?;
    }
    Ok(())
}

async fn checkpoint_cycle(
    persistence: &dyn PersistenceBackend,
    cycle: &CredentialQuotaCycle,
    through: i64,
) -> anyhow::Result<CredentialQuotaCycle> {
    // Callers may hold a snapshot captured before another refresh completed.
    // Always derive the next half-open interval and model baseline from the
    // newest stored row.
    let Some(cycle) = persistence.get_credential_quota_cycle(cycle.id).await? else {
        anyhow::bail!("credential quota cycle {} vanished", cycle.id);
    };
    // Never attribute traffic after a known reset/end to the old cycle, even
    // if the operator does not refresh the upstream snapshot until later.
    let through = cycle.period_end.map_or(through, |end| through.min(end));
    let from = cycle
        .aggregated_through
        .or(cycle.period_start)
        .unwrap_or(cycle.created_at);
    if through <= from || cycle.status != "open" {
        return Ok(cycle);
    }

    let allowed_models = locally_accountable_models(&cycle);
    let mut input = cycle_input(&cycle);
    input.aggregated_through = Some(through);
    let Some(allowed_models) = allowed_models else {
        input.coverage = "unknown".to_owned();
        return persistence.upsert_credential_quota_cycle(input).await;
    };

    // Capture the cumulative model baseline before doing any writes. Two
    // refreshes that start from the same watermark will then replace each
    // model with the same target instead of adding the interval twice.
    let model_baseline = persistence
        .list_credential_quota_cycle_models(cycle.id)
        .await?;

    // Persistence explorer filters use inclusive endpoints; subtracting one
    // implements the history contract's half-open [from, through) interval.
    let summaries = persistence
        .summarize_usages_by_model(&UsageQuery {
            at_from: Some(from),
            at_to: Some(through.saturating_sub(1)),
            provider_id: Some(cycle.provider_id),
            credential_id: Some(cycle.credential_id),
            ..Default::default()
        })
        .await?;
    let summaries: Vec<_> = summaries
        .into_iter()
        .filter(|summary| model_allowed(&allowed_models, summary.model.as_deref()))
        .collect();
    for summary in &summaries {
        add_model_summary(&mut input, summary);
    }
    set_equivalent_capacity(&mut input);
    let updated = persistence.upsert_credential_quota_cycle(input).await?;
    if updated.status == "open" && updated.aggregated_through == Some(through) {
        add_cycle_models(persistence, &updated, model_baseline, summaries).await?;
    }
    Ok(updated)
}

/// `None` means the descriptor cannot be mapped to locally recorded requests.
/// `Some(None)` means all models; `Some(Some(set))` means an exact model set.
fn locally_accountable_models(cycle: &CredentialQuotaCycle) -> Option<Option<BTreeSet<String>>> {
    match cycle.scope_kind.as_str() {
        "all" => Some(None),
        "models" => {
            let models = cycle
                .scope_json
                .as_ref()
                .and_then(|value| value.get("models").or(Some(value)))
                .and_then(Value::as_array)?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            Some(Some(models))
        }
        _ => None,
    }
}

fn model_allowed(allowed: &Option<BTreeSet<String>>, model: Option<&str>) -> bool {
    match allowed {
        None => true,
        Some(models) => model.is_some_and(|model| {
            let model_lower = model.to_ascii_lowercase();
            models.iter().any(|selector| {
                selector.eq_ignore_ascii_case(model)
                    // Some upstreams expose a model family ("sonnet"/"opus")
                    // rather than a versioned wire id. Treat that selector as
                    // a family match while retaining exact matching for the
                    // common fully-qualified case.
                    || model_lower.contains(&selector.to_ascii_lowercase())
            })
        }),
    }
}

async fn add_cycle_models(
    persistence: &dyn PersistenceBackend,
    cycle: &CredentialQuotaCycle,
    baseline: Vec<CredentialQuotaCycleModel>,
    deltas: Vec<UsageModelSummary>,
) -> anyhow::Result<()> {
    let mut existing: BTreeMap<_, _> = baseline
        .into_iter()
        .map(|row| (row.model.clone(), row))
        .collect();
    for delta in deltas {
        let model = delta
            .model
            .clone()
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| "unknown".to_owned());
        let prior = existing.remove(&model);
        let input = CredentialQuotaCycleModelInput {
            cycle_id: cycle.id,
            model,
            requests: prior.as_ref().map_or(0, |v| v.requests) + delta.requests,
            input_tokens: prior.as_ref().map_or(0, |v| v.input_tokens) + delta.input_tokens,
            output_tokens: prior.as_ref().map_or(0, |v| v.output_tokens) + delta.output_tokens,
            image_output_tokens: prior.as_ref().map_or(0, |v| v.image_output_tokens)
                + delta.image_output_tokens,
            cache_read_tokens: prior.as_ref().map_or(0, |v| v.cache_read_tokens)
                + delta.cache_read_tokens,
            cache_creation_5m_tokens: prior.as_ref().map_or(0, |v| v.cache_creation_5m_tokens)
                + delta.cache_creation_5m_tokens,
            cache_creation_30m_tokens: prior.as_ref().map_or(0, |v| v.cache_creation_30m_tokens)
                + delta.cache_creation_30m_tokens,
            cache_creation_1h_tokens: prior.as_ref().map_or(0, |v| v.cache_creation_1h_tokens)
                + delta.cache_creation_1h_tokens,
            cost: prior.as_ref().map_or(Decimal::ZERO, |v| v.cost) + delta.cost,
        };
        persistence
            .upsert_credential_quota_cycle_model(input)
            .await?;
    }
    Ok(())
}

fn add_model_summary(input: &mut CredentialQuotaCycleInput, summary: &UsageModelSummary) {
    input.requests = input.requests.saturating_add(summary.requests);
    input.input_tokens = input.input_tokens.saturating_add(summary.input_tokens);
    input.output_tokens = input.output_tokens.saturating_add(summary.output_tokens);
    input.image_output_tokens = input
        .image_output_tokens
        .saturating_add(summary.image_output_tokens);
    input.cache_read_tokens = input
        .cache_read_tokens
        .saturating_add(summary.cache_read_tokens);
    input.cache_creation_5m_tokens = input
        .cache_creation_5m_tokens
        .saturating_add(summary.cache_creation_5m_tokens);
    input.cache_creation_30m_tokens = input
        .cache_creation_30m_tokens
        .saturating_add(summary.cache_creation_30m_tokens);
    input.cache_creation_1h_tokens = input
        .cache_creation_1h_tokens
        .saturating_add(summary.cache_creation_1h_tokens);
    input.cost += summary.cost;
}

fn cycle_input(cycle: &CredentialQuotaCycle) -> CredentialQuotaCycleInput {
    CredentialQuotaCycleInput {
        credential_id: cycle.credential_id,
        provider_id: cycle.provider_id,
        channel: cycle.channel.clone(),
        window_key: cycle.window_key.clone(),
        name: cycle.name.clone(),
        label: cycle.label.clone(),
        scope_kind: cycle.scope_kind.clone(),
        scope_json: cycle.scope_json.clone(),
        meter_kind: cycle.meter_kind.clone(),
        period_start: cycle.period_start,
        period_end: cycle.period_end,
        boundary_source: cycle.boundary_source.clone(),
        boundary_confidence: cycle.boundary_confidence.clone(),
        last_observed_at: cycle.last_observed_at,
        used_percent: cycle.used_percent,
        upstream_used: cycle.upstream_used,
        upstream_limit: cycle.upstream_limit,
        coverage: cycle.coverage.clone(),
        requests: cycle.requests,
        input_tokens: cycle.input_tokens,
        output_tokens: cycle.output_tokens,
        image_output_tokens: cycle.image_output_tokens,
        cache_read_tokens: cycle.cache_read_tokens,
        cache_creation_5m_tokens: cycle.cache_creation_5m_tokens,
        cache_creation_30m_tokens: cycle.cache_creation_30m_tokens,
        cache_creation_1h_tokens: cycle.cache_creation_1h_tokens,
        cost: cycle.cost,
        estimated_tokens: cycle.estimated_tokens,
        estimated_cost: cycle.estimated_cost,
        aggregated_through: cycle.aggregated_through,
    }
}

fn set_equivalent_capacity(input: &mut CredentialQuotaCycleInput) {
    input.estimated_tokens = None;
    input.estimated_cost = None;
    if !matches!(input.scope_kind.as_str(), "all" | "models") {
        return;
    }
    let Some(percent) = input.used_percent.filter(|value| *value > Decimal::ZERO) else {
        return;
    };
    let fraction = percent / Decimal::ONE_HUNDRED;
    if fraction <= Decimal::ZERO {
        return;
    }
    let tokens = total_tokens(input);
    if tokens > 0 {
        input.estimated_tokens = (Decimal::from(tokens) / fraction).round().to_i64();
    }
    if input.cost > Decimal::ZERO {
        input.estimated_cost = Some(input.cost / fraction);
    }
}

fn total_tokens(input: &CredentialQuotaCycleInput) -> i64 {
    input
        .input_tokens
        .saturating_add(input.output_tokens)
        .saturating_add(input.image_output_tokens)
        .saturating_add(input.cache_read_tokens)
        .saturating_add(input.cache_creation_5m_tokens)
        .saturating_add(input.cache_creation_30m_tokens)
        .saturating_add(input.cache_creation_1h_tokens)
}

async fn cycle_local_usage(
    persistence: &dyn PersistenceBackend,
    cycle: &CredentialQuotaCycle,
) -> anyhow::Result<UsageWindowLocalUsage> {
    let mut by_model: Vec<_> = persistence
        .list_credential_quota_cycle_models(cycle.id)
        .await?
        .iter()
        .map(model_totals)
        .collect();
    by_model.sort_by(|left, right| {
        right
            .totals
            .total_tokens
            .cmp(&left.totals.total_tokens)
            .then_with(|| left.model.cmp(&right.model))
    });
    let estimated_capacity = if cycle.estimated_tokens.is_some() || cycle.estimated_cost.is_some() {
        Some(EquivalentCapacity {
            tokens: cycle.estimated_tokens,
            cost_usd: cycle
                .estimated_cost
                .map(|cost| cost.normalize().to_string()),
            basis: "current_mix",
        })
    } else {
        None
    };
    Ok(UsageWindowLocalUsage {
        period_start: cycle.period_start,
        observed_at: cycle.last_observed_at.unwrap_or(cycle.updated_at),
        coverage: cycle.coverage.clone(),
        scope: cycle.scope_kind.clone(),
        totals: CredentialUsageTotals::new(
            cycle.requests,
            cycle.input_tokens,
            cycle.output_tokens,
            cycle.image_output_tokens,
            cycle.cache_read_tokens,
            cycle.cache_creation_5m_tokens,
            cycle.cache_creation_30m_tokens,
            cycle.cache_creation_1h_tokens,
            cycle.cost,
        ),
        by_model,
        estimated_capacity,
    })
}

fn model_totals(row: &CredentialQuotaCycleModel) -> CredentialUsageModelTotals {
    CredentialUsageModelTotals {
        model: row.model.clone(),
        totals: CredentialUsageTotals::new(
            row.requests,
            row.input_tokens,
            row.output_tokens,
            row.image_output_tokens,
            row.cache_read_tokens,
            row.cache_creation_5m_tokens,
            row.cache_creation_30m_tokens,
            row.cache_creation_1h_tokens,
            row.cost,
        ),
    }
}

fn scope_parts(scope: &UsageWindowScope) -> (String, Option<Value>) {
    match scope {
        UsageWindowScope::All => ("all".to_owned(), None),
        UsageWindowScope::Models { models } => {
            ("models".to_owned(), Some(json!({ "models": models })))
        }
        UsageWindowScope::Feature { feature } => {
            ("feature".to_owned(), Some(json!({ "feature": feature })))
        }
        UsageWindowScope::Unknown => ("unknown".to_owned(), None),
    }
}

fn meter_name(meter: UsageWindowMeter) -> &'static str {
    match meter {
        UsageWindowMeter::Tokens => "tokens",
        UsageWindowMeter::Requests => "requests",
        UsageWindowMeter::Credits => "credits",
        UsageWindowMeter::Usd => "usd",
        UsageWindowMeter::Opaque => "opaque",
    }
}

fn boundary_source_name(source: UsageWindowBoundarySource) -> &'static str {
    match source {
        UsageWindowBoundarySource::Upstream => "upstream",
        UsageWindowBoundarySource::ResetAndDuration => "reset_and_duration",
        UsageWindowBoundarySource::KnownWindow => "known_window",
        UsageWindowBoundarySource::ResetOnly => "reset_only",
        UsageWindowBoundarySource::Unknown => "unknown",
    }
}

fn boundary_confidence_name(confidence: UsageWindowBoundaryConfidence) -> &'static str {
    match confidence {
        UsageWindowBoundaryConfidence::Exact => "exact",
        UsageWindowBoundaryConfidence::Derived => "derived",
        UsageWindowBoundaryConfidence::Partial => "partial",
        UsageWindowBoundaryConfidence::Unknown => "unknown",
    }
}

fn upstream_percent(window: &UsageWindow) -> Option<Decimal> {
    window.used_percent.and_then(decimal_from_f64).or_else(|| {
        let used = window.used.and_then(decimal_from_f64)?;
        let limit = window.limit.and_then(decimal_from_f64)?;
        (limit > Decimal::ZERO).then(|| used / limit * Decimal::ONE_HUNDRED)
    })
}

fn decimal_from_f64(value: f64) -> Option<Decimal> {
    value
        .is_finite()
        .then(|| value.to_string().parse().ok())
        .flatten()
}

fn reset_unix(window: &UsageWindow) -> Option<i64> {
    window.resets_at_unix.or_else(|| {
        let raw = window.resets_at.as_deref()?;
        crate::channel::usage_descriptor::iso_to_unix(raw).or_else(|| {
            // Copilot reports a date-only reset. Treat it as UTC midnight and
            // reuse the edge-safe parser shared by channel descriptors.
            (!raw.contains(['T', 't']))
                .then(|| format!("{raw}T00:00:00Z"))
                .and_then(|value| crate::channel::usage_descriptor::iso_to_unix(&value))
        })
    })
}
