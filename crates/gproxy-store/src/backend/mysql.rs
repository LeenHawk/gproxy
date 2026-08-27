use mysql_async::prelude::Queryable;
use mysql_async::{Opts, Pool, TxOpts, Value};

use super::{DbValue, Executor, QueryResult, Row, Statement};
use crate::StoreError;
use crate::schema::Dialect;

pub(super) struct Mysql {
    pool: Pool,
}

impl Mysql {
    pub(super) fn connect(dsn: &str) -> Result<Self, StoreError> {
        let options = Opts::from_url(dsn)
            .map_err(|_| StoreError::Database("MySQL configuration failed".into()))?;
        Ok(Self {
            pool: Pool::new(options),
        })
    }
}

impl Executor for Mysql {
    fn execute<'a>(&'a self, statement: Statement) -> super::DbFuture<'a, QueryResult> {
        Box::pin(async move {
            let mut connection = self.pool.get_conn().await.map_err(connection_error)?;
            run(&mut connection, statement, None).await
        })
    }

    fn batch<'a>(&'a self, statements: Vec<Statement>) -> super::DbFuture<'a, Vec<QueryResult>> {
        Box::pin(async move {
            let mut connection = self.pool.get_conn().await.map_err(connection_error)?;
            let mut transaction = connection
                .start_transaction(TxOpts::default())
                .await
                .map_err(database_error)?;
            let mut results = Vec::with_capacity(statements.len());
            for statement in statements {
                let changes = results
                    .last()
                    .map(|result: &QueryResult| result.affected_rows);
                results.push(run(&mut transaction, statement, changes).await?);
            }
            transaction.commit().await.map_err(database_error)?;
            Ok(results)
        })
    }
}

async fn run(
    connection: &mut impl Queryable,
    statement: Statement,
    changes: Option<u64>,
) -> Result<QueryResult, StoreError> {
    let sql = replace_mysql_syntax(&replace_changes(statement.sql_for(Dialect::Mysql), changes));
    let parameters = statement.args.into_iter().map(to_value).collect::<Vec<_>>();
    let mut result = connection
        .exec_iter(&sql, parameters)
        .await
        .map_err(database_error)?;
    let affected_rows = result.affected_rows();
    let last_insert_id = result
        .last_insert_id()
        .and_then(|value| i64::try_from(value).ok());
    let columns = result.columns().unwrap_or_default();
    let rows = result
        .collect::<mysql_async::Row>()
        .await
        .map_err(database_error)?;
    let rows = rows
        .into_iter()
        .map(|row| decode_row(row, &columns))
        .collect::<Result<_, _>>()?;
    Ok(QueryResult {
        rows,
        affected_rows,
        last_insert_id,
    })
}

fn replace_changes(sql: &str, changes: Option<u64>) -> String {
    let value = changes.unwrap_or_default().to_string();
    sql.replace("`changes`()", &value)
        .replace("changes()", &value)
}

fn replace_mysql_syntax(sql: &str) -> String {
    let mut sql = sql
        .replace(" AS NUMERIC)", " AS DECIMAL(65,30))")
        .replace(" AS TEXT)", " AS CHAR)")
        .replace(" AS BIGINT)", " AS SIGNED)")
        .replace(
            " ON DUPLICATE KEY IGNORE",
            " ON DUPLICATE KEY UPDATE `id`=`id`",
        );
    for column in [
        "requests",
        "input_tokens",
        "output_tokens",
        "cached_input_tokens",
        "cost",
        "version",
    ] {
        sql = sql.replace(
            &format!("`excluded`.`{column}`"),
            &format!("VALUES(`{column}`)"),
        );
    }
    if sql.starts_with("INSERT INTO `credential_health`")
        && let Some((insert, _)) = sql.split_once(" ON DUPLICATE KEY UPDATE ")
    {
        let condition = "VALUES(`credential_version`) > `credential_version` OR (VALUES(`credential_version`) = `credential_version` AND VALUES(`version`) >= `version`)";
        let updates = [
            "state",
            "credential_version",
            "version",
            "observed_at",
            "response_status",
            "detail",
        ]
        .map(|column| format!("`{column}`=IF({condition},VALUES(`{column}`),`{column}`)"))
        .join(",");
        return format!("{insert} ON DUPLICATE KEY UPDATE {updates}");
    }
    sql
}

fn to_value(value: DbValue) -> Value {
    match value {
        DbValue::Null => Value::NULL,
        DbValue::Integer(value) => Value::Int(value),
        DbValue::Real(value) => Value::Double(value),
        DbValue::Text(value) => Value::Bytes(value.into_bytes()),
        DbValue::Blob(value) => Value::Bytes(value),
    }
}

fn decode_row(row: mysql_async::Row, columns: &[mysql_async::Column]) -> Result<Row, StoreError> {
    let values = row
        .unwrap()
        .into_iter()
        .zip(columns)
        .map(|(value, column)| {
            let value = match value {
                Value::NULL => DbValue::Null,
                Value::Int(value) => DbValue::Integer(value),
                Value::UInt(value) => DbValue::Integer(
                    i64::try_from(value)
                        .map_err(|_| StoreError::Database("MySQL integer exceeds i64".into()))?,
                ),
                Value::Float(value) => DbValue::Real(f64::from(value)),
                Value::Double(value) => DbValue::Real(value),
                Value::Bytes(value) if column.character_set() == 63 => DbValue::Blob(value),
                Value::Bytes(value) => {
                    DbValue::Text(String::from_utf8(value).map_err(|_| {
                        StoreError::Database("MySQL returned non-UTF-8 text".into())
                    })?)
                }
                Value::Date(..) | Value::Time(..) => {
                    return Err(StoreError::Database(
                        "unsupported MySQL temporal result".into(),
                    ));
                }
            };
            Ok((column.name_str().into_owned(), value))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    Ok(Row::new(values))
}

fn connection_error(_: impl std::fmt::Display) -> StoreError {
    StoreError::Database("MySQL connection failed".into())
}
fn database_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::Database(format!("MySQL: {error}"))
}
