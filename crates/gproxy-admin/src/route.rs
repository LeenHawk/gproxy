use http::Method;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename_all = "kebab-case")]
pub(crate) enum Entity {
    Organizations,
    Teams,
    Providers,
    Credentials,
    Routes,
    RouteMembers,
    Aliases,
    ModelAliases,
    ProviderModels,
    Users,
    UserKeys,
    Permissions,
    RateLimits,
    Quotas,
    PriceRules,
    PriceRates,
    RoutingRules,
    RuleSets,
    Rules,
    ProviderRuleSets,
}

#[derive(Debug, Clone)]
pub(crate) enum Route {
    List(Entity),
    Create(Entity),
    Update(Entity, i64),
    Delete(Entity, i64),
    Batch(Entity),
    ConfigurationExport,
    ConfigurationImport,
    DefaultModelCatalog,
    PriceCatalog,
    ApplyDefaultModelPrices,
    ConnectivityTest,
    ModelTest,
    ModelDiscover,
    CredentialQuotaProbe(i64),
    CredentialQuotaReset(i64),
    CredentialHealthReset(i64),
    RevealCredentialSecret(i64),
    RevealUserKey(i64),
    UserPassword(i64),
    Usage,
    UsageTrend,
    QuotaWindows,
    CredentialCycles,
    Channels,
    TlsPresets,
    RulePresets,
    ApplyRulePreset { rule_set_id: i64, preset: String },
    ResetRoutingDefaults(i64),
    Audit,
    Logs,
    LogDetail(String),
    LogSettingsRead,
    LogSettingsWrite,
    InstanceSettingsRead,
    InstanceSettingsWrite,
    TokenizerVocabsRead,
    TokenizerVocabFetch,
    TokenizerVocabProgress,
    TokenizerVocabDelete,
    TokenizerAuthRead,
    TokenizerAuthWrite,
    TokenizerAuthReveal,
    PortalSettingsRead,
    PortalSettingsWrite,
    LoginAuthCodeStart,
    LoginAuthCodeComplete,
    LoginDeviceStart,
    LoginDevicePoll,
    LoginCookieExchange,
}

