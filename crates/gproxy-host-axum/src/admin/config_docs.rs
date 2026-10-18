use super::*;
use crate::documentation::Api;
use gproxy_sdk::dto as d;
use serde_json::{Value, json};

pub(crate) fn document(api: &mut Api) {
    macro_rules! family {
        ($path:literal,$write:ty,$patch:ty,$read:ty) => {{
            api.query::<d::ListQuery, d::Page<$read>>(concat!("/admin/api/", $path), $path);
            api.json::<$write, $read>("post", concat!("/admin/api/", $path), $path);
            api.json::<Vec<d::BatchItem<$write, $patch>>, Vec<Option<$read>>>(
                "post",
                concat!("/admin/api/", $path, "/batch"),
                $path,
            );
            api.get::<$read>(concat!("/admin/api/", $path, "/{id}"), $path);
            api.json::<$patch, $read>("patch", concat!("/admin/api/", $path, "/{id}"), $path);
            api.empty("delete", concat!("/admin/api/", $path, "/{id}"), $path);
        }};
    }
    family!(
        "providers",
        d::ProviderWrite,
        d::ProviderPatch,
        d::ProviderDto
    );
    family!(
        "credentials",
        d::CredentialWrite,
        d::CredentialPatch,
        d::CredentialDto
    );
    family!("models", d::ModelWrite, d::ModelPatch, d::ModelDto);
    family!(
        "provider-models",
        d::ProviderModelWrite,
        d::ProviderModelPatch,
        d::ProviderModelDto
    );
    family!("routes", d::RouteWrite, d::RoutePatch, d::RouteDto);
    family!(
        "route-members",
        d::RouteMemberWrite,
        d::RouteMemberPatch,
        d::RouteMemberDto
    );
    family!(
        "connection-profiles",
        d::ConnectionProfileWrite,
        d::ConnectionProfilePatch,
        d::ConnectionProfileDto
    );
    family!("rule-sets", d::RuleSetWrite, d::RuleSetPatch, d::RuleSetDto);
    family!(
        "rules",
        d::RewriteRuleWrite,
        d::RewriteRulePatch,
        d::RewriteRuleDto
    );
    family!(
        "provider-rule-sets",
        d::ProviderRuleSetWrite,
        d::ProviderRuleSetPatch,
        d::ProviderRuleSetDto
    );
    family!(
        "operation-rules",
        d::OperationRuleWrite,
        d::OperationRulePatch,
        d::OperationRuleDto
    );
    family!(
        "operation-endpoints",
        d::OperationEndpointWrite,
        d::OperationEndpointPatch,
        d::OperationEndpointDto
    );
    family!("quotas", d::QuotaWrite, d::QuotaPatch, d::QuotaDto);
    family!(
        "price-rules",
        d::PriceRuleWrite,
        d::PriceRulePatch,
        d::PriceRuleDto
    );
    family!(
        "price-rates",
        d::PriceRateWrite,
        d::PriceRatePatch,
        d::PriceRateDto
    );
    family!(
        "price-tiers",
        d::PriceTierWrite,
        d::PriceTierPatch,
        d::PriceTierDto
    );

    api.route("post","/admin/api/providers/{id}/routing-defaults/reset","providers",None,vec![],Some(json!({"type":"object","required":["cleared"],"properties":{"cleared":{"type":"integer"}}})));
    api.action::<Value>("/admin/api/credentials/{id}/reveal", "credentials");
    api.json::<StatusBody, d::CredentialDto>(
        "post",
        "/admin/api/credentials/{id}/status",
        "credentials",
    );
    let query = api.schemas.query::<ForceQuery>();
    let response = api.schemas.output::<d::CredentialSummaryDto>();
    api.route(
        "post",
        "/admin/api/credentials/{id}/refresh",
        "credentials",
        None,
        query,
        Some(response),
    );
    api.get::<d::CredentialQuotaDto>("/admin/api/credentials/{id}/quota", "credentials");
    api.query::<d::QuotaObservationQuery, d::Page<d::QuotaObservationDto>>(
        "/admin/api/credentials/{id}/quota-observations",
        "credentials",
    );
    api.action::<d::QuotaSnapshotDto>("/admin/api/credentials/{id}/quota-probe", "credentials");
    api.action::<d::QuotaProbeDto>(
        "/admin/api/credentials/{id}/quota-diagnostics",
        "credentials",
    );
    api.get::<d::QuotaResetCreditsDto>(
        "/admin/api/credentials/{id}/quota-reset-credits",
        "credentials",
    );
    api.json::<d::QuotaResetWrite, d::QuotaResetDto>(
        "post",
        "/admin/api/credentials/{id}/quota-reset",
        "credentials",
    )["requestBody"]["required"] = json!(false);
    api.action::<d::CredentialDto>("/admin/api/credentials/{id}/health-reset", "credentials");
    api.get::<Vec<d::CredentialLimitStatusDto>>(
        "/admin/api/credentials/{id}/limits",
        "credentials",
    );

    api.json::<DiscoverBody, Vec<d::DiscoveredModelDto>>(
        "post",
        "/admin/api/models/discover",
        "models",
    );
    api.json::<ApplyDiscoveredBody, Vec<String>>(
        "post",
        "/admin/api/models/discover/apply",
        "models",
    );
    api.json::<d::ModelTest, d::ModelTestResultDto>("post", "/admin/api/models/test", "models");
    api.get::<d::SettingsDto>("/admin/api/settings", "settings");
    api.json::<d::SettingsPatch, d::SettingsDto>("patch", "/admin/api/settings", "settings");
    api.action::<d::RuleSetDto>("/admin/api/providers/{id}/default-rule-set", "rule-sets")["x-gproxy-sections"] =
        json!(["providers", "provider-rule-sets", "rule-sets"]);
    api.json::<Vec<d::RewriteRuleWrite>, Vec<d::RewriteRuleDto>>(
        "put",
        "/admin/api/rule-sets/{id}/rules",
        "rule-sets",
    );
    api.action::<Vec<d::RewriteRuleDto>>(
        "/admin/api/rule-sets/{id}/rule-presets/{preset}",
        "rule-sets",
    );
    api.get::<Vec<d::OperationRoutingDto>>("/admin/api/providers/{id}/routing", "operation-rules");
    api.json::<d::RoutingMappingWrite, Vec<d::OperationRoutingDto>>(
        "put",
        "/admin/api/providers/{id}/routing/{operation}/{dialect}",
        "operation-rules",
    );
    let response = api.schemas.output::<Vec<d::OperationRoutingDto>>();
    api.route(
        "delete",
        "/admin/api/providers/{id}/routing/{operation}/{dialect}",
        "operation-rules",
        None,
        vec![],
        Some(response),
    );

    api.query::<OwnersQuery, Vec<d::BudgetStatusDto>>("/admin/api/quotas/status", "quotas");
    api.action::<d::BudgetStatusDto>("/admin/api/quotas/{id}/reset", "quotas");
    api.action::<Vec<String>>("/admin/api/quotas/{id}/limit-reset", "quotas");
    api.json::<d::ExportRequest, d::ConfigurationExportDto>(
        "post",
        "/admin/api/export",
        "transfer",
    );
    api.json::<d::ImportRequest, d::ImportReportDto>("post", "/admin/api/import", "transfer");
    api.json::<d::ConnectivityTest, d::ConnectivityResultDto>(
        "post",
        "/admin/api/connectivity/test",
        "connectivity",
    );
    api.get::<Vec<gproxy_channel::ChannelDescriptor>>("/admin/api/channels", "catalog");
    api.get::<Vec<d::TlsPresetDto>>("/admin/api/tls-presets", "catalog");
    api.get::<Vec<d::RulePresetDto>>("/admin/api/rule-presets", "catalog");
    api.get::<d::DefaultModelCatalogDto>("/admin/api/default-model-catalog", "catalog");
    api.json::<d::ApplyDefaultPricesRequest, d::ApplyDefaultPricesReportDto>(
        "post",
        "/admin/api/default-model-catalog/apply-prices",
        "catalog",
    );
    api.query::<d::ListQuery, d::Page<String>>("/admin/api/model-names", "provider-models");
    api.get::<Vec<d::RoutingTargetDto>>("/admin/api/operation-keys", "catalog");
    api.get::<Vec<d::VocabularyDto>>("/admin/api/tokenizer-vocabs", "tokenizer");
    api.json::<d::TokenizerFetch, d::VocabularyDto>(
        "post",
        "/admin/api/tokenizer-vocabs",
        "tokenizer",
    );
    api.get::<Option<d::TokenizerProgressDto>>("/admin/api/tokenizer-vocabs/progress", "tokenizer");
    api.empty("delete", "/admin/api/tokenizer-vocabs/{id}", "tokenizer");
    api.get::<d::TokenizerAuthDto>("/admin/api/tokenizer-auth", "tokenizer");
    api.json::<TokenBody, d::TokenizerAuthDto>("patch", "/admin/api/tokenizer-auth", "tokenizer");
    api.route("post","/admin/api/tokenizer-auth/reveal","tokenizer",None,vec![],Some(json!({"type":"object","required":["token"],"properties":{"token":{"type":["string","null"]}}})));
}
