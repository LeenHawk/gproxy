mod cleanup;
mod cycle;
mod health;
mod quota;

pub use cleanup::CleanupResult;

use crate::backend::Row;
use crate::query::runtime;
use crate::records::{
    CaptureInput, LogDetail, LogPage, LogQuery, RequestLogCompletion, RequestLogInput,
    RequestLogRecord, RequestLogSummary, WireLogRecord,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn begin_request_log(&self, input: &RequestLogInput) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::begin_request_log(input)?)
            .await?;
        Ok(())
    }

    pub async fn record_capture(&self, input: &CaptureInput) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::insert_capture(input)?)
            .await?;
        Ok(())
    }

    pub async fn finish_request_log(&self, input: &RequestLogCompletion) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::finish_request_log(input)?)
            .await?;
        Ok(())
    }

    pub async fn update_request_log_response(
        &self,
        request_id: &str,
        response_body: Vec<u8>,
    ) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::update_request_log_response(
                request_id,
                response_body,
            )?)
            .await?;
        Ok(())
    }

    pub async fn list_logs(&self, input: &LogQuery) -> Result<LogPage, StoreError> {
        let mut items = self
            .backend()
            .execute(runtime::list_logs(input)?)
            .await?
            .rows
            .into_iter()
            .map(parse_summary)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor =
            (items.len() > input.limit as usize).then(|| items[input.limit as usize - 1].id);
        items.truncate(input.limit as usize);
        Ok(LogPage { items, next_cursor })
    }

    pub async fn log_detail(&self, request_id: &str) -> Result<Option<LogDetail>, StoreError> {
        let usage = self.usage_by_request(request_id).await?;
        let request_id = usage
            .as_ref()
            .and_then(|row| row.usage.dimensions.get("parent_request_id"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(request_id);
        let mut results = self
            .backend()
            .batch(vec![
                runtime::request_log(request_id)?,
                runtime::wire_logs(request_id)?,
            ])
            .await?;
        let wires = results
            .pop()
            .expect("wire query result")
            .rows
            .into_iter()
            .map(parse_wire)
            .collect::<Result<Vec<_>, _>>()?;
        let request = results
            .pop()
            .expect("request query result")
            .rows
            .into_iter()
            .next()
            .map(parse_request)
            .transpose()?;
        Ok(request.map(|downstream| LogDetail {
            downstream,
            upstream: wires,
        }))
    }
}

fn parse_summary(row: Row) -> Result<RequestLogSummary, StoreError> {
    Ok(RequestLogSummary {
        id: row.i64("id")?,
        request_id: row.text("request_id")?.to_owned(),
        at: row.i64("at")?,
        method: row.text("method")?.to_owned(),
        path: row.text("path")?.to_owned(),
        response_status: optional_status(&row)?,
        error_kind: row.optional_text("error_kind")?.map(str::to_owned),
        client_ip: row.optional_text("client_ip")?.map(str::to_owned),
        duration_ms: optional_unsigned(&row, "duration_ms")?,
        output_tokens: optional_unsigned(&row, "output_tokens")?,
    })
}

fn parse_request(row: Row) -> Result<RequestLogRecord, StoreError> {
    Ok(RequestLogRecord {
        id: row.i64("id")?,
        input: RequestLogInput {
            request_id: row.text("request_id")?.to_owned(),
            at: row.i64("at")?,
            method: row.text("method")?.to_owned(),
            path: row.text("path")?.to_owned(),
            query: row.optional_text("query")?.map(str::to_owned),
            client_ip: row.optional_text("client_ip")?.map(str::to_owned),
            request_headers: optional_json(&row, "request_headers")?,
            request_body: row.optional_blob("request_body")?.map(<[u8]>::to_vec),
        },
        response_status: optional_status(&row)?,
        error_kind: row.optional_text("error_kind")?.map(str::to_owned),
        response_headers: optional_json(&row, "response_headers")?,
        response_body: row.optional_blob("response_body")?.map(<[u8]>::to_vec),
        duration_ms: optional_unsigned(&row, "duration_ms")?,
        output_tokens: optional_unsigned(&row, "output_tokens")?,
    })
}

fn parse_wire(row: Row) -> Result<WireLogRecord, StoreError> {
    Ok(WireLogRecord {
        id: row.i64("id")?,
        input: CaptureInput {
            request_id: row.text("request_id")?.to_owned(),
            at: row.i64("at")?,
            provider_id: row.optional_i64("provider_id")?,
            credential_id: row.optional_i64("credential_id")?,
            upstream_url: row.optional_text("upstream_url")?.map(str::to_owned),
            request_method: row.optional_text("request_method")?.map(str::to_owned),
            request_headers: optional_json(&row, "request_headers")?,
            response_status: optional_status(&row)?,
            response_headers: optional_json(&row, "response_headers")?,
            request_body: row.optional_blob("request_body")?.map(<[u8]>::to_vec),
            response_body: row.optional_blob("response_body")?.map(<[u8]>::to_vec),
        },
    })
}

fn optional_json(row: &Row, field: &'static str) -> Result<Option<serde_json::Value>, StoreError> {
    row.optional_text(field)?
        .map(|value| {
            serde_json::from_str(value).map_err(|error| StoreError::InvalidData {
                field,
                message: error.to_string(),
            })
        })
        .transpose()
}

fn optional_status(row: &Row) -> Result<Option<u16>, StoreError> {
    row.optional_i64("response_status")?
        .map(|value| {
            u16::try_from(value).map_err(|error| StoreError::InvalidData {
                field: "response_status",
                message: error.to_string(),
            })
        })
        .transpose()
}

fn optional_unsigned(row: &Row, field: &'static str) -> Result<Option<u64>, StoreError> {
    row.optional_i64(field)?
        .map(|value| super::usage::unsigned(value, field))
        .transpose()
}