pub(crate) fn parse(method: &Method, path: &str) -> Option<Route> {
    let segments = path
        .strip_prefix("/admin/api/")?
        .split('/')
        .collect::<Vec<_>>();
    if method == Method::POST {
        let login = match segments.as_slice() {
            ["login", "authcode", "start"] => Some(Route::LoginAuthCodeStart),
            ["login", "authcode", "complete"] => Some(Route::LoginAuthCodeComplete),
            ["login", "device", "start"] => Some(Route::LoginDeviceStart),
            ["login", "device", "poll"] => Some(Route::LoginDevicePoll),
            ["login", "cookie"] => Some(Route::LoginCookieExchange),
            _ => None,
        };
        if login.is_some() {
            return login;
        }
        if segments.as_slice() == ["connectivity", "test"] {
            return Some(Route::ConnectivityTest);
        }
        if segments.as_slice() == ["models", "test"] {
            return Some(Route::ModelTest);
        }
        if segments.as_slice() == ["models", "discover"] {
            return Some(Route::ModelDiscover);
        }
        if segments.as_slice() == ["default-model-catalog", "apply-prices"] {
            return Some(Route::ApplyDefaultModelPrices);
        }
        if segments.as_slice() == ["tokenizer-auth", "reveal"] {
            return Some(Route::TokenizerAuthReveal);
        }
        if let ["credentials", credential, "quota-probe"] = segments.as_slice() {
            return Some(Route::CredentialQuotaProbe(credential.parse().ok()?));
        }
        if let ["credentials", credential, "quota-reset"] = segments.as_slice() {
            return Some(Route::CredentialQuotaReset(credential.parse().ok()?));
        }
        if let ["credentials", credential, "health-reset"] = segments.as_slice() {
            return Some(Route::CredentialHealthReset(credential.parse().ok()?));
        }
        if let ["credentials", credential, "reveal"] = segments.as_slice() {
            return Some(Route::RevealCredentialSecret(credential.parse().ok()?));
        }
        if let ["users", user, "password"] = segments.as_slice() {
            return Some(Route::UserPassword(user.parse().ok()?));
        }
        if let ["rule-sets", rule_set, "rule-presets", preset] = segments.as_slice() {
            return Some(Route::ApplyRulePreset {
                rule_set_id: rule_set.parse().ok()?,
                preset: (*preset).to_owned(),
            });
        }
        if let ["providers", provider, "routing-defaults", "reset"] = segments.as_slice() {
            return Some(Route::ResetRoutingDefaults(provider.parse().ok()?));
        }
    }
    if segments.len() == 3
        && segments[0] == "user-keys"
        && segments[2] == "reveal"
        && method == Method::POST
    {
        return Some(Route::RevealUserKey(segments[1].parse().ok()?));
    }
    if segments.len() == 1 {
        return special(method, segments[0]).or_else(|| {
            let entity = entity(segments[0])?;
            match *method {
                Method::GET => Some(Route::List(entity)),
                Method::POST => Some(Route::Create(entity)),
                _ => None,
            }
        });
    }
    if segments.len() == 2 && segments[0] == "logs" && method == Method::GET {
        let request_id = percent_encoding::percent_decode_str(segments[1])
            .decode_utf8()
            .ok()?
            .into_owned();
        return Some(Route::LogDetail(request_id));
    }
    if segments.as_slice() == ["tokenizer-vocabs", "progress"] && method == Method::GET {
        return Some(Route::TokenizerVocabProgress);
    }
    if segments.len() == 2 && segments[0] == "batch" && method == Method::POST {
        return Some(Route::Batch(entity(segments[1])?));
    }
    if segments.len() == 2 && method == Method::PATCH {
        return Some(Route::Update(
            entity(segments[0])?,
            segments[1].parse().ok()?,
        ));
    }
    if segments.len() == 2 && method == Method::DELETE {
        return Some(Route::Delete(
            entity(segments[0])?,
            segments[1].parse().ok()?,
        ));
    }
    None
}

fn special(method: &Method, name: &str) -> Option<Route> {
    match (method, name) {
        (&Method::GET, "usage") => Some(Route::Usage),
        (&Method::GET, "usage-trend") => Some(Route::UsageTrend),
        (&Method::GET, "quota-windows") => Some(Route::QuotaWindows),
        (&Method::GET, "credential-cycles") => Some(Route::CredentialCycles),
        (&Method::GET, "channels") => Some(Route::Channels),
        (&Method::GET, "tls-presets") => Some(Route::TlsPresets),
        (&Method::GET, "rule-presets") => Some(Route::RulePresets),
        (&Method::GET, "audit") => Some(Route::Audit),
        (&Method::GET, "logs") => Some(Route::Logs),
        (&Method::GET, "log-settings") => Some(Route::LogSettingsRead),
        (&Method::PATCH, "log-settings") => Some(Route::LogSettingsWrite),
        (&Method::GET, "instance-settings") => Some(Route::InstanceSettingsRead),
        (&Method::PATCH, "instance-settings") => Some(Route::InstanceSettingsWrite),
        (&Method::GET, "tokenizer-vocabs") => Some(Route::TokenizerVocabsRead),
        (&Method::POST, "tokenizer-vocabs") => Some(Route::TokenizerVocabFetch),
        (&Method::DELETE, "tokenizer-vocabs") => Some(Route::TokenizerVocabDelete),
        (&Method::GET, "tokenizer-auth") => Some(Route::TokenizerAuthRead),
        (&Method::PATCH, "tokenizer-auth") => Some(Route::TokenizerAuthWrite),
        (&Method::GET, "portal-settings") => Some(Route::PortalSettingsRead),
        (&Method::PATCH, "portal-settings") => Some(Route::PortalSettingsWrite),
        (&Method::POST, "export") => Some(Route::ConfigurationExport),
        (&Method::POST, "import") => Some(Route::ConfigurationImport),
        (&Method::GET, "default-model-catalog") => Some(Route::DefaultModelCatalog),
        (&Method::GET, "price-catalog") => Some(Route::PriceCatalog),
        _ => None,
    }
}

