//! Reads over separate upstream/downstream captures and their many-to-many links.
use super::{MAX_DETAIL_EVENTS, filter};
use crate::{
    SdkError, SdkResult,
    dto::{
        CaptureDetailDto, CaptureEventDto, CaptureRecordDto, LogBodyDto, LogDetailDto, LogEntryDto,
        LogPageDto, LogQuery, UsageRecordDto,
    },
    handle::Inner,
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::usage::{
    capture_event, capture_link, capture_record, downstream_event, downstream_record,
    upstream_event, upstream_record, usage_record,
};
use sea_orm::{
    ActiveEnum, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    QueryTrait,
};
use std::sync::Arc;

pub struct Logs<'a, C> {
    inner: &'a Arc<Inner<C>>,
}
impl<'a, C> Logs<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }
}

// Identical paging over two physical tables. No UNION or side predicate on the hot read.
macro_rules! list_records {
    ($self:ident, $query:ident, $record:ident, $repository:ident, $upstream:expr) => {{
        use $record::Column as C;
        let query = $query;
        let mut condition = Condition::all();
        if let Some(from) = query.from_ms {
            condition = condition.add(C::StartedAtMs.gte(from));
        }
        if let Some(to) = query.to_ms {
            condition = condition.add(C::StartedAtMs.lt(to));
        }
        if let Some(id) = filter(&query.user_id) {
            condition = condition.add(C::UserId.eq(id));
        }
        if let Some(id) = filter(&query.api_key_id) {
            condition = condition.add(C::ApiKeyId.eq(id));
        }
        let mut upstream_filter = Condition::all();
        if let Some(id) = filter(&query.provider_id) {
            upstream_filter = upstream_filter.add(upstream_record::Column::ProviderId.eq(id));
        }
        if let Some(id) = filter(&query.credential_id) {
            upstream_filter = upstream_filter.add(upstream_record::Column::CredentialId.eq(id));
        }
        if filter(&query.provider_id).is_some() || filter(&query.credential_id).is_some() {
            if $upstream {
                condition = condition.add(upstream_filter);
            } else {
                let upstream_ids = upstream_record::Entity::find()
                    .select_only()
                    .column(upstream_record::Column::Id)
                    .filter(upstream_filter)
                    .into_query();
                let downstream_ids = capture_link::Entity::find()
                    .select_only()
                    .column(capture_link::Column::DownstreamId)
                    .filter(capture_link::Column::UpstreamId.in_subquery(upstream_ids))
                    .into_query();
                let mut direct = Condition::all();
                if let Some(id) = filter(&query.provider_id) {
                    direct = direct.add(C::ProviderId.eq(id));
                }
                if let Some(id) = filter(&query.credential_id) {
                    direct = direct.add(C::CredentialId.eq(id));
                }
                condition = condition.add(
                    Condition::any()
                        .add(direct)
                        .add(C::Id.in_subquery(downstream_ids)),
                );
            }
        }
        if let Some(model) = filter(&query.model) {
            condition = condition.add(C::Model.eq(model));
        }
        if let Some(operation) = filter(&query.operation) {
            condition = condition.add(C::Operation.eq(operation));
        }
        if let Some(reason) = filter(&query.reason) {
            condition = condition.add(C::Reason.eq(reason));
        }
        if let Some(status) = query.status {
            condition = condition.add(C::ResponseStatus.eq(status));
        }
        if let Some(id) = filter(&query.request_id) {
            let mut matching = Condition::any().add(C::Id.eq(id));
            if $upstream {
                let ids = capture_link::Entity::find()
                    .select_only()
                    .column(capture_link::Column::UpstreamId)
                    .filter(capture_link::Column::DownstreamId.eq(id))
                    .into_query();
                matching = matching.add(C::Id.in_subquery(ids));
            }
            condition = condition.add(matching);
        }
        if let Some(cursor) = query.cursor {
            condition = condition.add(match filter(&query.cursor_id) {
                Some(id) => Condition::any().add(C::StartedAtMs.lt(cursor)).add(
                    Condition::all()
                        .add(C::StartedAtMs.eq(cursor))
                        .add(C::Id.lt(id)),
                ),
                None => Condition::all().add(C::StartedAtMs.lt(cursor)),
            });
        }
        let limit = query.limit.unwrap_or(50).clamp(1, 500);
        let mut rows = $self
            .inner
            .store
            .$repository()
            .query(
                $record::Entity::find()
                    .filter(condition)
                    .order_by_desc(C::StartedAtMs)
                    .order_by_desc(C::Id)
                    .limit(limit + 1),
            )
            .await?;
        let more = rows.len() as u64 > limit;
        rows.truncate(limit as usize);
        let next = more
            .then(|| rows.last())
            .flatten()
            .map(|row| (row.started_at_ms, row.id.clone()));
        Ok(LogPageDto {
            items: rows
                .into_iter()
                .map(capture_record::Model::from)
                .map(LogEntryDto::from)
                .collect(),
            next_cursor: next.as_ref().map(|(time, _)| *time),
            next_cursor_id: next.map(|(_, id)| id),
        })
    }};
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Logs<'_, C> {
    pub async fn list(&self, query: LogQuery) -> SdkResult<LogPageDto> {
        list_records!(self, query, downstream_record, downstream_records, false)
    }
    pub async fn upstream(&self, query: LogQuery) -> SdkResult<LogPageDto> {
        list_records!(self, query, upstream_record, upstream_records, true)
    }
    pub async fn capture(&self, capture_id: &str) -> SdkResult<CaptureDetailDto> {
        let store = &self.inner.store;
        let ids = [capture_id.to_owned()];
        let (row, events): (capture_record::Model, Vec<capture_event::Model>) = if let Some(row) =
            store
                .upstream_records()
                .get_many(&ids)
                .await?
                .into_iter()
                .next()
                .flatten()
        {
            let events = store
                .upstream_events()
                .query(
                    upstream_event::Entity::find()
                        .filter(upstream_event::Column::CaptureId.eq(capture_id))
                        .order_by_asc(upstream_event::Column::Sequence)
                        .limit(MAX_DETAIL_EVENTS + 1),
                )
                .await?;
            (row.into(), events.into_iter().map(Into::into).collect())
        } else {
            let row = store
                .downstream_records()
                .get_many(&ids)
                .await?
                .into_iter()
                .next()
                .flatten()
                .ok_or_else(|| SdkError::not_found("capture record", capture_id))?;
            let events = store
                .downstream_events()
                .query(
                    downstream_event::Entity::find()
                        .filter(downstream_event::Column::CaptureId.eq(capture_id))
                        .order_by_asc(downstream_event::Column::Sequence)
                        .limit(MAX_DETAIL_EVENTS + 1),
                )
                .await?;
            (row.into(), events.into_iter().map(Into::into).collect())
        };
        let events_truncated = events.len() as u64 > MAX_DETAIL_EVENTS;
        Ok(CaptureDetailDto {
            record: record(row),
            events: events
                .into_iter()
                .take(MAX_DETAIL_EVENTS as usize)
                .map(event)
                .collect(),
            events_truncated,
        })
    }
    pub async fn detail(&self, request_id: &str) -> SdkResult<LogDetailDto> {
        let store = &self.inner.store;
        let downstream = store
            .downstream_records()
            .get_many(&[request_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| SdkError::not_found("downstream record", request_id))?;
        let ids = capture_link::Entity::find()
            .select_only()
            .column(capture_link::Column::UpstreamId)
            .filter(capture_link::Column::DownstreamId.eq(request_id))
            .into_query();
        let upstream = store
            .upstream_records()
            .query(
                upstream_record::Entity::find()
                    .filter(upstream_record::Column::Id.in_subquery(ids.clone()))
                    .order_by_asc(upstream_record::Column::StartedAtMs)
                    .order_by_asc(upstream_record::Column::AttemptOrdinal)
                    .order_by_asc(upstream_record::Column::Id),
            )
            .await?;
        let mut events: Vec<capture_event::Model> = store
            .downstream_events()
            .query(
                downstream_event::Entity::find()
                    .filter(downstream_event::Column::CaptureId.eq(request_id))
                    .order_by_asc(downstream_event::Column::Sequence)
                    .limit(MAX_DETAIL_EVENTS + 1),
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        events.extend(
            store
                .upstream_events()
                .query(
                    upstream_event::Entity::find()
                        .filter(upstream_event::Column::CaptureId.in_subquery(ids.clone()))
                        .order_by_asc(upstream_event::Column::CaptureId)
                        .order_by_asc(upstream_event::Column::Sequence)
                        .limit(MAX_DETAIL_EVENTS + 1),
                )
                .await?
                .into_iter()
                .map(capture_event::Model::from),
        );
        events.sort_by(|a, b| (&a.capture_id, a.sequence).cmp(&(&b.capture_id, b.sequence)));
        let events_truncated = events.len() as u64 > MAX_DETAIL_EVENTS;
        events.truncate(MAX_DETAIL_EVENTS as usize);
        let usage = store
            .usage_records()
            .query(
                usage_record::Entity::find()
                    .filter(usage_record::Column::RequestId.in_subquery(ids))
                    .order_by_asc(usage_record::Column::StartedAtMs)
                    .order_by_asc(usage_record::Column::RequestId),
            )
            .await?
            .into_iter()
            .map(UsageRecordDto::from)
            .collect();
        Ok(LogDetailDto {
            downstream: record(downstream.into()),
            upstream: upstream.into_iter().map(|row| record(row.into())).collect(),
            events: events.into_iter().map(event).collect(),
            events_truncated,
            usage,
        })
    }
}

/// A captured exchange with all stored body bytes.
fn record(row: capture_record::Model) -> CaptureRecordDto {
    CaptureRecordDto {
        request_body: body(row.request_body_state, row.request_body.as_deref()),
        response_body: body(row.response_body_state, row.response_body.as_deref()),
        id: row.id,
        initiator_request_id: row.initiator_request_id,
        attempt_id: row.attempt_id,
        attempt_ordinal: row.attempt_ordinal,
        side: row.side.to_value(),
        kind: row.kind.to_value(),
        session_id: row.session_id,
        stream_key: row.stream_key,
        user_id: row.user_id,
        api_key_id: row.api_key_id,
        provider_id: row.provider_id,
        credential_id: row.credential_id,
        agent_assignment_id: row.agent_assignment_id,
        model: row.model,
        operation: row.operation,
        request_method: row.request_method,
        request_url: row.request_url,
        request_query: row.request_query,
        request_headers: row.request_headers,
        request_framing: row.request_framing.to_value(),
        response_status: row.response_status,
        response_headers: row.response_headers,
        response_framing: row.response_framing.to_value(),
        client_ip: row.client_ip,
        metrics: row.metrics,
        state: row.state.to_value(),
        error: row.error,
        reason: row.reason,
        started_at_ms: row.started_at_ms,
        first_response_at_ms: row.first_response_at_ms,
        ended_at_ms: row.ended_at_ms,
    }
}

/// One body, respecting what the record says about it.
///
/// `NotCaptured` never produces content, whatever is in the column: a body
/// nobody stored must not come back looking like a body that was empty. The
/// state travels with every body so the two cases stay distinguishable, and so
/// does a body that is only in the events because its framing was not
/// `buffered`.
fn body(state: capture_record::CaptureBodyState, bytes: Option<&[u8]>) -> LogBodyDto {
    let name = state.to_value();
    match bytes {
        Some(bytes) if state != capture_record::CaptureBodyState::NotCaptured => {
            LogBodyDto::new(name, bytes)
        }
        _ => LogBodyDto::absent(name),
    }
}

/// An event's payload is always exactly the bytes that were stored for it, so
/// its state is `complete` by construction and all stored bytes are returned.
fn event(row: capture_event::Model) -> CaptureEventDto {
    CaptureEventDto {
        payload: LogBodyDto::new("complete".to_owned(), &row.payload),
        capture_id: row.capture_id,
        sequence: row.sequence,
        turn_id: row.turn_id,
        direction: row.direction.to_value(),
        kind: row.kind.to_value(),
        observed_at_ms: row.observed_at_ms,
    }
}
