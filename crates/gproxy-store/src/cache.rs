//! `gproxy_cache::Cache` over three Store tables, for hosts that have no
//! memory-resident process and no Redis: edge isolates on D1 or libSQL. Every
//! operation is one atomic batch on the connection; expiry is compared in
//! SQL against the host clock, so peers on the same database agree on what
//! is live. Notifications have no transport here: `subscribe` yields the
//! initial `ResyncRequired` and then nothing, so a host must reload
//! authoritatively on its own schedule.

use crate::{
    Store,
    entity::cache::{cache_counter, cache_entry, cache_permit},
    repository::{affected, models},
};
use gproxy_cache::{
    Cache, CacheError, CasOutcome, Counter, Entry, IncrementOutcome, Notification, Replacement,
    Result, Subscription, Version,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, SelectProjection};
use sea_orm::sea_query::{Expr, ExprTrait, OnConflict, Query};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryTrait, Set};
use std::{sync::Arc, time::Duration};

pub struct StoreCache<C> {
    store: Arc<Store<C>>,
    limits: gproxy_cache::Limits,
}

impl<C> StoreCache<C> {
    pub fn new(store: Arc<Store<C>>) -> Self {
        Self {
            store,
            limits: gproxy_cache::Limits::default(),
        }
    }
    pub fn with_limits(mut self, limits: gproxy_cache::Limits) -> Self {
        self.limits = limits;
        self
    }
}

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn expiry(ttl: Duration, now: i64) -> Result<i64> {
    let ms = i64::try_from(ttl.as_millis()).map_err(|_| CacheError::Invalid("ttl too large"))?;
    if ms <= 0 {
        return Err(CacheError::Invalid("ttl must be positive"));
    }
    Ok(now.saturating_add(ms))
}

fn fresh() -> Result<Version> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|e| CacheError::Entropy(e.to_string()))?;
    Ok(Version::from_bytes(bytes))
}

fn version_of(bytes: &[u8]) -> Result<Version> {
    <[u8; 32]>::try_from(bytes)
        .map(Version::from_bytes)
        .map_err(|_| CacheError::Corrupt)
}

fn storage(error: impl std::fmt::Display) -> CacheError {
    // The contract has no storage variant; a database failure is a corrupt
    // cache from the caller's point of view: reload durable state.
    let _ = error;
    CacheError::Corrupt
}

struct Resync(bool);
#[async_trait::async_trait]
impl Subscription for Resync {
    async fn recv(&mut self) -> Result<Notification> {
        if !self.0 {
            self.0 = true;
            return Ok(Notification::ResyncRequired);
        }
        std::future::pending().await
    }
}

impl<C: BatchConnectionTrait + Send + Sync> StoreCache<C> {
    fn check_key(&self, key: &str) -> Result<()> {
        if key.is_empty() || key.len() > self.limits.max_key_bytes {
            return Err(CacheError::Invalid("invalid cache key length"));
        }
        Ok(())
    }

    async fn live_entry(&self, key: &str, now: i64) -> Result<Option<cache_entry::Model>> {
        Ok(self
            .store
            .cache_entries()
            .query(
                cache_entry::Entity::find_by_id(key.to_owned())
                    .filter(cache_entry::Column::ExpiresAtMs.gt(now)),
            )
            .await
            .map_err(storage)?
            .pop())
    }

    async fn live_counter(&self, key: &str, now: i64) -> Result<Option<cache_counter::Model>> {
        Ok(self
            .store
            .cache_counters()
            .query(
                cache_counter::Entity::find_by_id(key.to_owned())
                    .filter(cache_counter::Column::ExpiresAtMs.gt(now)),
            )
            .await
            .map_err(storage)?
            .pop())
    }

