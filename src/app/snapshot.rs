//! The control-plane snapshot (§7.2): the sole `ArcSwap` snapshot read on the
//! hot path. Fully rebuildable from persistence (boot + invalidation). Holds no
//! counters/sessions/health — those are redis-direct or separate local state.
//!
//! M2/M3 extend THIS struct + [`ControlPlaneSnapshot::build`], never a parallel
//! snapshot.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use regex::Regex;

use crate::app::models_index::{self, ExposedModel};
use crate::process::CompiledRule;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{
    Alias, Credential, Org, PriceRule, Provider, ProviderModel, Quota, RateLimit, Route,
    RouteMember, RoutePermission, Scope, Team, User, UserKey,
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
    /// Enabled pricing rules, sorted by resolver rank at lookup time.
    pub price_rules: Arc<Vec<PriceRule>>,
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
    /// Whether to apply channel built-in TLS/HTTP2 impersonation when no
    /// provider/credential TLS fingerprint is configured. Defaults off.
    pub spoof_emulation: bool,
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
            price_rules: Arc::new(Vec::new()),
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
            spoof_emulation: false,
            update_channel: None,
            version,
        }
    }

    /// Full reload from persistence (boot + invalidation). Every table is
    /// fetched whole in ONE parallel read round and grouped in memory — the
    /// former per-parent query pattern cost O(providers×4 + routes + users +
    /// scopes×3) SERIAL round trips, which dominated edge cold starts (each
    /// query is an independent HTTP call there). On wasm the backend trait is
    /// `?Send`, so this future is non-Send — await it directly, never on a
    /// `Send`-requiring spawn.
    pub async fn build(db: &dyn PersistenceBackend, version: u64) -> anyhow::Result<Self> {
        let mut snap = Self::empty(version);

        let (
            providers,
            credentials,
            provider_models,
            routing_rules,
            provider_rule_sets,
            rule_sets,
            rules,
            price_rules,
            routes,
            route_members,
            aliases,
            users,
            user_keys,
            orgs,
            teams,
            permissions,
            rate_limits,
            quotas,
            instance_settings,
        ) = futures_util::try_join!(
            db.list_providers(),
            db.list_all_credentials(),
            db.list_all_provider_models(),
            db.list_all_routing_rules(),
            db.list_all_provider_rule_sets(),
            db.list_rule_sets(),
            db.list_all_rules(),
            db.list_price_rules(),
            db.list_routes(),
            db.list_all_route_members(),
            db.list_aliases(),
            db.list_users(),
            db.list_all_user_keys(),
            db.list_orgs(),
            db.list_all_teams(),
            db.list_all_route_permissions(),
            db.list_all_rate_limits(),
            db.list_all_quotas(),
            db.list_instance_settings(),
        )?;

        // rule sets compile once; providers attach by id below
        let mut rules_by_set = group_by(rules, |r| r.rule_set_id);
        let mut compiled_sets: HashMap<i64, Vec<CompiledRule>> = HashMap::new();
        for set in rule_sets.into_iter().filter(|s| s.enabled) {
            let rules = rules_by_set.remove(&set.id).unwrap_or_default();
            compiled_sets.insert(set.id, crate::process::compile_rules(&rules));
        }

        // providers + their credentials/models
        let mut creds_by_provider = group_by(credentials.into_iter().filter(|c| c.enabled), |c| {
            c.provider_id
        });
        let mut models_by_provider = group_by(provider_models, |m| m.provider_id);
        let mut routing_by_provider = group_by(routing_rules, |r| r.provider_id);
        let mut attachments_by_provider =
            group_by(provider_rule_sets.into_iter().filter(|a| a.enabled), |a| {
                a.provider_id
            });
        for provider in providers {
            let pid = provider.id;
            let creds = creds_by_provider
                .remove(&pid)
                .unwrap_or_default()
                .into_iter()
                .map(Arc::new)
                .collect::<Vec<_>>();
            let models = models_by_provider
                .remove(&pid)
                .unwrap_or_default()
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

            let routing = routing_by_provider.remove(&pid).unwrap_or_default();
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

            let mut attachments = attachments_by_provider.remove(&pid).unwrap_or_default();
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

        snap.price_rules = Arc::new(price_rules.into_iter().filter(|r| r.enabled).collect());

        // routes (enabled only — a disabled route must vanish from routing AND
        // from the model list) + members (sorted).
        let mut members_by_route = group_by(route_members.into_iter().filter(|m| m.enabled), |m| {
            m.route_id
        });
        for route in routes.into_iter().filter(|r| r.enabled) {
            let mut members = members_by_route.remove(&route.id).unwrap_or_default();
            members.sort_by(|a, b| a.tier.cmp(&b.tier).then(b.weight.cmp(&a.weight)));
            let name = route.name.clone();
            snap.routes_by_name
                .insert(name, Arc::new(ResolvedRoute { route, members }));
        }

        // model aliases, grouped by global/provider scope and compiled once.
        let mut aliases_by_provider: HashMap<String, Vec<CompiledAlias>> = HashMap::new();
        for alias in aliases.into_iter().filter(|a| a.enabled) {
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
        let mut keys_by_user = group_by(user_keys.into_iter().filter(|k| k.enabled), |k| k.user_id);
        let mut user_ids: Vec<i64> = Vec::new();
        for user in users.into_iter().filter(|u| u.enabled) {
            user_ids.push(user.id);
            let keys = keys_by_user.remove(&user.id).unwrap_or_default();
            let user = Arc::new(user);
            for key in keys {
                let digest = key.api_key_digest.clone();
                let identity = Arc::new(KeyIdentity {
                    user_key: key,
                    user: User::clone(&user),
                });
                snap.keys_by_digest.insert(digest, identity);
            }
        }

        load_authz(
            &mut snap,
            orgs,
            teams,
            &user_ids,
            permissions,
            rate_limits,
            quotas,
        );

        // Instance usage/log toggles — single row in practice; `.first()`
        // mirrors the tokenizer-download seeding in main.
        if let Some(s) = instance_settings.first() {
            snap.log_settings = LogSettings {
                enable_usage: s.enable_usage,
                enable_upstream_log: s.enable_upstream_log,
                enable_upstream_log_body: s.enable_upstream_log_body,
                enable_downstream_log: s.enable_downstream_log,
                enable_downstream_log_body: s.enable_downstream_log_body,
                disable_log_redaction: s.disable_log_redaction,
            };
            snap.proxy = s.proxy.clone().filter(|p| !p.trim().is_empty());
            snap.spoof_emulation = s.spoof_emulation.unwrap_or(false);
            snap.update_channel = s.update_channel.clone().filter(|c| !c.trim().is_empty());
        }

        Ok(snap)
    }
}

/// Group whole-table child rows by their parent id (insertion order kept
/// within each group — the backends return primary-key order).
fn group_by<T>(
    items: impl IntoIterator<Item = T>,
    key: impl Fn(&T) -> i64,
) -> HashMap<i64, Vec<T>> {
    let mut map: HashMap<i64, Vec<T>> = HashMap::new();
    for item in items {
        map.entry(key(&item)).or_default().push(item);
    }
    map
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

/// Index orgs, teams, and the authz scope universe (permissions / rate limits
/// / quotas) into `snap` from the prefetched whole-table rows. Rows outside
/// the scope universe (orgs + teams + ENABLED users) are dropped, matching
/// the former per-scope query behaviour. Separated to keep `build` within
/// size limits.
fn load_authz(
    snap: &mut ControlPlaneSnapshot,
    orgs: Vec<Org>,
    teams: Vec<Team>,
    user_ids: &[i64],
    permissions: Vec<RoutePermission>,
    rate_limits: Vec<RateLimit>,
    quotas: Vec<Quota>,
) {
    let mut universe: HashSet<(Scope, i64)> = HashSet::new();
    for org in orgs {
        universe.insert((Scope::Org, org.id));
        snap.orgs_by_id.insert(org.id, Arc::new(org));
    }
    for team in teams {
        universe.insert((Scope::Team, team.id));
        snap.teams_by_id.insert(team.id, Arc::new(team));
    }
    universe.extend(user_ids.iter().map(|&id| (Scope::User, id)));

    let mut patterns: HashMap<(Scope, i64), Vec<String>> = HashMap::new();
    for p in permissions {
        if universe.contains(&(p.scope, p.scope_id)) {
            patterns
                .entry((p.scope, p.scope_id))
                .or_default()
                .push(p.route_pattern);
        }
    }
    snap.permissions_by_scope = patterns
        .into_iter()
        .map(|(k, v)| (k, Arc::new(v)))
        .collect();

    let mut limits: HashMap<(Scope, i64), Vec<RateLimit>> = HashMap::new();
    for l in rate_limits {
        if universe.contains(&(l.scope, l.scope_id)) {
            limits.entry((l.scope, l.scope_id)).or_default().push(l);
        }
    }
    snap.rate_limits_by_scope = limits.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();

    for q in quotas {
        if universe.contains(&(q.scope, q.scope_id)) {
            snap.quotas_by_scope
                .insert((q.scope, q.scope_id), Arc::new(q));
        }
    }
}
