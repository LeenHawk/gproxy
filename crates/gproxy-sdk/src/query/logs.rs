//! Captured requests: the downstream list, and one request in full.
//!
//! The list is cursor-paged. A request log is an append-only stream whose head
//! keeps moving while a person reads it, and an offset page over that either
//! repeats rows or drops them as new ones arrive. The cursor is the
//! `(started_at_ms, id)` of the last row returned — both halves, because two
//! requests can start in the same millisecond and a timestamp-only cursor
//! would then loop on them forever or skip past them.
//!
//! A detail is a tree: the downstream record, the upstream attempts reached
//! through `capture_links`, and the stream events of all of them. Bodies are
//! returned in full. Events are limited by [`MAX_DETAIL_EVENTS`], with the
//! event-list cut reported separately on the answer.
//!
//! Nothing here redacts. Core's observer applied the deployment's logging
//! redaction policy as it wrote these rows, so a host must not assume a second
//! pass happens on read — if a secret is in the database, it is because the
//! policy allowed it there.

use std::{collections::BTreeSet, sync::Arc};

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::usage::{capture_event, capture_link, capture_record};
use sea_orm::{
    ActiveEnum, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    QueryTrait,
};

use super::{MAX_DETAIL_EVENTS, filter};
use crate::{
    SdkError, SdkResult,
    dto::{
        CaptureDetailDto, CaptureEventDto, CaptureRecordDto, LogBodyDto, LogDetailDto, LogEntryDto,
        LogPageDto, LogQuery, UsageRecordDto,
    },
    handle::Inner,
};

pub struct Logs<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

impl<'a, C> Logs<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Logs<'_, C> {
    /// One page of downstream requests, newest first.
    ///
    /// Only `side = downstream` rows are listed: an upstream attempt is not a
    /// request, it is something a request did, and it is reached through
    /// [`detail`](Self::detail). Hand `next_cursor` and `next_cursor_id` back
    /// unchanged for the next page; a `cursor` without its `cursor_id` still
    /// works but may repeat the rows of its own millisecond.
    pub async fn list(&self, query: LogQuery) -> SdkResult<LogPageDto> {
        self.list_side(query, capture_record::CaptureSide::Downstream)
            .await
    }

    /// Physical upstream exchanges, including attempts without a retained downstream row.
    pub async fn upstream(&self, query: LogQuery) -> SdkResult<LogPageDto> {
        self.list_side(query, capture_record::CaptureSide::Upstream)
            .await
    }