    /// Write `value` under a fresh version when the row is absent or expired,
    /// or when its version equals `expected`. Returns the new version when
    /// the write landed.
    async fn write(
        &self,
        key: &str,
        value: Vec<u8>,
        expires_at_ms: i64,
        expected: Option<Version>,
        now: i64,
    ) -> Result<Option<Version>> {
        let next = fresh()?;
        let backend = self.store.db.get_database_backend();
        let statement = match expected {
            Some(expected) => cache_entry::Entity::update_many()
                .col_expr(cache_entry::Column::Value, Expr::val(value))
                .col_expr(
                    cache_entry::Column::Version,
                    Expr::val(next.as_bytes().to_vec()),
                )
                .col_expr(cache_entry::Column::ExpiresAtMs, Expr::val(expires_at_ms))
                .filter(cache_entry::Column::Key.eq(key))
                .filter(cache_entry::Column::Version.eq(expected.as_bytes().to_vec()))
                .filter(cache_entry::Column::ExpiresAtMs.gt(now))
                .build(backend),
            None => {
                let expired = cache_entry::Column::ExpiresAtMs.lte(now);
                let conflict = OnConflict::column(cache_entry::Column::Key)
                    .value(
                        cache_entry::Column::Value,
                        Expr::case(expired.clone(), Expr::val(value.clone()))
                            .finally(Expr::col(cache_entry::Column::Value)),
                    )
                    .value(
                        cache_entry::Column::Version,
                        Expr::case(expired.clone(), Expr::val(next.as_bytes().to_vec()))
                            .finally(Expr::col(cache_entry::Column::Version)),
                    )
                    .value(
                        cache_entry::Column::ExpiresAtMs,
                        Expr::case(expired, Expr::val(expires_at_ms))
                            .finally(Expr::col(cache_entry::Column::ExpiresAtMs)),
                    )
                    .to_owned();
                cache_entry::Entity::insert(cache_entry::ActiveModel {
                    key: Set(key.to_owned()),
                    value: Set(value),
                    version: Set(next.as_bytes().to_vec()),
                    expires_at_ms: Set(expires_at_ms),
                })
                .on_conflict(conflict)
                .build(backend)
            }
        };
        let batch = vec![
            BatchStatement::Execute(statement),
            BatchStatement::Query(
                cache_entry::Entity::find_by_id(key.to_owned())
                    .filter(cache_entry::Column::Version.eq(next.as_bytes().to_vec()))
                    .batch_query(backend)
                    .map_err(storage)?,
            ),
        ];
        let mut results = self
            .store
            .db
            .batch(&batch)
            .await
            .map_err(storage)?
            .into_iter();
        affected(results.next().ok_or(CacheError::Corrupt)?).map_err(storage)?;
        let landed = !models::<cache_entry::Model>(results.next().ok_or(CacheError::Corrupt)?)
            .map_err(storage)?
            .is_empty();
        Ok(landed.then_some(next))
    }
}

#[async_trait::async_trait]
impl<C: BatchConnectionTrait + Send + Sync> Cache for StoreCache<C> {
    async fn get(&self, key: &str) -> Result<Option<Entry>> {
        self.check_key(key)?;
        let now = now_ms();
        self.live_entry(key, now)
            .await?
            .map(|row| {
                Ok(Entry {
                    value: row.value,
                    version: version_of(&row.version)?,
                })
            })
            .transpose()
    }

    /// One read for every key, instead of a query per key.
    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Entry>>> {
        for key in keys {
            self.check_key(key)?;
        }
        let now = now_ms();
        self.store
            .cache_entries()
            .get_many(keys)
            .await
            .map_err(storage)?
            .into_iter()
            .map(|row| {
                row.filter(|row| row.expires_at_ms > now)
                    .map(|row| {
                        Ok(Entry {
                            value: row.value,
                            version: version_of(&row.version)?,
                        })
                    })
                    .transpose()
            })
            .collect()
    }