fn entity(name: &str) -> Option<Entity> {
    Some(match name {
        "organizations" => Entity::Organizations,
        "teams" => Entity::Teams,
        "providers" => Entity::Providers,
        "credentials" => Entity::Credentials,
        "routes" => Entity::Routes,
        "route-members" => Entity::RouteMembers,
        "aliases" => Entity::Aliases,
        "model-aliases" => Entity::ModelAliases,
        "provider-models" => Entity::ProviderModels,
        "users" => Entity::Users,
        "user-keys" => Entity::UserKeys,
        "permissions" => Entity::Permissions,
        "rate-limits" => Entity::RateLimits,
        "quotas" => Entity::Quotas,
        "price-rules" => Entity::PriceRules,
        "price-rates" => Entity::PriceRates,
        "routing-rules" => Entity::RoutingRules,
        "rule-sets" => Entity::RuleSets,
        "rules" => Entity::Rules,
        "provider-rule-sets" => Entity::ProviderRuleSets,
        _ => return None,
    })
}

pub(crate) struct AuditDescriptor {
    pub action: String,
    pub target_kind: String,
    pub target_id: Option<i64>,
}

pub(crate) fn audit(route: &Route, body: &[u8]) -> Option<AuditDescriptor> {
    let mutation = |entity: Entity, verb: &str, id| AuditDescriptor {
        action: format!("{}.{}", entity.id(), verb),
        target_kind: entity.id().into(),
        target_id: id,
    };
    Some(match route {
        Route::Create(entity) => mutation(*entity, "create", None),
        Route::Update(entity, id) => mutation(*entity, "update", Some(*id)),
        Route::Delete(entity, id) => mutation(*entity, "delete", Some(*id)),
        Route::Batch(entity) => {
            let verb = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|value| value.get("action")?.as_str().map(str::to_owned))
                .unwrap_or_else(|| "batch".into());
            mutation(*entity, &verb, None)
        }
        Route::UserPassword(id) => action("users.password", "users", Some(*id)),
        Route::ConfigurationImport => action("configuration.import", "configuration", None),
        Route::ApplyDefaultModelPrices => provider_action("default_prices.apply", body),
        Route::ApplyRulePreset { rule_set_id, .. } => {
            action("rule_preset.apply", "rule_sets", Some(*rule_set_id))
        }
        Route::ResetRoutingDefaults(provider_id) => {
            action("routing_defaults.reset", "providers", Some(*provider_id))
        }
        Route::LogSettingsWrite => action("log_settings.update", "settings", None),
        Route::InstanceSettingsWrite => action("instance_settings.update", "settings", None),
        Route::TokenizerVocabFetch => action("tokenizer_vocab.fetch", "tokenizer_vocabs", None),
        Route::TokenizerVocabDelete => action("tokenizer_vocab.delete", "tokenizer_vocabs", None),
        Route::TokenizerAuthWrite => action("tokenizer_auth.update", "tokenizer_auth", None),
        Route::TokenizerAuthReveal => action("tokenizer_auth.reveal", "tokenizer_auth", None),
        Route::PortalSettingsWrite => action("portal_settings.update", "settings", None),
        Route::LoginAuthCodeStart => provider_action("channel_login.authcode_start", body),
        Route::LoginAuthCodeComplete => provider_action("channel_login.authcode_complete", body),
        Route::LoginDeviceStart => provider_action("channel_login.device_start", body),
        Route::LoginDevicePoll => action("channel_login.device_poll", "credentials", None),
        Route::LoginCookieExchange => provider_action("channel_login.cookie", body),
        Route::ModelTest => action("model.test", "providers", None),
        Route::CredentialQuotaProbe(id) => {
            action("credential.quota_probe", "credentials", Some(*id))
        }
        Route::CredentialQuotaReset(id) => {
            action("credential.quota_reset", "credentials", Some(*id))
        }
        Route::CredentialHealthReset(id) => {
            action("credential.health_reset", "credentials", Some(*id))
        }
        Route::RevealUserKey(id) => action("user_key.reveal", "user_key", Some(*id)),
        Route::RevealCredentialSecret(id) => {
            action("credential.secret_reveal", "credentials", Some(*id))
        }
        Route::ModelDiscover => action("model.discover", "providers", None),
        Route::List(_)
        | Route::ConfigurationExport
        | Route::DefaultModelCatalog
        | Route::PriceCatalog
        | Route::ConnectivityTest
        | Route::Usage
        | Route::UsageTrend
        | Route::QuotaWindows
        | Route::CredentialCycles
        | Route::Channels
        | Route::TlsPresets
        | Route::RulePresets
        | Route::Audit
        | Route::Logs
        | Route::LogDetail(_)
        | Route::LogSettingsRead
        | Route::InstanceSettingsRead
        | Route::TokenizerVocabsRead
        | Route::TokenizerVocabProgress
        | Route::TokenizerAuthRead
        | Route::PortalSettingsRead => return None,
    })
}

