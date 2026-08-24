use serde::Deserialize;

use super::WireValue;
use super::value::decode_row;
use crate::StoreError;
use crate::backend::QueryResult;
use crate::backend::libsql::invalid;

#[derive(Deserialize)]
struct PipelineResponse {
    results: Vec<PipelineResult>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum PipelineResult {
    Ok { response: Response },
    Error { error: HranaError },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Response {
    Execute { result: ExecuteResult },
    Batch { result: BatchResult },
    Close,
}

#[derive(Deserialize)]
struct ExecuteResult {
    cols: Vec<Column>,
    rows: Vec<Vec<WireValue>>,
    affected_row_count: u64,
    last_insert_rowid: Option<String>,
}

#[derive(Deserialize)]
struct BatchResult {
    step_results: Vec<Option<ExecuteResult>>,
    step_errors: Vec<Option<HranaError>>,
}

#[derive(Deserialize)]
struct Column {
    name: Option<String>,
}

#[derive(Deserialize)]
struct HranaError {
    message: String,
}

pub(in crate::backend::libsql) fn decode_execute(bytes: &[u8]) -> Result<QueryResult, StoreError> {
    let mut results = pipeline_results(bytes)?;
    require_pipeline_pair(&results)?;
    let executed = match results.remove(0) {
        PipelineResult::Error { error } => return Err(hrana_error(error)),
        PipelineResult::Ok {
            response: Response::Execute { result },
        } => result.into_query_result()?,
        PipelineResult::Ok { .. } => return Err(invalid("expected execute response")),
    };
    decode_close(results.pop())?;
    Ok(executed)
}

pub(in crate::backend::libsql) fn decode_batch(
    bytes: &[u8],
    expected: usize,
) -> Result<Vec<QueryResult>, StoreError> {
    let mut results = pipeline_results(bytes)?;
    require_pipeline_pair(&results)?;
    let batch = match results.remove(0) {
        PipelineResult::Error { error } => return Err(hrana_error(error)),
        PipelineResult::Ok {
            response: Response::Batch { result },
        } => result,
        PipelineResult::Ok { .. } => return Err(invalid("expected batch response")),
    };
    decode_close(results.pop())?;
    batch.into_query_results(expected)
}

fn pipeline_results(bytes: &[u8]) -> Result<Vec<PipelineResult>, StoreError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| invalid(format!("invalid Hrana JSON: {error}")))?;
    if value.get("type").and_then(serde_json::Value::as_str) == Some("error") {
        let message = value
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown top-level Hrana error");
        return Err(StoreError::Database(format!("libSQL: {message}")));
    }
    serde_json::from_value::<PipelineResponse>(value)
        .map(|response| response.results)
        .map_err(|error| invalid(error.to_string()))
}

fn require_pipeline_pair(results: &[PipelineResult]) -> Result<(), StoreError> {
    if results.len() == 2 {
        Ok(())
    } else {
        Err(invalid(format!(
            "expected 2 pipeline results, received {}",
            results.len()
        )))
    }
}

fn decode_close(result: Option<PipelineResult>) -> Result<(), StoreError> {
    match result {
        Some(PipelineResult::Ok {
            response: Response::Close,
        }) => Ok(()),
        Some(PipelineResult::Error { error }) => Err(hrana_error(error)),
        Some(PipelineResult::Ok { .. }) => Err(invalid("pipeline did not end with close")),
        None => Err(invalid("pipeline close result missing")),
    }
}

impl BatchResult {
    fn into_query_results(self, expected: usize) -> Result<Vec<QueryResult>, StoreError> {
        let step_count = expected + 2;
        if self.step_results.len() != step_count || self.step_errors.len() != step_count {
            return Err(invalid("batch step count mismatch"));
        }
        if let Some(error) = self.step_errors.into_iter().flatten().next() {
            return Err(hrana_error(error));
        }
        let mut steps = self.step_results.into_iter();
        require_step(steps.next(), "BEGIN")?;
        let results = steps
            .by_ref()
            .take(expected)
            .map(|result| require_step(Some(result), "statement")?.into_query_result())
            .collect::<Result<Vec<_>, _>>()?;
        require_step(steps.next(), "END")?;
        Ok(results)
    }
}

fn require_step(
    result: Option<Option<ExecuteResult>>,
    step: &'static str,
) -> Result<ExecuteResult, StoreError> {
    result
        .flatten()
        .ok_or_else(|| invalid(format!("{step} batch result missing")))
}

impl ExecuteResult {
    fn into_query_result(self) -> Result<QueryResult, StoreError> {
        let names = self
            .cols
            .into_iter()
            .map(|column| column.name.ok_or_else(|| invalid("unnamed result column")))
            .collect::<Result<Vec<_>, _>>()?;
        let rows = self
            .rows
            .into_iter()
            .map(|values| decode_row(&names, values))
            .collect::<Result<_, _>>()?;
        let last_insert_id = self
            .last_insert_rowid
            .map(|value| {
                value
                    .parse()
                    .map_err(|error| invalid(format!("invalid last insert id: {error}")))
            })
            .transpose()?;
        Ok(QueryResult {
            rows,
            affected_rows: self.affected_row_count,
            last_insert_id,
        })
    }
}

fn hrana_error(error: HranaError) -> StoreError {
    StoreError::Database(format!("libSQL: {}", error.message))
}