    async fn put(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<Version> {
        self.check_key(key)?;
        if value.len() > self.limits.max_value_bytes {
            return Err(CacheError::Invalid("value exceeds the size limit"));
        }
        let now = now_ms();
        let expires = expiry(ttl, now)?;
        let next = fresh()?;
        let conflict = OnConflict::column(cache_entry::Column::Key)
            .update_columns([
                cache_entry::Column::Value,
                cache_entry::Column::Version,
                cache_entry::Column::ExpiresAtMs,
            ])
            .to_owned();
        let statement = cache_entry::Entity::insert(cache_entry::ActiveModel {
            key: Set(key.to_owned()),
            value: Set(value),
            version: Set(next.as_bytes().to_vec()),
            expires_at_ms: Set(expires),
        })
        .on_conflict(conflict)
        .build(self.store.db.get_database_backend());
        self.store
            .db
            .batch(&[BatchStatement::Execute(statement)])
            .await
            .map_err(storage)?;
        Ok(next)
    }

    async fn delete(&self, key: &str) -> Result<bool> {
        self.check_key(key)?;
        let now = now_ms();
        let statement = cache_entry::Entity::delete_by_id(key.to_owned())
            .filter(cache_entry::Column::ExpiresAtMs.gt(now))
            .build(self.store.db.get_database_backend());
        let rows = self
            .store
            .db
            .batch(&[BatchStatement::Execute(statement)])
            .await
            .map_err(storage)?
            .into_iter()
            .next()
            .map(affected)
            .transpose()
            .map_err(storage)?
            .unwrap_or(0);
        Ok(rows > 0)
    }

    async fn compare_exchange(
        &self,
        key: &str,
        expected: Option<Version>,
        replacement: Option<Replacement>,
    ) -> Result<CasOutcome> {
        self.check_key(key)?;
        let now = now_ms();
        match replacement {
            Some(replacement) => {
                if replacement.value.len() > self.limits.max_value_bytes {
                    return Err(CacheError::Invalid("value exceeds the size limit"));
                }
                let expires = expiry(replacement.ttl, now)?;
                Ok(
                    match self
                        .write(key, replacement.value, expires, expected, now)
                        .await?
                    {
                        Some(version) => CasOutcome::Applied(Some(version)),
                        None => CasOutcome::Conflict,
                    },
                )
            }
            None => {
                let backend = self.store.db.get_database_backend();
                let statement = match expected {
                    Some(expected) => cache_entry::Entity::delete_by_id(key.to_owned())
                        .filter(cache_entry::Column::Version.eq(expected.as_bytes().to_vec()))
                        .filter(cache_entry::Column::ExpiresAtMs.gt(now))
                        .build(backend),
                    None => {
                        // Expecting absence: succeed only when nothing live exists.
                        if self.live_entry(key, now).await?.is_some() {
                            return Ok(CasOutcome::Conflict);
                        }
                        return Ok(CasOutcome::Applied(None));
                    }
                };
                let rows = self
                    .store
                    .db
                    .batch(&[BatchStatement::Execute(statement)])
                    .await
                    .map_err(storage)?
                    .into_iter()
                    .next()
                    .map(affected)
                    .transpose()
                    .map_err(storage)?
                    .unwrap_or(0);
                Ok(if rows > 0 {
                    CasOutcome::Applied(None)
                } else {
                    CasOutcome::Conflict
                })
            }
        }
    }

    async fn counter(&self, key: &str) -> Result<Option<Counter>> {
        self.check_key(key)?;
        let now = now_ms();
        self.live_counter(key, now)
            .await?
            .map(|row| {
                Ok(Counter {
                    value: u64::try_from(row.value).map_err(|_| CacheError::Corrupt)?,
                    generation: version_of(&row.generation)?,
                })
            })
            .transpose()
    }

    /// Read, check the limit, then add with the read value as the CAS
    /// guard; a concurrent change makes the update miss and the loop retries.
    async fn increment(
        &self,
        key: &str,
        amount: u64,
        limit: u64,
        ttl: Duration,
    ) -> Result<IncrementOutcome> {
        self.check_key(key)?;
        if amount == 0 || limit == 0 || amount > limit {
            return Err(CacheError::Invalid(
                "increment amount and limit must be positive and amount <= limit",
            ));
        }
        let backend = self.store.db.get_database_backend();
        for _ in 0..8 {
            let now = now_ms();
            let expires = expiry(ttl, now)?;
            let current = self.live_counter(key, now).await?;
            let current_value = current
                .as_ref()
                .map(|row| u64::try_from(row.value).unwrap_or(0))
                .unwrap_or(0);
            if amount > limit.saturating_sub(current_value) {
                return Ok(IncrementOutcome::Limited {
                    current: current_value,
                });
            }
            let next_value =
                i64::try_from(current_value + amount).map_err(|_| CacheError::Corrupt)?;
            let (statement, generation) = match &current {
                Some(row) => (
                    cache_counter::Entity::update_many()
                        .col_expr(cache_counter::Column::Value, Expr::val(next_value))
                        .filter(cache_counter::Column::Key.eq(key))
                        .filter(cache_counter::Column::Value.eq(row.value))
                        .filter(cache_counter::Column::Generation.eq(row.generation.clone()))
                        .filter(cache_counter::Column::ExpiresAtMs.gt(now))
                        .build(backend),
                    version_of(&row.generation)?,
                ),
                None => {
                    let generation = fresh()?;
                    let expired = cache_counter::Column::ExpiresAtMs.lte(now);
                    let conflict = OnConflict::column(cache_counter::Column::Key)
                        .value(
                            cache_counter::Column::Value,
                            Expr::case(expired.clone(), Expr::val(next_value))
                                .finally(Expr::col(cache_counter::Column::Value)),
                        )
                        .value(
                            cache_counter::Column::Generation,
                            Expr::case(expired.clone(), Expr::val(generation.as_bytes().to_vec()))
                                .finally(Expr::col(cache_counter::Column::Generation)),
                        )
                        .value(
                            cache_counter::Column::ExpiresAtMs,
                            Expr::case(expired, Expr::val(expires))
                                .finally(Expr::col(cache_counter::Column::ExpiresAtMs)),
                        )
                        .to_owned();
                    (
                        cache_counter::Entity::insert(cache_counter::ActiveModel {
                            key: Set(key.to_owned()),
                            value: Set(next_value),
                            generation: Set(generation.as_bytes().to_vec()),
                            expires_at_ms: Set(expires),
                        })
                        .on_conflict(conflict)
                        .build(backend),
                        generation,
                    )
                }
            };
            let batch = vec![
                BatchStatement::Execute(statement),
                BatchStatement::Query(
                    cache_counter::Entity::find_by_id(key.to_owned())
                        .filter(cache_counter::Column::Value.eq(next_value))
                        .filter(
                            cache_counter::Column::Generation.eq(generation.as_bytes().to_vec()),
                        )
                        .batch_query(backend)
                        .map_err(storage)?,
                ),
            ];
            let mut results = self
                .store
                .db
                .batch(&batch)
                .await
                .map_err(storage)?
                .into_iter();
            affected(results.next().ok_or(CacheError::Corrupt)?).map_err(storage)?;
            let landed =
                !models::<cache_counter::Model>(results.next().ok_or(CacheError::Corrupt)?)
                    .map_err(storage)?
                    .is_empty();
            if landed {
                return Ok(IncrementOutcome::Applied(Counter {
                    value: current_value + amount,
                    generation,
                }));
            }
        }
        Err(CacheError::Capacity)
    }

    async fn decrement(
        &self,
        key: &str,
        generation: Version,
        amount: u64,
    ) -> Result<Option<Counter>> {
        self.check_key(key)?;
        let now = now_ms();
        let amount_i = i64::try_from(amount).map_err(|_| CacheError::Corrupt)?;
        let backend = self.store.db.get_database_backend();
        let batch = vec![
            BatchStatement::Execute(
                cache_counter::Entity::update_many()
                    .col_expr(
                        cache_counter::Column::Value,
                        Expr::col(cache_counter::Column::Value).sub(Expr::val(amount_i)),
                    )
                    .filter(cache_counter::Column::Key.eq(key))
                    .filter(cache_counter::Column::Generation.eq(generation.as_bytes().to_vec()))
                    .filter(cache_counter::Column::Value.gte(amount_i))
                    .filter(cache_counter::Column::ExpiresAtMs.gt(now))
                    .build(backend),
            ),
            BatchStatement::Query(
                cache_counter::Entity::find_by_id(key.to_owned())
                    .filter(cache_counter::Column::Generation.eq(generation.as_bytes().to_vec()))
                    .filter(cache_counter::Column::ExpiresAtMs.gt(now))
                    .batch_query(backend)
                    .map_err(storage)?,
            ),
        ];
        let mut results = self
            .store
            .db
            .batch(&batch)
            .await
            .map_err(storage)?
            .into_iter();
        let rows = affected(results.next().ok_or(CacheError::Corrupt)?).map_err(storage)?;
        let row = models::<cache_counter::Model>(results.next().ok_or(CacheError::Corrupt)?)
            .map_err(storage)?
            .pop();
        match row {
            None => Ok(None),
            Some(row) if rows == 0 => {
                if u64::try_from(row.value).unwrap_or(0) < amount {
                    Err(CacheError::Underflow)
                } else {
                    Ok(None)
                }
            }
            Some(row) => Ok(Some(Counter {
                value: u64::try_from(row.value).map_err(|_| CacheError::Corrupt)?,
                generation,
            })),
        }
    }

    /// Insert the owner, then count live owners; over the limit the insert is
    /// withdrawn. Two racing callers can both withdraw, which only delays one.
    async fn acquire_permit(
        &self,
        key: &str,
        limit: u32,
        ttl: Duration,
    ) -> Result<Option<Version>> {
        self.check_key(key)?;
        if limit == 0 || limit > self.limits.max_permits_per_key {
            return Err(CacheError::Invalid("permit limit out of range"));
        }
        let now = now_ms();
        let expires = expiry(ttl, now)?;
        let owner = fresh()?;
        let backend = self.store.db.get_database_backend();
        let count = Query::select()
            .expr_as(
                Expr::col(cache_permit::Column::Key).count(),
                sea_orm::sea_query::Alias::new("count"),
            )
            .from(cache_permit::Entity)
            .and_where(cache_permit::Column::Key.eq(key))
            .and_where(cache_permit::Column::ExpiresAtMs.gt(now))
            .to_owned();
        let batch = vec![
            BatchStatement::Execute(
                cache_permit::Entity::delete_many()
                    .filter(cache_permit::Column::Key.eq(key))
                    .filter(cache_permit::Column::ExpiresAtMs.lte(now))
                    .build(backend),
            ),
            BatchStatement::Execute(
                cache_permit::Entity::insert(cache_permit::ActiveModel {
                    key: Set(key.to_owned()),
                    owner: Set(owner.as_bytes().to_vec()),
                    expires_at_ms: Set(expires),
                })
                .build(backend),
            ),
            BatchStatement::Query(gproxy_seaorm::BatchQuery::new(
                backend.build(&count),
                gproxy_seaorm::Projection::new()
                    .column("count", gproxy_seaorm::D1Type::I64, false)
                    .map_err(storage)?,
            )),
        ];
        let mut results = self
            .store
            .db
            .batch(&batch)
            .await
            .map_err(storage)?
            .into_iter();
        results.next();
        results.next();
        let live = match results.next().ok_or(CacheError::Corrupt)? {
            gproxy_seaorm::BatchResult::Rows(rows) => rows
                .first()
                .and_then(|row| row.try_get::<i64>("", "count").ok())
                .unwrap_or(0),
            _ => return Err(CacheError::Corrupt),
        };
        if live > i64::from(limit) {
            self.release_permit(key, owner).await?;
            return Ok(None);
        }
        Ok(Some(owner))
    }

    async fn renew_permit(&self, key: &str, owner: Version, ttl: Duration) -> Result<bool> {
        self.check_key(key)?;
        let now = now_ms();
        let expires = expiry(ttl, now)?;
        let statement = cache_permit::Entity::update_many()
            .col_expr(cache_permit::Column::ExpiresAtMs, Expr::val(expires))
            .filter(cache_permit::Column::Key.eq(key))
            .filter(cache_permit::Column::Owner.eq(owner.as_bytes().to_vec()))
            .filter(cache_permit::Column::ExpiresAtMs.gt(now))
            .build(self.store.db.get_database_backend());
        let rows = self
            .store
            .db
            .batch(&[BatchStatement::Execute(statement)])
            .await
            .map_err(storage)?
            .into_iter()
            .next()
            .map(affected)
            .transpose()
            .map_err(storage)?
            .unwrap_or(0);
        Ok(rows > 0)
    }

    async fn release_permit(&self, key: &str, owner: Version) -> Result<bool> {
        self.check_key(key)?;
        let statement =
            cache_permit::Entity::delete_by_id((key.to_owned(), owner.as_bytes().to_vec()))
                .build(self.store.db.get_database_backend());
        let rows = self
            .store
            .db
            .batch(&[BatchStatement::Execute(statement)])
            .await
            .map_err(storage)?
            .into_iter()
            .next()
            .map(affected)
            .transpose()
            .map_err(storage)?
            .unwrap_or(0);
        Ok(rows > 0)
    }

    async fn publish(&self, _topic: &str, _payload: Vec<u8>) -> Result<()> {
        Ok(())
    }

    async fn subscribe(&self, _topic: &str) -> Result<Box<dyn Subscription>> {
        Ok(Box::new(Resync(false)))
    }
}
