use super::{entry, state};
use crate::records::{CredentialInput, ProviderInput};
use crate::{Store, StoreError};
use gproxy_core::channel_api::QuotaSnapshot;

pub(super) async fn run(store: &Store, unrelated_id: i64) -> Result<(), StoreError> {
    let unrelated = store.credential(unrelated_id).await?.unwrap();
    let unaffected = store.credential_quota_snapshot(unrelated_id).await?;
    let mut provider = ProviderInput {
        name: "quota-provider".into(),
        label: None,
        channel: "deepseek".into(),
        settings: serde_json::json!({"base_url": "https://first.invalid"}),
        credential_strategy: "round_robin".into(),
        proxy_url: None,
        tls_fingerprint: None,
        enabled: true,
    };
    let id = store.insert_provider(&provider).await?;
    let credential = CredentialInput {
        provider_id: id,
        label: None,
        kind: "api_key".into(),
        envelope: unrelated.envelope,
        enabled: true,
        weight: 100,
        rpm_limit: None,
        tpm_limit: None,
        proxy_url: None,
        tls_fingerprint: None,
    };
    let ids = [
        store.insert_credential(&credential).await?,
        store.insert_credential(&credential).await?,
    ];
    for credential_id in ids {
        store
            .save_credential_quota_source(
                credential_id,
                0,
                &state("first", 100, false),
                Some(&[entry("first", "budget", 100, 10)]),
            )
            .await?;
        store
            .observe_credential_quota_entries(
                credential_id,
                0,
                &[entry("response_limits", "rate", 100, 10)],
            )
            .await?;
    }
    let saved = store.credential_quota_snapshot(ids[0]).await?;
    provider.name = "renamed-provider".into();
    provider.label = Some("display label".into());
    provider.credential_strategy = "random".into();
    provider.enabled = false;
    assert!(store.update_provider(id, &provider).await?);
    for credential_id in ids {
        assert_eq!(store.credential(credential_id).await?.unwrap().version, 0);
        assert_eq!(store.credential_quota_snapshot(credential_id).await?, saved);
    }
    provider.settings = serde_json::json!({"base_url": "https://second.invalid"});
    assert!(store.update_provider(id, &provider).await?);
    for credential_id in ids {
        assert_eq!(store.credential(credential_id).await?.unwrap().version, 1);
        assert_eq!(
            store.credential_quota_snapshot(credential_id).await?,
            QuotaSnapshot::default()
        );
        store
            .save_credential_quota_source(
                credential_id,
                0,
                &state("first", 200, false),
                Some(&[entry("first", "budget", 200, 99)]),
            )
            .await?;
        store
            .observe_credential_quota_entries(
                credential_id,
                0,
                &[entry("response_limits", "rate", 200, 99)],
            )
            .await?;
        assert_eq!(
            store.credential_quota_snapshot(credential_id).await?,
            QuotaSnapshot::default()
        );
        store
            .save_credential_quota_source(
                credential_id,
                1,
                &state("first", 200, false),
                Some(&[entry("first", "budget", 200, 20)]),
            )
            .await?;
    }
    assert_eq!(
        store.credential_quota_snapshot(unrelated_id).await?,
        unaffected
    );
    assert_eq!(
        store.credential(unrelated_id).await?.unwrap().version,
        unrelated.version
    );
    assert!(store.update_provider(id, &provider).await?);
    assert_eq!(store.credential(ids[0]).await?.unwrap().version, 1);
    for (index, (channel, proxy, fingerprint)) in [
        ("openrouter", None, None),
        ("openrouter", Some("http://proxy.invalid"), None),
        ("openrouter", None, None),
        (
            "openrouter",
            None,
            Some(serde_json::json!({"profile": "chrome"})),
        ),
        ("openrouter", None, None),
    ]
    .into_iter()
    .enumerate()
    {
        provider.channel = channel.into();
        provider.proxy_url = proxy.map(str::to_owned);
        provider.tls_fingerprint = fingerprint;
        assert!(store.update_provider(id, &provider).await?);
        assert_eq!(
            store.credential(ids[0]).await?.unwrap().version,
            index as u64 + 2
        );
    }
    assert!(store.delete_provider(id).await?);
    Ok(())
}
