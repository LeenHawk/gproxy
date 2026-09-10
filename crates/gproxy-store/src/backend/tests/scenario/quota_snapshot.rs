mod merge;
mod provider;

use gproxy_core::channel_api::{
    QuotaAllowance, QuotaEntry, QuotaKind, QuotaQueryMode, QuotaRefreshError, QuotaResetCredits,
    QuotaScope, QuotaSnapshot, QuotaSource, QuotaSourceState, QuotaSubject, QuotaSupport,
    QuotaValue,
};

use crate::{Store, StoreError};

pub(super) async fn run(store: &Store, credential_id: i64) -> Result<QuotaSnapshot, StoreError> {
    let version = store.credential(credential_id).await?.unwrap().version;
    let first = entry("first", "budget", 100, 10);
    store
        .save_credential_quota_source(
            credential_id,
            version,
            &state("first", 100, false),
            Some(std::slice::from_ref(&first)),
        )
        .await?;
    let second = entry("second", "budget", 150, 20);
    store
        .save_credential_quota_source(
            credential_id,
            version,
            &state("second", 150, false),
            Some(std::slice::from_ref(&second)),
        )
        .await?;
    let failed = state("first", 200, true);
    store
        .save_credential_quota_source(credential_id, version, &failed, None)
        .await?;
    let snapshot = store.credential_quota_snapshot(credential_id).await?;
    assert_eq!(snapshot.entries, vec![first.clone(), second.clone()]);
    assert_eq!(snapshot.sources[0].observed_at_ms, Some(100));
    assert_eq!(snapshot.sources[0].attempted_at_ms, Some(200));
    assert_eq!(
        snapshot.sources[0].reset_credits,
        state("first", 100, false).reset_credits
    );
    assert_eq!(snapshot.sources[0].error, failed.error);
    assert_eq!(snapshot.sources[1], state("second", 150, false));

    store
        .save_credential_quota_source(
            credential_id,
            version,
            &state("first", 199, false),
            Some(&[entry("first", "budget", 199, 99)]),
        )
        .await?;
    assert_eq!(
        store.credential_quota_snapshot(credential_id).await?,
        snapshot
    );
    let replacement = entry("first", "budget", 200, 30);
    store
        .save_credential_quota_source(
            credential_id,
            version,
            &state("first", 200, false),
            Some(std::slice::from_ref(&replacement)),
        )
        .await?;
    store
        .save_credential_quota_source(credential_id, version, &state("first", 200, true), None)
        .await?;
    store
        .save_credential_quota_source(
            credential_id,
            version,
            &state("first", 200, false),
            Some(&[entry("first", "budget", 200, 99)]),
        )
        .await?;
    let snapshot = store.credential_quota_snapshot(credential_id).await?;
    assert_eq!(snapshot.entries, vec![replacement, second]);
    assert!(snapshot.sources[0].error.is_none());
    store
        .save_credential_quota_source(
            credential_id,
            version,
            &state("first", 201, false),
            Some(&[]),
        )
        .await?;
    let snapshot = store.credential_quota_snapshot(credential_id).await?;
    assert_eq!(snapshot.entries.len(), 1);

    let model_a = entry("rate_limits", "model-a", 100, 1);
    let model_b = entry("rate_limits", "model-b", 100, 2);
    let a_entries = [model_a.clone()];
    let b_entries = [model_b.clone()];
    let (left, right) = tokio::join!(
        store.observe_credential_quota_entries(credential_id, version, &a_entries),
        store.observe_credential_quota_entries(credential_id, version, &b_entries),
    );
    left?;
    right?;
    store
        .observe_credential_quota_entries(
            credential_id,
            version,
            &[entry("rate_limits", "model-a", 99, 9)],
        )
        .await?;
    store
        .observe_credential_quota_entries(
            credential_id,
            version,
            &[entry("rate_limits", "model-b", 100, 9)],
        )
        .await?;
    let snapshot = store.credential_quota_snapshot(credential_id).await?;
    assert_eq!(&snapshot.entries[1..], &[model_a, model_b]);
    let model_a = entry("rate_limits", "model-a", 101, 3);
    store
        .observe_credential_quota_entries(credential_id, version, std::slice::from_ref(&model_a))
        .await?;
    let snapshot = store.credential_quota_snapshot(credential_id).await?;
    assert_eq!(snapshot.entries[1], model_a);

    lifecycle(store, credential_id, &first, &model_a).await?;
    provider::run(store, credential_id).await?;
    merge::run(store, credential_id, version).await
}

