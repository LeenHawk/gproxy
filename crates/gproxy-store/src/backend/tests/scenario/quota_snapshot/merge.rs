use super::{entry, state};
use crate::{Store, StoreError};
use gproxy_core::channel_api::{QuotaEntry, QuotaSnapshot};

pub(super) async fn run(store: &Store, id: i64, version: u64) -> Result<QuotaSnapshot, StoreError> {
    let unchanged = store.credential_quota_snapshot(id).await?.entries;
    let probe = [
        entry("subscription", "primary", 300, 30),
        entry("subscription", "weekly", 300, 40),
    ];
    store
        .save_credential_quota_source(
            id,
            version,
            &state("subscription", 300, false),
            Some(&probe),
        )
        .await?;
    store
        .observe_credential_quota_entries(
            id,
            version,
            &[
                entry("subscription", "primary", 299, 99),
                entry("subscription", "weekly", 300, 99),
                entry("subscription", "secondary", 300, 50),
            ],
        )
        .await?;
    let saved = store.credential_quota_snapshot(id).await?;
    assert_eq!(find(&saved, "primary"), &probe[0]);
    assert_eq!(find(&saved, "weekly"), &probe[1]);
    assert_eq!(
        find(&saved, "secondary"),
        &entry("subscription", "secondary", 300, 50)
    );
    assert_eq!(saved.entries.len(), unchanged.len() + 3);
    store
        .observe_credential_quota_entries(id, version, &[entry("subscription", "primary", 301, 31)])
        .await?;
    let saved = store.credential_quota_snapshot(id).await?;
    assert_eq!(
        find(&saved, "primary"),
        &entry("subscription", "primary", 301, 31)
    );
    assert_eq!(find(&saved, "weekly"), &probe[1]);
    let refreshed = [entry("subscription", "primary", 302, 32), probe[1].clone()];
    store
        .save_credential_quota_source(
            id,
            version,
            &state("subscription", 302, false),
            Some(&refreshed),
        )
        .await?;
    store
        .observe_credential_quota_entries(id, version, &[entry("subscription", "primary", 302, 99)])
        .await?;
    let saved = store.credential_quota_snapshot(id).await?;
    assert_eq!(find(&saved, "primary"), &refreshed[0]);
    assert_eq!(
        find(&saved, "secondary"),
        &entry("subscription", "secondary", 300, 50)
    );
    assert_eq!(saved.entries.len(), unchanged.len() + 3);
    assert!(unchanged.iter().all(|entry| saved.entries.contains(entry)));
    Ok(saved)
}

fn find<'a>(snapshot: &'a QuotaSnapshot, id: &str) -> &'a QuotaEntry {
    snapshot
        .entries
        .iter()
        .find(|entry| entry.source_id == "subscription" && entry.id == id)
        .unwrap()
}
