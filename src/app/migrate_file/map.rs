//! MIGRATE-FILE (temporary 2.x bridge, remove in 2.3): map rows to a v2 bundle.

use super::read::LegacyData;
use crate::app::export::{credential_to_import, user_key_to_import};
use crate::app::import::Bundle;
use crate::crypto::SecretCipher;
use crate::store::persistence::records::{Credential, UserKey};

pub(super) fn to_bundle(data: LegacyData, cipher: &dyn SecretCipher) -> anyhow::Result<Bundle> {
    let credentials = data
        .credentials
        .into_iter()
        .map(|x| {
            credential_to_import(
                Credential {
                    id: x.id,
                    provider_id: x.provider_id,
                    name: x.name,
                    kind: x.kind,
                    secret_json: x.secret_json,
                    weight: x.weight,
                    rpm_limit: x.rpm_limit,
                    tpm_limit: x.tpm_limit,
                    proxy_url: x.proxy_url,
                    tls_fingerprint: x.tls_fingerprint,
                    enabled: x.enabled,
                    created_at: 0,
                    updated_at: 0,
                },
                cipher,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let user_keys = data
        .user_keys
        .into_iter()
        .map(|x| {
            user_key_to_import(
                UserKey {
                    id: x.id,
                    user_id: x.user_id,
                    api_key_ciphertext: x.api_key_ciphertext,
                    api_key_digest: x.api_key_digest,
                    label: x.label,
                    enabled: x.enabled,
                    created_at: 0,
                    updated_at: 0,
                },
                cipher,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(Bundle {
        schema_version: 1,
        orgs: data.orgs.into_iter().map(Into::into).collect(),
        teams: data.teams.into_iter().map(Into::into).collect(),
        users: data.users.into_iter().map(Into::into).collect(),
        user_keys,
        route_permissions: data.route_permissions.into_iter().map(Into::into).collect(),
        rate_limits: data.rate_limits.into_iter().map(Into::into).collect(),
        quotas: data.quotas.into_iter().map(Into::into).collect(),
        providers: data.providers.into_iter().map(Into::into).collect(),
        credentials,
        provider_models: data.provider_models.into_iter().map(Into::into).collect(),
        price_rules: data.price_rules.into_iter().map(Into::into).collect(),
        routes: data.routes.into_iter().map(Into::into).collect(),
        route_members: data.route_members.into_iter().map(Into::into).collect(),
        aliases: data.aliases.into_iter().map(Into::into).collect(),
        routing_rules: data.routing_rules.into_iter().map(Into::into).collect(),
        rule_sets: data.rule_sets.into_iter().map(Into::into).collect(),
        rules: data.rules.into_iter().map(Into::into).collect(),
        provider_rule_sets: data
            .provider_rule_sets
            .into_iter()
            .map(Into::into)
            .collect(),
        instance_settings: data.instance_settings.into_iter().map(Into::into).collect(),
    })
}