fn state(id: &str, attempted_at_ms: i64, failed: bool) -> QuotaSourceState {
    QuotaSourceState {
        capability: QuotaSource {
            id: id.into(),
            label: id.into(),
            kinds: vec![QuotaKind::Budget],
            mode: QuotaQueryMode::Probe,
            support: QuotaSupport::Ready,
            reason: None,
            automatic: true,
        },
        attempted_at_ms: Some(attempted_at_ms),
        observed_at_ms: (!failed).then_some(attempted_at_ms),
        reset_credits: (!failed).then_some(QuotaResetCredits {
            available_count: 2,
            expires_at: None,
        }),
        error: failed.then_some(QuotaRefreshError {
            code: "timeout".into(),
            message: "request timed out".into(),
        }),
    }
}

fn entry(source_id: &str, id: &str, observed_at_ms: i64, used: i64) -> QuotaEntry {
    QuotaEntry {
        id: id.into(),
        source_id: source_id.into(),
        label: None,
        subject: QuotaSubject::Key,
        model_scope: QuotaScope::All,
        observed_at_ms,
        value: QuotaValue::Budget(QuotaAllowance {
            used: Some(used.into()),
            ..Default::default()
        }),
    }
}

async fn lifecycle(
    store: &Store,
    credential_id: i64,
    first: &QuotaEntry,
    model_a: &QuotaEntry,
) -> Result<(), StoreError> {
    let credential = store.credential(credential_id).await?.unwrap();
    let doomed = crate::records::CredentialInput {
        provider_id: credential.provider_id,
        label: Some("quota snapshot deletion".into()),
        kind: credential.kind,
        envelope: credential.envelope,
        enabled: true,
        weight: 100,
        rpm_limit: None,
        tpm_limit: None,
        proxy_url: None,
        tls_fingerprint: None,
    };
    let doomed_id = store.insert_credential(&doomed).await?;
    store
        .save_credential_quota_source(
            doomed_id,
            0,
            &state("first", 100, false),
            Some(std::slice::from_ref(first)),
        )
        .await?;
    store
        .observe_credential_quota_entries(doomed_id, 0, std::slice::from_ref(model_a))
        .await?;
    let saved = store.credential_quota_snapshot(doomed_id).await?;
    store
        .persist_credential_rotation(doomed_id, &doomed.envelope, 0)
        .await?;
    store
        .save_credential_quota_source(doomed_id, 0, &state("first", 999, true), None)
        .await?;
    store
        .observe_credential_quota_entries(
            doomed_id,
            0,
            &[entry("rate_limits", "model-a", 999, 999)],
        )
        .await?;
    assert_eq!(store.credential_quota_snapshot(doomed_id).await?, saved);
    store
        .update_credential(
            doomed_id,
            &crate::records::CredentialUpdateInput {
                provider_id: doomed.provider_id,
                label: None,
                kind: doomed.kind,
                envelope: Some(doomed.envelope),
                enabled: true,
                weight: 100,
                rpm_limit: None,
                tpm_limit: None,
                proxy_url: None,
                tls_fingerprint: None,
            },
        )
        .await?;
    assert_eq!(
        store.credential_quota_snapshot(doomed_id).await?,
        QuotaSnapshot::default()
    );
    store
        .save_credential_quota_source(doomed_id, 1, &state("first", 999, true), None)
        .await?;
    store
        .observe_credential_quota_entries(doomed_id, 1, std::slice::from_ref(model_a))
        .await?;
    assert_eq!(
        store.credential_quota_snapshot(doomed_id).await?,
        QuotaSnapshot::default()
    );
    store
        .save_credential_quota_source(
            doomed_id,
            2,
            &state("first", 999, false),
            Some(std::slice::from_ref(first)),
        )
        .await?;
    store
        .observe_credential_quota_entries(doomed_id, 2, std::slice::from_ref(model_a))
        .await?;
    assert_eq!(
        store
            .credential_quota_snapshot(doomed_id)
            .await?
            .entries
            .len(),
        2
    );
    assert!(store.delete_credential(doomed_id).await?);
    assert_eq!(
        store.credential_quota_snapshot(doomed_id).await?,
        QuotaSnapshot::default()
    );
    store
        .observe_credential_quota_entries(doomed_id, 0, std::slice::from_ref(model_a))
        .await?;
    store
        .save_credential_quota_source(doomed_id, 0, &state("first", 100, true), None)
        .await?;
    assert_eq!(
        store.credential_quota_snapshot(doomed_id).await?,
        QuotaSnapshot::default()
    );
    Ok(())
}
