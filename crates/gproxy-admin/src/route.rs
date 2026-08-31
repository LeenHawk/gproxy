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
    ConnectivityTest,
    ModelTest,
    ModelDiscover,
    CredentialQuotaProbe(i64),
    RevealUserKey(i64),
    Usage,
    QuotaWindows,
    CredentialCycles,
    Channels,
    TlsPresets,
    RulePresets,
    ApplyRulePreset { provider_id: i64, preset: String },
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
    TokenizerVocabDelete,
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
        if let ["credentials", credential, "quota-probe"] = segments.as_slice() {
            return Some(Route::CredentialQuotaProbe(credential.parse().ok()?));
        }
        if let ["providers", provider, "rule-presets", preset] = segments.as_slice() {
            return Some(Route::ApplyRulePreset {
                provider_id: provider.parse().ok()?,
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
        return Some(Route::LogDetail(segments[1].to_owned()));
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
        (&Method::GET, "portal-settings") => Some(Route::PortalSettingsRead),
        (&Method::PATCH, "portal-settings") => Some(Route::PortalSettingsWrite),
        (&Method::POST, "export") => Some(Route::ConfigurationExport),
        (&Method::POST, "import") => Some(Route::ConfigurationImport),
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
        Route::ConfigurationImport => action("configuration.import", "configuration", None),
        Route::ApplyRulePreset { provider_id, .. } => {
            action("rule_preset.apply", "providers", Some(*provider_id))
        }
        Route::ResetRoutingDefaults(provider_id) => {
            action("routing_defaults.reset", "providers", Some(*provider_id))
        }
        Route::LogSettingsWrite => action("log_settings.update", "settings", None),
        Route::InstanceSettingsWrite => action("instance_settings.update", "settings", None),
        Route::TokenizerVocabFetch => action("tokenizer_vocab.fetch", "tokenizer_vocabs", None),
        Route::TokenizerVocabDelete => action("tokenizer_vocab.delete", "tokenizer_vocabs", None),
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
        Route::ModelDiscover => action("model.discover", "providers", None),
        Route::List(_)
        | Route::ConfigurationExport
        | Route::ConnectivityTest
        | Route::RevealUserKey(_)
        | Route::Usage
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
