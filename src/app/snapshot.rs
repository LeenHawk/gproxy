//! The control-plane snapshot (§7.2): the sole `ArcSwap` snapshot read on the
//! hot path. Fully rebuildable from persistence (boot + invalidation). Holds no
//! counters/sessions/health — those are redis-direct or separate local state.
//!
//! M2/M3 extend THIS struct + [`ControlPlaneSnapshot::build`], never a parallel
//! snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use regex::Regex;

use crate::app::models_index::{self, ExposedModel};
use crate::process::CompiledRule;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{
    Alias, Credential, Org, Provider, ProviderModel, Quota, RateLimit, Route, RouteMember, Scope,
    Team, User, UserKey,
};
use crate::transform::routing::{CompiledRoutingRule, RoutingRuleSpec};

/// Immutable control-plane snapshot.
pub struct ControlPlaneSnapshot {
    pub providers_by_name: HashMap<String, Arc<Provider>>,
    pub providers_by_id: HashMap<i64, Arc<Provider>>,
    pub routes_by_name: HashMap<String, Arc<ResolvedRoute>>,
    /// Alias scope (`*` or provider name) → compiled model alias rules.
    pub aliases_by_provider: HashMap<String, Arc<Vec<CompiledAlias>>>,
    /// api-key digest → identity (auth without a DB hit). ENABLED keys + users.
    pub keys_by_digest: HashMap<String, Arc<KeyIdentity>>,
    /// provider id → ENABLED credential pool.
    pub credentials_by_provider: HashMap<i64, Vec<Arc<Credential>>>,
    /// provider id → models.
    pub models_by_provider: HashMap<i64, Vec<Arc<ProviderModel>>>,
    /// provider id → expansion of `provider_models` rows (enabled, variants
    /// applied) for list-side serving (§8-B).
    pub exposed_models_by_provider: HashMap<i64, Arc<Vec<ExposedModel>>>,
    /// provider id → variant full id → base id (request-side suffix strip).
    pub variant_base_by_provider: HashMap<i64, Arc<HashMap<String, String>>>,
    /// provider id → compiled transform-dispatch rules (§8-B2 `routing_rules`).
    pub routing_rules_by_provider: HashMap<i64, Arc<Vec<CompiledRoutingRule>>>,
    /// provider id → flattened, apply-ordered process rules (§8-B2 rule sets,
    /// via `provider_rule_sets`).
    pub rule_sets_by_provider: HashMap<i64, Arc<Vec<CompiledRule>>>,
    /// All orgs (incl. disabled) keyed by id; authz checks `enabled` itself.
    pub orgs_by_id: HashMap<i64, Arc<Org>>,
    /// All teams keyed by id.
    pub teams_by_id: HashMap<i64, Arc<Team>>,
    /// (scope, scope_id) → permission glob patterns (§8-C union semantics).
    pub permissions_by_scope: HashMap<(Scope, i64), Arc<Vec<String>>>,
    /// (scope, scope_id) → rate-limit rows.
    pub rate_limits_by_scope: HashMap<(Scope, i64), Arc<Vec<RateLimit>>>,
    /// (scope, scope_id) → quota row.
    pub quotas_by_scope: HashMap<(Scope, i64), Arc<Quota>>,
    /// Instance usage/log toggles (§8-E), snapshot-resident so the hot path
    /// reads them without a DB hit; hot-reloaded via §7.2 invalidation.
    pub log_settings: LogSettings,
    /// Instance-level default upstream proxy (`instance_settings.proxy`,
    /// Console-editable). The global fallback for [`effective_proxy`]
    /// (per-credential / per-provider proxies still override it); hot-reloaded
    /// via §7.2 so changing it in the Console applies without a restart.
    pub proxy: Option<String>,
    /// Console-editable self-update channel override. `None` falls back to the
    /// server startup default.
    pub update_channel: Option<String>,
    /// Bumped on each rebuild.
    pub version: u64,
}

/// Hot-path view of the `instance_settings` usage/log flags (§8-E, §14.3).
/// [`Default`] applies when no settings row exists: usage recording ON,
/// request capture OFF, redaction ON.
#[derive(Debug, Clone)]
pub struct LogSettings {
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub disable_log_redaction: bool,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            enable_usage: true,
            enable_upstream_log: false,
            enable_upstream_log_body: false,
            enable_downstream_log: false,
            enable_downstream_log_body: false,
            disable_log_redaction: false,
        }
    }
}

/// A route plus its members, pre-sorted by `(tier asc, weight desc)`.
pub struct ResolvedRoute {
    pub route: Route,
    pub members: Vec<RouteMember>,
}

/// Auth identity resolved from a user key (`org_id`/`team_id` used by M3 authz).
pub struct KeyIdentity {
    pub user_key: UserKey,
    pub user: User,
}

