mod control;
mod identity;
mod logs;
mod process;
mod usage;

use std::collections::BTreeMap;

use gproxy_store::Store;
use gproxy_store::records::RecordBatch;

use super::model::{Legacy, SourceData};
use super::report::ImportCount;

pub(super) struct Context<'a> {
    pub store: &'a Store,
    pub cipher: &'a crate::secrets::EnvelopeCipher,
    pub organizations: BTreeMap<i64, i64>,
    pub teams: BTreeMap<i64, i64>,
    pub users: BTreeMap<i64, i64>,
    pub user_keys: BTreeMap<i64, i64>,
    pub providers: BTreeMap<i64, i64>,
    pub credentials: BTreeMap<i64, i64>,
    pub routes: BTreeMap<i64, i64>,
    pub price_rules: BTreeMap<i64, i64>,
    pub rule_sets: BTreeMap<i64, i64>,
}

pub(super) async fn run(
    store: &Store,
    cipher: &crate::secrets::EnvelopeCipher,
    data: SourceData,
    counts: &mut [ImportCount],
    source: &std::path::Path,
) -> Result<(), crate::AppError> {
    let mut context = Context {
        store,
        cipher,
        organizations: BTreeMap::new(),
        teams: BTreeMap::new(),
        users: BTreeMap::new(),
        user_keys: BTreeMap::new(),
        providers: BTreeMap::new(),
        credentials: BTreeMap::new(),
        routes: BTreeMap::new(),
        price_rules: BTreeMap::new(),
        rule_sets: BTreeMap::new(),
    };
    identity::base(&mut context, &data, counts).await?;
    control::base(&mut context, &data, counts).await?;
    identity::keys_and_quotas(&mut context, &data, counts).await?;
    control::pricing(&mut context, &data, counts).await?;
    process::run(&mut context, &data, counts).await?;
    usage::settings(&context, &data, counts).await?;
    usage::history(&context, &data, counts).await?;
    logs::run(&context, source, counts).await?;
    Ok(())
}

pub(super) async fn mapped<T>(
    context: &Context<'_>,
    source: &[Legacy<T>],
    batch: RecordBatch,
) -> Result<BTreeMap<i64, i64>, crate::AppError> {
    let ids = context.store.insert_record_batch(batch).await?;
    if ids.len() != source.len() {
        return Err(crate::AppError::Migration(
            "target returned the wrong number of inserted ids".into(),
        ));
    }
    Ok(source.iter().map(|value| value.id).zip(ids).collect())
}

pub(super) fn id(map: &BTreeMap<i64, i64>, old: i64) -> Result<i64, crate::AppError> {
    map.get(&old).copied().ok_or_else(|| {
        crate::AppError::Migration(format!("validated source reference {old} was not mapped"))
    })
}

pub(super) fn optional(
    map: &BTreeMap<i64, i64>,
    old: Option<i64>,
) -> Result<Option<i64>, crate::AppError> {
    old.map(|old| id(map, old)).transpose()
}

pub(super) fn mark(counts: &mut [ImportCount], entity: &str, imported: usize) {
    if let Some(count) = counts.iter_mut().find(|count| count.entity == entity) {
        count.imported = imported;
    }
}

pub(super) fn unsigned(value: i64, field: &str) -> Result<u64, crate::AppError> {
    u64::try_from(value)
        .map_err(|_| crate::AppError::Migration(format!("invalid {field} after validation")))
}

pub(super) fn unsigned32(value: i64, field: &str) -> Result<u32, crate::AppError> {
    u32::try_from(value)
        .map_err(|_| crate::AppError::Migration(format!("invalid {field} after validation")))
}

pub(super) fn existing(
    counts: Vec<(&'static str, u64)>,
    user_settings: usize,
) -> Vec<(&'static str, usize)> {
    let mut values = counts
        .into_iter()
        .filter(|(entity, _)| *entity != "oauth_clients")
        .map(|(entity, count)| (entity, usize::try_from(count).unwrap_or(usize::MAX)))
        .collect::<Vec<_>>();
    if let Some((_, count)) = values.iter_mut().find(|(entity, _)| *entity == "settings") {
        *count = user_settings;
    }
    values.retain(|(_, count)| *count != 0);
    values
}
