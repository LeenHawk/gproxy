//! Terse record → `*Input` mappers for [`super::export_bundle`]: each copies
//! the editable fields and pins `id: Some(record.id)` so a re-import upserts
//! the same row (stable cross-references). Credentials and user keys are NOT
//! here — they decrypt secrets and live in `export.rs`.

use crate::store::persistence::records::{
    Alias, AliasInput, InstanceSettings, InstanceSettingsInput, Org, OrgInput, Provider,
    ProviderInput, ProviderModel, ProviderModelInput, ProviderRuleSet, ProviderRuleSetInput, Quota,
    QuotaInput, RateLimit, RateLimitInput, Route, RouteInput, RouteMember, RouteMemberInput,
    RoutePermission, RoutePermissionInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet,
    RuleSetInput, Team, TeamInput, User, UserInput,
};

pub(super) fn org_to_input(r: Org) -> OrgInput {
    OrgInput {
        id: Some(r.id),
        name: r.name,
        enabled: r.enabled,
        description: r.description,
    }
}

pub(super) fn team_to_input(r: Team) -> TeamInput {
    TeamInput {
        id: Some(r.id),
        org_id: r.org_id,
        name: r.name,
        enabled: r.enabled,
    }
}

pub(super) fn user_to_input(r: User) -> UserInput {
    UserInput {
        id: Some(r.id),
        name: r.name,
        org_id: r.org_id,
        team_id: r.team_id,
        password: r.password,
        enabled: r.enabled,
        is_admin: r.is_admin,
    }
}

pub(super) fn perm_to_input(r: RoutePermission) -> RoutePermissionInput {
    RoutePermissionInput {
        id: Some(r.id),
        scope: r.scope,
        scope_id: r.scope_id,
        route_pattern: r.route_pattern,
    }
}

pub(super) fn rate_limit_to_input(r: RateLimit) -> RateLimitInput {
    RateLimitInput {
        id: Some(r.id),
        scope: r.scope,
        scope_id: r.scope_id,
        route_pattern: r.route_pattern,
        rpm: r.rpm,
        rpd: r.rpd,
        total_tokens: r.total_tokens,
    }
}

pub(super) fn quota_to_input(r: Quota) -> QuotaInput {
    QuotaInput {
        id: Some(r.id),
        scope: r.scope,
        scope_id: r.scope_id,
        quota_total: r.quota_total,
        cost_used: r.cost_used,
    }
}

pub(super) fn provider_to_input(r: Provider) -> ProviderInput {
    ProviderInput {
        id: Some(r.id),
        name: r.name,
        channel: r.channel,
        label: r.label,
        settings_json: r.settings_json,
        credential_strategy: r.credential_strategy,
        proxy_url: r.proxy_url,
        tls_fingerprint: r.tls_fingerprint,
        enabled: r.enabled,
    }
}

pub(super) fn provider_model_to_input(r: ProviderModel) -> ProviderModelInput {
    ProviderModelInput {
        id: Some(r.id),
        provider_id: r.provider_id,
        model_id: r.model_id,
        display_name: r.display_name,
        pricing_json: r.pricing_json,
        variants_json: r.variants_json,
        enabled: r.enabled,
    }
}

pub(super) fn routing_rule_to_input(r: RoutingRule) -> RoutingRuleInput {
    RoutingRuleInput {
        id: Some(r.id),
        provider_id: r.provider_id,
        operation: r.operation,
        kind: r.kind,
        implementation: r.implementation,
        dest_operation: r.dest_operation,
        dest_kind: r.dest_kind,
        sort_order: r.sort_order,
        enabled: r.enabled,
    }
}

pub(super) fn provider_rule_set_to_input(r: ProviderRuleSet) -> ProviderRuleSetInput {
    ProviderRuleSetInput {
        id: Some(r.id),
        provider_id: r.provider_id,
        rule_set_id: r.rule_set_id,
        sort_order: r.sort_order,
        enabled: r.enabled,
    }
}

pub(super) fn rule_set_to_input(r: RuleSet) -> RuleSetInput {
    RuleSetInput {
        id: Some(r.id),
        name: r.name,
        enabled: r.enabled,
        description: r.description,
    }
}

pub(super) fn rule_to_input(r: Rule) -> RuleInput {
    RuleInput {
        id: Some(r.id),
        rule_set_id: r.rule_set_id,
        kind: r.kind,
        config_json: r.config_json,
        filter_model_pattern: r.filter_model_pattern,
        filter_operation_keys: r.filter_operation_keys,
        sort_order: r.sort_order,
        enabled: r.enabled,
    }
}

pub(super) fn route_to_input(r: Route) -> RouteInput {
    RouteInput {
        id: Some(r.id),
        name: r.name,
        strategy: r.strategy,
        enabled: r.enabled,
        description: r.description,
        settings_json: r.settings_json,
    }
}

pub(super) fn route_member_to_input(r: RouteMember) -> RouteMemberInput {
    RouteMemberInput {
        id: Some(r.id),
        route_id: r.route_id,
        provider_id: r.provider_id,
        upstream_model_id: r.upstream_model_id,
        weight: r.weight,
        tier: r.tier,
        enabled: r.enabled,
    }
}

pub(super) fn alias_to_input(r: Alias) -> AliasInput {
    AliasInput {
        id: Some(r.id),
        alias: r.alias,
        route_id: r.route_id,
    }
}

pub(super) fn settings_to_input(r: InstanceSettings) -> InstanceSettingsInput {
    InstanceSettingsInput {
        id: Some(r.id),
        instance_name: r.instance_name,
        proxy: r.proxy,
        spoof_emulation: r.spoof_emulation,
        enable_usage: r.enable_usage,
        enable_upstream_log: r.enable_upstream_log,
        enable_upstream_log_body: r.enable_upstream_log_body,
        enable_downstream_log: r.enable_downstream_log,
        enable_downstream_log_body: r.enable_downstream_log_body,
        disable_log_redaction: r.disable_log_redaction,
        enable_tokenizer_download: r.enable_tokenizer_download,
        update_channel: r.update_channel,
        retention_days: r.retention_days,
    }
}