impl ControlPlaneSnapshot {
    /// An empty snapshot (used transiently at boot before the first build).
    pub fn empty(version: u64) -> Self {
        Self {
            providers_by_name: HashMap::new(),
            providers_by_id: HashMap::new(),
            routes_by_name: HashMap::new(),
            aliases_by_provider: HashMap::new(),
            keys_by_digest: HashMap::new(),
            credentials_by_provider: HashMap::new(),
            models_by_provider: HashMap::new(),
            exposed_models_by_provider: HashMap::new(),
            variant_base_by_provider: HashMap::new(),
            routing_rules_by_provider: HashMap::new(),
            rule_sets_by_provider: HashMap::new(),
            orgs_by_id: HashMap::new(),
            teams_by_id: HashMap::new(),
            permissions_by_scope: HashMap::new(),
            rate_limits_by_scope: HashMap::new(),
            quotas_by_scope: HashMap::new(),
            log_settings: LogSettings::default(),
            proxy: None,
            update_channel: None,
            version,
        }
    }

    /// Full reload from persistence (boot + invalidation). On wasm the backend
    /// trait is `?Send`, so this future is non-Send — await it directly, never
    /// on a `Send`-requiring spawn.
    pub async fn build(db: &dyn PersistenceBackend, version: u64) -> anyhow::Result<Self> {
        let mut snap = Self::empty(version);

        // rule sets compile once; providers attach by id below
        let mut compiled_sets: HashMap<i64, Vec<CompiledRule>> = HashMap::new();
        for set in db.list_rule_sets().await?.into_iter().filter(|s| s.enabled) {
            let rules = db.list_rules(set.id).await?;
            compiled_sets.insert(set.id, crate::process::compile_rules(&rules));
        }

        // providers + their credentials/models
        for provider in db.list_providers().await? {
            let pid = provider.id;
            let creds = db
                .list_credentials(pid)
                .await?
                .into_iter()
                .filter(|c| c.enabled)
                .map(Arc::new)
                .collect::<Vec<_>>();
            let models = db
                .list_provider_models(pid)
                .await?
                .into_iter()
                .map(Arc::new)
                .collect::<Vec<_>>();
            snap.credentials_by_provider.insert(pid, creds);
            let compiled = models_index::compile(&models);
            if !compiled.exposed.is_empty() {
                snap.exposed_models_by_provider
                    .insert(pid, Arc::new(compiled.exposed));
            }
            if !compiled.variant_base.is_empty() {
                snap.variant_base_by_provider
                    .insert(pid, Arc::new(compiled.variant_base));
            }
            snap.models_by_provider.insert(pid, models);

            let routing = db.list_routing_rules(pid).await?;
            let routing_specs = routing
                .iter()
                .map(|r| RoutingRuleSpec {
                    id: r.id,
                    provider_id: r.provider_id,
                    operation: &r.operation,
                    kind: &r.kind,
                    implementation: &r.implementation,
                    dest_operation: r.dest_operation.as_deref(),
                    dest_kind: r.dest_kind.as_deref(),
                    sort_order: r.sort_order,
                    enabled: r.enabled,
                })
                .collect::<Vec<_>>();
            let compiled = crate::transform::routing::compile(&routing_specs);
            if !compiled.is_empty() {
                snap.routing_rules_by_provider
                    .insert(pid, Arc::new(compiled));
            }

            let mut attachments = db.list_provider_rule_sets(pid).await?;
            attachments.retain(|a| a.enabled);
            attachments.sort_by_key(|a| a.sort_order);
            let mut prov_rules: Vec<CompiledRule> = Vec::new();
            for a in &attachments {
                if let Some(rules) = compiled_sets.get(&a.rule_set_id) {
                    prov_rules.extend(rules.iter().cloned());
                }
            }
            crate::process::order_for_apply(&mut prov_rules);
            if !prov_rules.is_empty() {
                snap.rule_sets_by_provider.insert(pid, Arc::new(prov_rules));
            }

            let provider = Arc::new(provider);
            snap.providers_by_name
                .insert(provider.name.clone(), Arc::clone(&provider));
            snap.providers_by_id.insert(pid, provider);
        }

        // routes (enabled only — a disabled route must vanish from routing AND
        // from the model list) + members (sorted).
        for route in db.list_routes().await?.into_iter().filter(|r| r.enabled) {
            let mut members = db.list_route_members(route.id).await?;
            members.retain(|m| m.enabled);
            members.sort_by(|a, b| a.tier.cmp(&b.tier).then(b.weight.cmp(&a.weight)));
            let name = route.name.clone();
            snap.routes_by_name
                .insert(name, Arc::new(ResolvedRoute { route, members }));
        }

        // model aliases, grouped by global/provider scope and compiled once.
        let mut aliases_by_provider: HashMap<String, Vec<CompiledAlias>> = HashMap::new();
        for alias in db.list_aliases().await?.into_iter().filter(|a| a.enabled) {
            match CompiledAlias::try_from(alias) {
                Some(rule) => aliases_by_provider
                    .entry(rule.provider.clone())
                    .or_default()
                    .push(rule),
                None => tracing::warn!("alias regex failed to compile; skipped"),
            }
        }
        for rules in aliases_by_provider.values_mut() {
            rules.sort_by_key(|r| (r.sort_order, r.id));
        }
        snap.aliases_by_provider = aliases_by_provider
            .into_iter()
            .map(|(provider, rules)| (provider, Arc::new(rules)))
            .collect();

        // users (enabled) + their keys (enabled), indexed by digest;
        // collect ids for the authz scope universe below.
        let mut user_ids: Vec<i64> = Vec::new();
        for user in db.list_users().await?.into_iter().filter(|u| u.enabled) {
            user_ids.push(user.id);
            let keys = db.list_user_keys(user.id).await?;
            let user = Arc::new(user);
            for key in keys.into_iter().filter(|k| k.enabled) {
                let digest = key.api_key_digest.clone();
                let identity = Arc::new(KeyIdentity {
                    user_key: key,
                    user: User::clone(&user),
                });
                snap.keys_by_digest.insert(digest, identity);
            }
        }

        load_authz(db, &mut snap, &user_ids).await?;

        // Instance usage/log toggles — single row in practice; `.first()`
        // mirrors the tokenizer-download seeding in main.
        if let Some(s) = db.list_instance_settings().await?.first() {
            snap.log_settings = LogSettings {
                enable_usage: s.enable_usage,
                enable_upstream_log: s.enable_upstream_log,
                enable_upstream_log_body: s.enable_upstream_log_body,
                enable_downstream_log: s.enable_downstream_log,
                enable_downstream_log_body: s.enable_downstream_log_body,
                disable_log_redaction: s.disable_log_redaction,
            };
            snap.proxy = s.proxy.clone().filter(|p| !p.trim().is_empty());
            snap.update_channel = s.update_channel.clone().filter(|c| !c.trim().is_empty());
        }

        Ok(snap)
    }
}

