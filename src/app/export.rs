//! Config export (§18): read all control-plane config via the persistence
//! backend, decrypt secrets to plaintext, emit the `import::Bundle` shape so
//! `export | import` round-trips. NOT usage/logs/rollups.

use serde_json::Value;

use crate::app::import::{Bundle, CredentialImport, UserKeyImport};
use crate::crypto::SecretCipher;
use crate::crypto::envelope::is_envelope;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{Credential, Scope, UserKey};

mod mappers;
use mappers::*;

#[cfg(test)]
mod tests;

/// Read every control-plane entity, decrypt secrets, and assemble the
/// `import::Bundle`. Preserves record ids so a re-import upserts the same rows
/// (stable cross-references). Reads from whichever backend `db` is — file or
/// db (multi-instance): both expose the identical list/get surface.
pub async fn export_bundle(
    db: &dyn PersistenceBackend,
    cipher: &dyn SecretCipher,
) -> anyhow::Result<Bundle> {
    let mut bundle = Bundle {
        schema_version: 1,
        orgs: Vec::new(),
        teams: Vec::new(),
        users: Vec::new(),
        user_keys: Vec::new(),
        route_permissions: Vec::new(),
        rate_limits: Vec::new(),
        quotas: Vec::new(),
        providers: Vec::new(),
        credentials: Vec::new(),
        provider_models: Vec::new(),
        routes: Vec::new(),
        route_members: Vec::new(),
        aliases: Vec::new(),
        routing_rules: Vec::new(),
        rule_sets: Vec::new(),
        rules: Vec::new(),
        provider_rule_sets: Vec::new(),
        instance_settings: Vec::new(),
    };

    // orgs → teams (+ collect the scope universe ids as we go, mirroring
    // snapshot::build / load_authz).
    let mut scopes: Vec<(Scope, i64)> = Vec::new();
    for org in db.list_orgs().await? {
        scopes.push((Scope::Org, org.id));
        for team in db.list_teams(org.id).await? {
            scopes.push((Scope::Team, team.id));
            bundle.teams.push(team_to_input(team));
        }
        bundle.orgs.push(org_to_input(org));
    }

    // users → keys (invert the seal to the bare api key).
    for user in db.list_users().await? {
        scopes.push((Scope::User, user.id));
        for key in db.list_user_keys(user.id).await? {
            bundle.user_keys.push(user_key_to_import(key, cipher)?);
        }
        bundle.users.push(user_to_input(user));
    }

    // authz scope universe: permissions / rate limits / quotas per scope row.
    for (scope, id) in scopes {
        for p in db.list_route_permissions(scope, id).await? {
            bundle.route_permissions.push(perm_to_input(p));
        }
        for r in db.list_rate_limits(scope, id).await? {
            bundle.rate_limits.push(rate_limit_to_input(r));
        }
        if let Some(q) = db.get_quota(scope, id).await? {
            bundle.quotas.push(quota_to_input(q));
        }
    }

    // providers → credentials (decrypt) / models / routing rules / rule-set
    // attachments.
    for provider in db.list_providers().await? {
        let pid = provider.id;
        for c in db.list_credentials(pid).await? {
            bundle.credentials.push(credential_to_import(c, cipher)?);
        }
        for m in db.list_provider_models(pid).await? {
            bundle.provider_models.push(provider_model_to_input(m));
        }
        for r in db.list_routing_rules(pid).await? {
            bundle.routing_rules.push(routing_rule_to_input(r));
        }
        for a in db.list_provider_rule_sets(pid).await? {
            bundle
                .provider_rule_sets
                .push(provider_rule_set_to_input(a));
        }
        bundle.providers.push(provider_to_input(provider));
    }

    // rule sets → rules.
    for set in db.list_rule_sets().await? {
        for rule in db.list_rules(set.id).await? {
            bundle.rules.push(rule_to_input(rule));
        }
        bundle.rule_sets.push(rule_set_to_input(set));
    }

    // routes → members; aliases; instance settings.
    for route in db.list_routes().await? {
        for m in db.list_route_members(route.id).await? {
            bundle.route_members.push(route_member_to_input(m));
        }
        bundle.routes.push(route_to_input(route));
    }
    for alias in db.list_aliases().await? {
        bundle.aliases.push(alias_to_input(alias));
    }
    bundle.instance_settings = db
        .list_instance_settings()
        .await?
        .into_iter()
        .map(settings_to_input)
        .collect();

    Ok(bundle)
}

/// Recover the bare api key from a stored `api_key_ciphertext` — the inverse of
/// `import_bundle`'s seal. Keyless mode stored the bare string verbatim; a real
/// cipher stored the envelope JSON serialized to a string. So: parse the column
/// as JSON — if it is an envelope object, `open` it (the plaintext is a JSON
/// string holding the bare key); otherwise the column already IS the bare key.
fn user_key_to_import(key: UserKey, cipher: &dyn SecretCipher) -> anyhow::Result<UserKeyImport> {
    let api_key = match serde_json::from_str::<Value>(&key.api_key_ciphertext) {
        Ok(v) if is_envelope(&v) => match cipher.open(&v)? {
            Value::String(s) => s,
            other => anyhow::bail!("decrypted user_key {} is not a string: {other}", key.id),
        },
        _ => key.api_key_ciphertext,
    };
    Ok(UserKeyImport {
        id: Some(key.id),
        user_id: key.user_id,
        api_key,
        label: key.label,
        enabled: key.enabled,
    })
}

/// Decrypt a credential's `secret_json` to plaintext for the export bundle
/// (`label` re-maps from the `name` column, the import shape's field).
fn credential_to_import(
    cred: Credential,
    cipher: &dyn SecretCipher,
) -> anyhow::Result<CredentialImport> {
    let secret_json = cipher.open(&cred.secret_json)?;
    Ok(CredentialImport {
        id: Some(cred.id),
        provider_id: cred.provider_id,
        label: cred.name,
        kind: cred.kind,
        secret_json,
        weight: cred.weight,
        rpm_limit: cred.rpm_limit,
        tpm_limit: cred.tpm_limit,
        proxy_url: cred.proxy_url,
        tls_fingerprint: cred.tls_fingerprint,
        enabled: cred.enabled,
    })
}
