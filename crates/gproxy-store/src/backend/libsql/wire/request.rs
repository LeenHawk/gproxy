use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Serialize;

use super::WireValue;
use crate::StoreError;
use crate::backend::{DbValue, Statement};

#[derive(Serialize)]
struct Pipeline {
    baton: Option<String>,
    requests: Vec<PipelineRequest>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum PipelineRequest {
    Execute { stmt: WireStatement },
    Batch { batch: Batch },
    Close,
}

#[derive(Serialize)]
struct WireStatement {
    sql: String,
    args: Vec<WireValue>,
}

#[derive(Serialize)]
struct Batch {
    steps: Vec<BatchStep>,
}

#[derive(Serialize)]
struct BatchStep {
    stmt: WireStatement,
}

pub(in crate::backend::libsql) fn encode_execute(
    statement: Statement,
) -> Result<Vec<u8>, StoreError> {
    encode(PipelineRequest::Execute {
        stmt: encode_statement(statement)?,
    })
}

pub(in crate::backend::libsql) fn encode_batch(
    statements: Vec<Statement>,
) -> Result<Vec<u8>, StoreError> {
    let statements = std::iter::once(Statement::plain("BEGIN"))
        .chain(statements)
        .chain(std::iter::once(Statement::plain("END")));
    let steps = statements
        .map(|statement| {
            Ok(BatchStep {
                stmt: encode_statement(statement)?,
            })
        })
        .collect::<Result<_, StoreError>>()?;
    encode(PipelineRequest::Batch {
        batch: Batch { steps },
    })
}

fn encode(request: PipelineRequest) -> Result<Vec<u8>, StoreError> {
    serde_json::to_vec(&Pipeline {
        baton: None,
        requests: vec![request, PipelineRequest::Close],
    })
    .map_err(|error| StoreError::Database(format!("encode Hrana request: {error}")))
}

fn encode_statement(statement: Statement) -> Result<WireStatement, StoreError> {
    Ok(WireStatement {
        sql: statement.sql,
        args: statement
            .args
            .into_iter()
            .map(encode_value)
            .collect::<Result<_, _>>()?,
    })
}

fn encode_value(value: DbValue) -> Result<WireValue, StoreError> {
    Ok(match value {
        DbValue::Null => WireValue::Null,
        DbValue::Integer(value) => WireValue::Integer {
            value: value.to_string(),
        },
        DbValue::Real(value) if value.is_finite() => WireValue::Float { value },
        DbValue::Real(_) => return Err(StoreError::Database("non-finite SQLite value".into())),
        DbValue::Text(value) => WireValue::Text { value },
        DbValue::Blob(value) => WireValue::Blob {
            base64: BASE64.encode(value),
        },
    })
}