/// Snapshot-compiled model alias rule. `regex` is anchored as a full match.
pub struct CompiledAlias {
    pub id: i64,
    pub provider: String,
    pub alias: String,
    pub target: String,
    pub sort_order: i64,
    regex: Regex,
}

impl CompiledAlias {
    fn try_from(alias: Alias) -> Option<Self> {
        if alias.target.trim().is_empty() {
            return None;
        }
        let pattern = format!("^(?:{})$", alias.alias);
        let regex = Regex::new(&pattern).ok()?;
        Some(Self {
            id: alias.id,
            provider: alias.provider,
            alias: alias.alias,
            target: alias.target,
            sort_order: alias.sort_order,
            regex,
        })
    }

    pub fn apply(&self, model: &str) -> Option<String> {
        self.regex
            .is_match(model)
            .then(|| self.regex.replace(model, self.target.as_str()).into_owned())
    }
}

/// Load orgs, teams, and the authz scope universe (permissions / rate limits /
/// quotas) into `snap`. Separated to keep `build` within size limits.
async fn load_authz(
    db: &dyn PersistenceBackend,
    snap: &mut ControlPlaneSnapshot,
    user_ids: &[i64],
) -> anyhow::Result<()> {
    let orgs = db.list_orgs().await?;
    let mut org_ids: Vec<i64> = Vec::with_capacity(orgs.len());
    let mut team_ids: Vec<i64> = Vec::new();

    for org in orgs {
        org_ids.push(org.id);
        let teams = db.list_teams(org.id).await?;
        for team in teams {
            team_ids.push(team.id);
            snap.teams_by_id.insert(team.id, Arc::new(team));
        }
        snap.orgs_by_id.insert(org.id, Arc::new(org));
    }

    // Build the full scope universe: orgs + teams + (enabled) users.
    let mut scopes: Vec<(Scope, i64)> =
        Vec::with_capacity(org_ids.len() + team_ids.len() + user_ids.len());
    scopes.extend(org_ids.iter().map(|&id| (Scope::Org, id)));
    scopes.extend(team_ids.iter().map(|&id| (Scope::Team, id)));
    scopes.extend(user_ids.iter().map(|&id| (Scope::User, id)));

    for (scope, id) in scopes {
        let perms = db.list_route_permissions(scope, id).await?;
        if !perms.is_empty() {
            let patterns: Vec<String> = perms.into_iter().map(|p| p.route_pattern).collect();
            snap.permissions_by_scope
                .insert((scope, id), Arc::new(patterns));
        }

        let limits = db.list_rate_limits(scope, id).await?;
        if !limits.is_empty() {
            snap.rate_limits_by_scope
                .insert((scope, id), Arc::new(limits));
        }

        if let Some(quota) = db.get_quota(scope, id).await? {
            snap.quotas_by_scope.insert((scope, id), Arc::new(quota));
        }
    }

    Ok(())
}
