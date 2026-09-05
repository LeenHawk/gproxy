use crate::dto::{UsageRecordDto, UsageRecordPageDto, UsageRecordQueryDto, UsageSummaryDto};
use crate::{AdminError, State, response};
use bytes::Bytes;
use gproxy_store::records::UsageFilter;
use http::{Response, StatusCode, request::Parts};

fn query(parts: &Parts) -> Result<(UsageRecordQueryDto, UsageFilter), AdminError> {
    let query: UsageRecordQueryDto =
        serde_urlencoded::from_str(parts.uri.query().unwrap_or_default())
            .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    let (from, to) = super::range(query.from, query.to)?;
    if query
        .ended
        .as_deref()
        .is_some_and(|value| !matches!(value, "complete" | "interrupted"))
        || query
            .usage_source
            .as_deref()
            .is_some_and(|value| !matches!(value, "upstream" | "estimated"))
    {
        return Err(AdminError::BadRequest(
            "invalid usage source or ended filter".into(),
        ));
    }
    let filter = UsageFilter {
        from,
        to,
        user_key_id: query.user_key_id,
        user_id: query.user_id,
        provider_id: query.provider_id,
        credential_id: query.credential_id,
        model: query.model.clone(),
        request_id: query.request_id.clone(),
        operation: query.operation.clone(),
        usage_source: query.usage_source.clone(),
        ended: query.ended.clone(),
    };
    Ok((query, filter))
}

pub(in crate::handlers) async fn records(
    state: &impl State,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let (query, filter) = query(parts)?;
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(10);
    if page == 0
        || !matches!(page_size, 10 | 20 | 50 | 100)
        || page.checked_mul(page_size).is_none()
    {
        return Err(AdminError::BadRequest("invalid page or page_size".into()));
    }
    let (records, total) = state
        .store()
        .usage_records(&filter, page, page_size)
        .await?;
    let items = records
        .into_iter()
        .map(|record| {
            let value = record.usage;
            UsageRecordDto {
                id: record.id,
                request_id: value.request_id,
                at: value.at,
                provider_id: value.provider_id,
                credential_id: value.credential_id,
                user_id: value.user_id,
                user_key_id: value.user_key_id,
                operation: value.operation,
                model: value.upstream_model,
                input_tokens: value.input_tokens,
                output_tokens: value.output_tokens,
                cached_input_tokens: value.cached_input_tokens,
                metrics: value.metrics,
                dimensions: value.dimensions,
                cost: value.cost.normalize().to_string(),
                usage_source: value.usage_source,
                ended: value.ended,
                latency_ms: value.latency_ms,
            }
        })
        .collect();
    response::json(
        StatusCode::OK,
        &UsageRecordPageDto {
            items,
            total,
            page,
            page_size,
        },
    )
}

pub(in crate::handlers) async fn summary(
    state: &impl State,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let (_, filter) = query(parts)?;
    let totals: UsageSummaryDto = state.store().usage_summary(&filter).await?.into();
    response::json(StatusCode::OK, &totals)
}
