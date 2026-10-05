//! Deleting request history on an operator's say-so: captured logs and usage
//! records, by id or all at once.
//!
//! History is not configuration, so nothing here bumps the revision or
//! reloads. Usage records are what the console reports, not the settled quota
//! counters, billing entries or subscription windows, which stay untouched.
//! A capture still in progress is never deleted: its writer would only find
//! the row gone when it settles.

use std::sync::Arc;

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::usage::{capture_link, downstream_record, upstream_record, usage_record};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect, QueryTrait};

use crate::{SdkError, SdkResult, handle::Inner};

/// The most ids one deletion names; a console selects from one page.
pub const MAX_HISTORY_DELETE: usize = 500;

/// Which of the two capture tables a deletion addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub enum LogSide {
    Downstream,
    Upstream,
}

pub struct History<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

impl<'a, C> History<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> History<'_, C> {
    /// Delete settled captures of `side` by id; `None` deletes all of them.
    /// Their events go with them through the cascading foreign keys.
    pub async fn delete_logs(&self, side: LogSide, ids: Option<&[String]>) -> SdkResult<u64> {
        let store = &self.inner.store;
        let deleted = match side {
            LogSide::Downstream => {
                use downstream_record::{CaptureState, Column, Entity};
                let mut query =
                    Entity::delete_many().filter(Column::State.ne(CaptureState::InProgress));
                if let Some(ids) = checked(ids)? {
                    query = query.filter(Column::Id.is_in(ids.iter().cloned()));
                }
                store
                    .downstream_records()
                    .delete_where_many(vec![query])
                    .await?
            }
            LogSide::Upstream => {
                use upstream_record::{CaptureState, Column, Entity};
                let mut query =
                    Entity::delete_many().filter(Column::State.ne(CaptureState::InProgress));
                if let Some(ids) = checked(ids)? {
                    query = query.filter(Column::Id.is_in(ids.iter().cloned()));
                }
                store
                    .upstream_records()
                    .delete_where_many(vec![query])
                    .await?
            }
        };
        self.sweep_links().await?;
        store.collect_capture_garbage().await?;
        Ok(deleted.into_iter().sum())
    }

    /// Delete usage records by request id; `None` deletes all of them.
    pub async fn delete_usage(&self, ids: Option<&[String]>) -> SdkResult<u64> {
        let mut query = usage_record::Entity::delete_many();
        if let Some(ids) = checked(ids)? {
            query = query.filter(usage_record::Column::RequestId.is_in(ids.iter().cloned()));
        }
        let deleted = self
            .inner
            .store
            .usage_records()
            .delete_where_many(vec![query])
            .await?;
        self.sweep_links().await?;
        Ok(deleted.into_iter().sum())
    }

    /// Drop every association neither of whose captures nor upstream usage
    /// survives, the same rule retention applies.
    async fn sweep_links(&self) -> SdkResult<()> {
        let live_upstream = upstream_record::Entity::find()
            .select_only()
            .column(upstream_record::Column::Id)
            .into_query();
        let live_downstream = downstream_record::Entity::find()
            .select_only()
            .column(downstream_record::Column::Id)
            .into_query();
        let live_usage = usage_record::Entity::find()
            .select_only()
            .column(usage_record::Column::RequestId)
            .into_query();
        self.inner
            .store
            .capture_links()
            .delete_where_many(vec![
                capture_link::Entity::delete_many()
                    .filter(capture_link::Column::UpstreamId.not_in_subquery(live_upstream))
                    .filter(capture_link::Column::DownstreamId.not_in_subquery(live_downstream))
                    .filter(capture_link::Column::UpstreamId.not_in_subquery(live_usage)),
            ])
            .await?;
        Ok(())
    }
}

/// An explicit id list must name something and stay within one page.
fn checked(ids: Option<&[String]>) -> SdkResult<Option<&[String]>> {
    match ids {
        Some([]) => Err(SdkError::invalid("no ids to delete")),
        Some(ids) if ids.len() > MAX_HISTORY_DELETE => Err(SdkError::invalid(format!(
            "at most {MAX_HISTORY_DELETE} ids can be deleted at once"
        ))),
        other => Ok(other),
    }
}
