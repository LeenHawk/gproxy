use bytes::BytesMut;
use tokio_postgres::types::{FromSql, IsNull, ToSql, Type, to_sql_checked};
use tokio_postgres::{GenericClient, NoTls};

use super::{DbValue, Executor, QueryResult, Row, Statement};
use crate::StoreError;
use crate::schema::Dialect;

pub(super) struct Postgres {
    client: tokio::sync::Mutex<tokio_postgres::Client>,
}

impl Postgres {
    pub(super) async fn connect(dsn: &str) -> Result<Self, StoreError> {
        let (client, connection) = tokio_postgres::connect(dsn, NoTls)
            .await
            .map_err(|_| StoreError::Database("PostgreSQL connection failed".into()))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(Self {
            client: tokio::sync::Mutex::new(client),
        })
    }
}

impl Executor for Postgres {
    fn execute<'a>(&'a self, statement: Statement) -> super::DbFuture<'a, QueryResult> {
        Box::pin(async move { run(&*self.client.lock().await, statement, None).await })
    }

    fn batch<'a>(&'a self, statements: Vec<Statement>) -> super::DbFuture<'a, Vec<QueryResult>> {
        Box::pin(async move {
            let mut client = self.client.lock().await;
            let transaction = client.transaction().await.map_err(database_error)?;
            let mut results = Vec::with_capacity(statements.len());
            for statement in statements {
                let changes = results
                    .last()
                    .map(|result: &QueryResult| result.affected_rows);
                results.push(run(&transaction, statement, changes).await?);
            }
            transaction.commit().await.map_err(database_error)?;
            Ok(results)
        })
    }
}

async fn run(
    client: &impl GenericClient,
    statement: Statement,
    changes: Option<u64>,
) -> Result<QueryResult, StoreError> {
    let sql = replace_changes(statement.sql_for(Dialect::Postgres), changes);
    let values = statement
        .args
        .into_iter()
        .map(PgValue::from)
        .collect::<Vec<_>>();
    let parameters = values
        .iter()
        .map(|value| value as &(dyn ToSql + Sync))
        .collect::<Vec<_>>();
    let prepared = client.prepare(&sql).await.map_err(database_error)?;
    if prepared.columns().is_empty() {
        let affected_rows = client
            .execute(&prepared, &parameters)
            .await
            .map_err(|error| query_error(error, &prepared, &values, &sql))?;
        return Ok(QueryResult {
            rows: Vec::new(),
            affected_rows,
            last_insert_id: None,
        });
    }
    let selected = client
        .query(&prepared, &parameters)
        .await
        .map_err(|error| query_error(error, &prepared, &values, &sql))?;
    let writes = !sql.trim_start().to_ascii_uppercase().starts_with("SELECT");
    let last_insert_id = selected.first().and_then(|row| {
        row.columns()
            .iter()
            .position(|column| column.name() == "id")
            .and_then(|index| row.try_get::<_, i64>(index).ok())
    });
    let rows: Vec<Row> = selected
        .into_iter()
        .map(decode_row)
        .collect::<Result<_, _>>()?;
    Ok(QueryResult {
        affected_rows: if writes { rows.len() as u64 } else { 0 },
        rows,
        last_insert_id,
    })
}

fn replace_changes(sql: &str, changes: Option<u64>) -> String {
    let value = changes.unwrap_or_default().to_string();
    sql.replace("\"changes\"()", &value)
        .replace("changes()", &value)
}

#[derive(Debug)]
enum PgValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl From<DbValue> for PgValue {
    fn from(value: DbValue) -> Self {
        match value {
            DbValue::Null => Self::Null,
            DbValue::Integer(value) => Self::Integer(value),
            DbValue::Real(value) => Self::Real(value),
            DbValue::Text(value) => Self::Text(value),
            DbValue::Blob(value) => Self::Blob(value),
        }
    }
}

impl ToSql for PgValue {
    fn to_sql(
        &self,
        ty: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            Self::Null => Ok(IsNull::Yes),
            Self::Integer(value) if *ty == Type::INT4 => i32::try_from(*value)?.to_sql(ty, output),
            Self::Integer(value) if matches!(*ty, Type::TEXT | Type::VARCHAR) => {
                value.to_string().to_sql(ty, output)
            }
            Self::Integer(value) => value.to_sql(ty, output),
            Self::Real(value) if *ty == Type::FLOAT4 => (*value as f32).to_sql(ty, output),
            Self::Real(value) if matches!(*ty, Type::TEXT | Type::VARCHAR) => {
                value.to_string().to_sql(ty, output)
            }
            Self::Real(value) => value.to_sql(ty, output),
            Self::Text(value) => value.to_sql(ty, output),
            Self::Blob(value) => value.to_sql(ty, output),
        }
    }
    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::INT4
                | Type::INT8
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::TEXT
                | Type::VARCHAR
                | Type::BYTEA
        )
    }
    to_sql_checked!();
}

fn decode_row(row: tokio_postgres::Row) -> Result<Row, StoreError> {
    let values = row
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let value = match *column.type_() {
                Type::INT8 => optional::<i64>(&row, index, DbValue::Integer)?,
                Type::INT4 => {
                    optional::<i32>(&row, index, |value| DbValue::Integer(i64::from(value)))?
                }
                Type::FLOAT8 => optional::<f64>(&row, index, DbValue::Real)?,
                Type::FLOAT4 => {
                    optional::<f32>(&row, index, |value| DbValue::Real(f64::from(value)))?
                }
                Type::BYTEA => optional::<Vec<u8>>(&row, index, DbValue::Blob)?,
                Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => {
                    optional::<String>(&row, index, DbValue::Text)?
                }
                _ => {
                    return Err(StoreError::Database(format!(
                        "unsupported PostgreSQL result type {}",
                        column.type_()
                    )));
                }
            };
            Ok((column.name().to_owned(), value))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    Ok(Row::new(values))
}

fn optional<'a, T: FromSql<'a>>(
    row: &'a tokio_postgres::Row,
    index: usize,
    map: impl FnOnce(T) -> DbValue,
) -> Result<DbValue, StoreError> {
    row.try_get::<_, Option<T>>(index)
        .map(|value| value.map_or(DbValue::Null, map))
        .map_err(database_error)
}

fn database_error(error: tokio_postgres::Error) -> StoreError {
    match error.as_db_error() {
        Some(database) => StoreError::Database(format!("PostgreSQL: {}", database.message())),
        None => StoreError::Database(format!("PostgreSQL: {error}")),
    }
}

fn query_error(
    error: tokio_postgres::Error,
    statement: &tokio_postgres::Statement,
    values: &[PgValue],
    sql: &str,
) -> StoreError {
    for (index, (value, ty)) in values.iter().zip(statement.params()).enumerate() {
        if value.to_sql_checked(ty, &mut BytesMut::new()).is_err() {
            return StoreError::Database(format!(
                "PostgreSQL cannot encode parameter {index} as {ty} for {}",
                sql.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
            ));
        }
    }
    database_error(error)
}
