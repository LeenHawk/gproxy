//! Native installer first-run options: starter providers and an admin API key.

use crate::channel::registry::ChannelRegistry;
use crate::crypto::SecretCipher;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{ProviderInput, UserKeyInput};

/// Apply the options offered by native first-run setup. Channel ids are
/// validated before any write, then materialized as enabled providers with the
/// channel's default routing table. The optional API key belongs to the admin
/// user and is idempotent by digest, so retrying a launcher after a slow boot
/// does not create duplicates.
pub async fn ensure(
    db: &dyn PersistenceBackend,
    cipher: &dyn SecretCipher,
    channels: &ChannelRegistry,
    admin_user: &str,
    admin_password: Option<&str>,
    selected_channels: &[String],
    admin_api_key: Option<&str>,
) -> anyhow::Result<()> {
    let selected = validate_channels(channels, selected_channels)?;
    let admin_api_key = validate_admin_api_key(admin_api_key)?;

    super::bootstrap::ensure_admin(db, admin_user, admin_password).await?;
    ensure_providers(db, channels, selected).await?;
    if let Some(bare) = admin_api_key {
        ensure_admin_api_key(db, cipher, admin_user, bare).await?;
    }
    Ok(())
}

fn validate_channels(
    channels: &ChannelRegistry,
    selected: &[String],
) -> anyhow::Result<Vec<String>> {
    let mut valid = Vec::new();
    for id in selected.iter().flat_map(|value| value.split(',')) {
        let id = id.trim();
        if id.is_empty() || valid.iter().any(|existing| existing == id) {
            continue;
        }
        if channels.get(id).is_none() {
            anyhow::bail!("unknown bootstrap channel: {id}");
        }
        valid.push(id.to_string());
    }
    Ok(valid)
}

fn validate_admin_api_key(key: Option<&str>) -> anyhow::Result<Option<&str>> {
    let key = key.map(str::trim).filter(|key| !key.is_empty());
    if let Some(key) = key
        && (!key.starts_with("sk-") || key.chars().count() < 32)
    {
        anyhow::bail!(
            "GPROXY_BOOTSTRAP_ADMIN_API_KEY must be a generated sk- key of at least 32 characters"
        );
    }
    Ok(key)
}

async fn ensure_providers(
    db: &dyn PersistenceBackend,
    channels: &ChannelRegistry,
    selected: Vec<String>,
) -> anyhow::Result<()> {
    let existing = db.list_providers().await?;
    for channel in selected {
        if existing.iter().any(|provider| provider.channel == channel) {
            continue;
        }
        let provider = db
            .upsert_provider(ProviderInput {
                id: None,
                name: channel.clone(),
                channel,
                label: None,
                settings_json: serde_json::json!({}),
                credential_strategy: "round_robin".to_string(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: true,
            })
            .await?;
        crate::api::routing::seed_default_routing(db, channels, provider.id, false)
            .await
            .map_err(|error| anyhow::anyhow!(error.message()))?;
    }
    Ok(())
}

async fn ensure_admin_api_key(
    db: &dyn PersistenceBackend,
    cipher: &dyn SecretCipher,
    admin_user: &str,
    bare: &str,
) -> anyhow::Result<()> {
    let admin = db
        .get_user_by_name(admin_user)
        .await?
        .filter(|user| user.is_admin && user.enabled)
        .ok_or_else(|| anyhow::anyhow!("bootstrap admin {admin_user:?} is missing or disabled"))?;
    let digest = crate::pipeline::auth::key_digest(bare);
    if db
        .list_user_keys(admin.id)
        .await?
        .iter()
        .any(|key| key.api_key_digest == digest)
    {
        return Ok(());
    }

    let sealed = cipher.seal(&serde_json::Value::String(bare.to_string()))?;
    let ciphertext = match sealed {
        serde_json::Value::String(value) => value,
        other => serde_json::to_string(&other)?,
    };
    db.upsert_user_key(UserKeyInput {
        id: None,
        user_id: admin.id,
        api_key_ciphertext: ciphertext,
        api_key_digest: digest,
        label: Some("installer".to_string()),
        enabled: true,
    })
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::NoopCipher;

    async fn store() -> (
        tempfile::TempDir,
        crate::store::persistence::FilePersistence,
    ) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = crate::store::persistence::FilePersistence::open(dir.path().to_path_buf())
            .await
            .expect("open");
        (dir, store)
    }

    #[tokio::test]
    async fn seeds_channels_and_admin_key_idempotently() {
        let (_dir, db) = store().await;
        let channels = ChannelRegistry::with_builtin();
        let selected = vec!["openai".to_string(), "codex".to_string()];
        let key = "sk-installer-test-0123456789abcdef";

        ensure(
            &db,
            &NoopCipher,
            &channels,
            "owner",
            Some("secret"),
            &selected,
            Some(key),
        )
        .await
        .unwrap();
        ensure(
            &db,
            &NoopCipher,
            &channels,
            "owner",
            None,
            &selected,
            Some(key),
        )
        .await
        .unwrap();

        let providers = db.list_providers().await.unwrap();
        assert_eq!(providers.len(), 2);
        for provider in providers {
            assert!(!db.list_routing_rules(provider.id).await.unwrap().is_empty());
        }
        let admin = db.get_user_by_name("owner").await.unwrap().unwrap();
        let keys = db.list_user_keys(admin.id).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].label.as_deref(), Some("installer"));
    }

    #[tokio::test]
    async fn invalid_input_is_rejected_before_writes() {
        let (_dir, db) = store().await;
        let error = ensure(
            &db,
            &NoopCipher,
            &ChannelRegistry::with_builtin(),
            "admin",
            Some("secret"),
            &["not-a-channel".to_string()],
            Some("weak"),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("unknown bootstrap channel"));
        assert!(db.list_users().await.unwrap().is_empty());
        assert!(db.list_providers().await.unwrap().is_empty());

        let error = ensure(
            &db,
            &NoopCipher,
            &ChannelRegistry::with_builtin(),
            "admin",
            Some("secret"),
            &[],
            Some("weak"),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("must be a generated sk- key"));
        assert!(db.list_users().await.unwrap().is_empty());
    }
}
