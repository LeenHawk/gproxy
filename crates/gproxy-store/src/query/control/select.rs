use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::select_all;

pub(crate) fn select_providers() -> Result<Statement, StoreError> {
    select_all(
        "providers",
        &[
            "id",
            "name",
            "channel",
            "settings_json",
            "enabled",
            "tls_fingerprint",
        ],
    )
}

pub(crate) fn select_credential_meta() -> Result<Statement, StoreError> {
    select_all(
        "credentials",
        &[
            "id",
            "provider_id",
            "version",
            "enabled",
            "weight",
            "rpm_limit",
            "tpm_limit",
            "proxy_url",
            "tls_fingerprint",
        ],
    )
}

pub(crate) fn select_admin_credentials() -> Result<Statement, StoreError> {
    select_all(
        "credentials",
        &[
            "id",
            "provider_id",
            "label",
            "version",
            "enabled",
            "weight",
            "rpm_limit",
            "tpm_limit",
            "proxy_url",
            "tls_fingerprint",
        ],
    )
}

pub(crate) fn select_routes() -> Result<Statement, StoreError> {
    select_all("routes", &["id", "name", "max_attempts", "enabled"])
}

pub(crate) fn select_route_members() -> Result<Statement, StoreError> {
    select_all(
        "route_members",
        &[
            "id",
            "route_id",
            "provider_id",
            "credential_id",
            "upstream_model",
            "tier",
            "weight",
            "enabled",
        ],
    )
}

pub(crate) fn select_aliases() -> Result<Statement, StoreError> {
    select_all(
        "aliases",
        &[
            "id",
            "alias",
            "target",
            "provider_id",
            "priority",
            "enabled",
        ],
    )
}

pub(crate) fn select_exposed_models() -> Result<Statement, StoreError> {
    select_all("exposed_models", &["id", "name", "route_id", "enabled"])
}

pub(crate) fn select_price_rules() -> Result<Statement, StoreError> {
    select_all(
        "price_rules",
        &[
            "id",
            "provider_id",
            "model_pattern",
            "tiers_json",
            "priority",
            "enabled",
        ],
    )
}

pub(crate) fn select_price_rates() -> Result<Statement, StoreError> {
    select_all(
        "price_rates",
        &[
            "id",
            "rule_id",
            "metric",
            "unit_size",
            "price",
            "conditions_json",
            "priority",
        ],
    )
}

pub(crate) fn select_settings() -> Result<Statement, StoreError> {
    select_all("settings", &["key", "value_json"])
}