    async fn list_side(
        &self,
        query: LogQuery,
        side: capture_record::CaptureSide,
    ) -> SdkResult<LogPageDto> {
        use capture_record::Column as C;
        let mut condition = Condition::all().add(C::Side.eq(side));
        if let Some(from_ms) = query.from_ms {
            condition = condition.add(C::StartedAtMs.gte(from_ms));
        }
        if let Some(to_ms) = query.to_ms {
            condition = condition.add(C::StartedAtMs.lt(to_ms));
        }
        if let Some(user_id) = filter(&query.user_id) {
            condition = condition.add(C::UserId.eq(user_id));
        }
        if let Some(api_key_id) = filter(&query.api_key_id) {
            condition = condition.add(C::ApiKeyId.eq(api_key_id));
        }
        let mut upstream_filter = Condition::all();
        let provider = filter(&query.provider_id);
        let credential = filter(&query.credential_id);
        if let Some(provider_id) = provider {
            upstream_filter = upstream_filter.add(C::ProviderId.eq(provider_id));
        }
        if let Some(credential_id) = credential {
            upstream_filter = upstream_filter.add(C::CredentialId.eq(credential_id));
        }
        if provider.is_some() || credential.is_some() {
            if side == capture_record::CaptureSide::Downstream {
                let upstream_ids = capture_record::Entity::find()
                    .select_only()
                    .column(C::Id)
                    .filter(C::Side.eq(capture_record::CaptureSide::Upstream))
                    .filter(upstream_filter.clone())
                    .into_query();
                let downstream_ids = capture_link::Entity::find()
                    .select_only()
                    .column(capture_link::Column::DownstreamId)
                    .filter(capture_link::Column::UpstreamId.in_subquery(upstream_ids))
                    .into_query();
                condition = condition.add(
                    Condition::any()
                        .add(upstream_filter)
                        .add(C::Id.in_subquery(downstream_ids)),
                );
            } else {
                condition = condition.add(upstream_filter);
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
        // A downstream record's id is the request id, so there is nothing to
        // look up in `initiator_request_id` here.
        if let Some(request_id) = filter(&query.request_id) {
            condition = condition.add(
                Condition::any()
                    .add(C::Id.eq(request_id))
                    .add(C::InitiatorRequestId.eq(request_id)),
            );
        }
        if let Some(cursor) = query.cursor {
            condition = condition.add(match filter(&query.cursor_id) {
                Some(cursor_id) => Condition::any().add(C::StartedAtMs.lt(cursor)).add(
                    Condition::all()
                        .add(C::StartedAtMs.eq(cursor))
                        .add(C::Id.lt(cursor_id)),
                ),
                None => Condition::all().add(C::StartedAtMs.lt(cursor)),
            });
        }

        let limit = query.limit.unwrap_or(50).clamp(1, 500);
        // One row past the page, so "there is more" is a fact rather than a
        // guess from a full page.
        let mut rows = self
            .inner
            .store
            .capture_records()
            .query(
                capture_record::Entity::find()
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
            items: rows.into_iter().map(LogEntryDto::from).collect(),
            next_cursor: next.as_ref().map(|(started_at_ms, _)| *started_at_ms),
            next_cursor_id: next.map(|(_, id)| id),
        })
    }

    /// One physical exchange and its events, regardless of downstream retention.
    pub async fn capture(&self, capture_id: &str) -> SdkResult<CaptureDetailDto> {
        let store = &self.inner.store;
        let row = store
            .capture_records()
            .get_many(&[capture_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| SdkError::not_found("capture record", capture_id))?;
        let mut events = store
            .capture_events()
            .query(
                capture_event::Entity::find()
                    .filter(capture_event::Column::CaptureId.eq(capture_id))
                    .order_by_asc(capture_event::Column::Sequence)
                    .limit(MAX_DETAIL_EVENTS + 1),
            )
            .await?;
        let events_truncated = events.len() as u64 > MAX_DETAIL_EVENTS;
        events.truncate(MAX_DETAIL_EVENTS as usize);
        Ok(CaptureDetailDto {
            record: record(row),
            events: events.into_iter().map(event).collect(),
            events_truncated,
        })
    }

    /// One request and everything captured under it: the downstream record,
    /// every upstream attempt linked to it, their stream events, and the
    /// settled usage row when there is one.
    pub async fn detail(&self, request_id: &str) -> SdkResult<LogDetailDto> {
        let store = &self.inner.store;
        let downstream = store
            .capture_records()
            .get_many(&[request_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .filter(|row| row.side == capture_record::CaptureSide::Downstream)
            .ok_or_else(|| SdkError::not_found("capture record", request_id))?;

        // Links, not `initiator_request_id`: a link is the causal edge, and it
        // is what represents an upstream call shared by two downstream ones.
        let links = store
            .capture_links()
            .query(
                capture_link::Entity::find()
                    .filter(capture_link::Column::DownstreamId.eq(request_id))
                    .order_by_asc(capture_link::Column::Sequence)
                    .order_by_asc(capture_link::Column::UpstreamId),
            )
            .await?;
        let upstream_ids: Vec<String> = links.into_iter().map(|link| link.upstream_id).collect();
        let upstream: Vec<capture_record::Model> = store
            .capture_records()
            .get_many(&upstream_ids)
            .await?
            .into_iter()
            .flatten()
            .collect();

        let capture_ids: Vec<String> = std::iter::once(downstream.id.clone())
            .chain(upstream.iter().map(|row| row.id.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut events = store
            .capture_events()
            .query(
                capture_event::Entity::find()
                    .filter(capture_event::Column::CaptureId.is_in(capture_ids))
                    .order_by_asc(capture_event::Column::CaptureId)
                    .order_by_asc(capture_event::Column::Sequence)
                    .limit(MAX_DETAIL_EVENTS + 1),
            )
            .await?;
        let events_truncated = events.len() as u64 > MAX_DETAIL_EVENTS;
        events.truncate(MAX_DETAIL_EVENTS as usize);

        let usage = store
            .usage_records()
            .get_many(&[request_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .map(UsageRecordDto::from);

        Ok(LogDetailDto {
            downstream: record(downstream),
            upstream: upstream.into_iter().map(record).collect(),
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