fn action(action: &str, target_kind: &str, target_id: Option<i64>) -> AuditDescriptor {
    AuditDescriptor {
        action: action.into(),
        target_kind: target_kind.into(),
        target_id,
    }
}

fn provider_action(action_name: &str, body: &[u8]) -> AuditDescriptor {
    let provider_id = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("provider_id")?.as_i64());
    action(action_name, "providers", provider_id)
}

impl Entity {
    fn id(self) -> &'static str {
        match self {
            Self::Organizations => "organizations",
            Self::Teams => "teams",
            Self::Providers => "providers",
            Self::Credentials => "credentials",
            Self::Routes => "routes",
            Self::RouteMembers => "route_members",
            Self::Aliases => "aliases",
            Self::ModelAliases => "model_aliases",
            Self::ProviderModels => "provider_models",
            Self::Users => "users",
            Self::UserKeys => "user_keys",
            Self::Permissions => "permissions",
            Self::RateLimits => "rate_limits",
            Self::Quotas => "quotas",
            Self::PriceRules => "price_rules",
            Self::PriceRates => "price_rates",
            Self::RoutingRules => "routing_rules",
            Self::RuleSets => "rule_sets",
            Self::Rules => "rules",
            Self::ProviderRuleSets => "provider_rule_sets",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Route, audit, parse};

    #[test]
    fn parses_and_audits_credential_quota_reset() {
        let route = parse(&http::Method::POST, "/admin/api/credentials/17/quota-reset").unwrap();
        assert!(matches!(route, Route::CredentialQuotaReset(17)));
        let descriptor = audit(&route, b"{}").unwrap();
        assert_eq!(descriptor.action, "credential.quota_reset");
        assert_eq!(descriptor.target_kind, "credentials");
        assert_eq!(descriptor.target_id, Some(17));
    }

    #[test]
    fn log_detail_decodes_the_request_id_path_segment() {
        let route = parse(
            &http::Method::GET,
            "/admin/api/logs/model-test%3A1%3Agpt-5.6-luna",
        )
        .unwrap();
        let Route::LogDetail(request_id) = route else {
            panic!("expected log detail route");
        };
        assert_eq!(request_id, "model-test:1:gpt-5.6-luna");
    }

    #[test]
    fn secret_reveal_is_a_central_audit_action() {
        let route = parse(&http::Method::POST, "/admin/api/credentials/17/reveal").unwrap();
        let descriptor = audit(&route, b"{}").unwrap();
        assert_eq!(descriptor.action, "credential.secret_reveal");
        assert_eq!(descriptor.target_id, Some(17));

        let route = parse(&http::Method::POST, "/admin/api/tokenizer-auth/reveal").unwrap();
        let descriptor = audit(&route, b"{}").unwrap();
        assert_eq!(descriptor.action, "tokenizer_auth.reveal");
        assert_eq!(descriptor.target_id, None);
    }
}
