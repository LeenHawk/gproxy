use std::sync::Arc;

use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use sea_orm::{ConnectionTrait, DbBackend, DbErr, EntityTrait, ExecResult, QueryResult, Statement};
use send_wrapper::SendWrapper;
use serde_json::Value as Json;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::{
    BatchConnectionTrait, BatchResult, BatchStatement, D1_MAX_BIND_PARAMETERS, Projection, codec,
    error,
};

#[wasm_bindgen]
extern "C" {
    #[derive(Clone, Debug)]
    type D1Database;
    #[wasm_bindgen(method, catch)]
    fn prepare(this: &D1Database, sql: &str) -> Result<D1PreparedStatement, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn batch(this: &D1Database, statements: &Array) -> Result<Promise, JsValue>;

    type D1PreparedStatement;
    #[wasm_bindgen(method, catch, variadic)]
    fn bind(this: &D1PreparedStatement, values: &Array) -> Result<D1PreparedStatement, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn raw(this: &D1PreparedStatement, options: &JsValue) -> Result<Promise, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn run(this: &D1PreparedStatement) -> Result<Promise, JsValue>;
}

/// SeaORM connection to a Workers D1 binding, without interactive transactions.
///
/// JS handles and futures remain on their originating WASM thread. Cloning a
/// connection shares its binding, not a mutable/global projection.
///
/// This type does not implement or expose `TransactionTrait`. SeaORM APIs that
/// require it (including some cascading relation writes) cannot be used here.
#[derive(Clone, Debug)]
pub struct D1Connection {
    binding: Arc<SendWrapper<D1Database>>,
    pub(crate) projection: Projection,
}

fn js_error(value: JsValue) -> DbErr {
    let message = value
        .as_string()
        .or_else(|| {
            Reflect::get(&value, &JsValue::from_str("message"))
                .ok()?
                .as_string()
        })
        .unwrap_or_else(|| "D1 binding call failed".to_owned());
    error(message)
}

impl D1Connection {
    /// Bind the actual Workers `env.DB` object. Starts with a write-only projection.
    pub fn from_binding(binding: JsValue) -> Result<Self, DbErr> {
        for method in ["prepare", "batch"] {
            let function = Reflect::get(&binding, &JsValue::from_str(method)).map_err(js_error)?;
            if !function.is_instance_of::<Function>() {
                return Err(error("expected a Workers D1 database binding"));
            }
        }
        Ok(Self {
            binding: Arc::new(SendWrapper::new(binding.unchecked_into())),
            projection: Projection::new(),
        })
    }

    /// Make a connection view with result types derived from an entity.
    pub fn for_entity<E: EntityTrait>(&self) -> Result<Self, DbErr> {
        Ok(self.with_projection(Projection::for_entity::<E>()?))
    }

    /// Make a connection view for explicit SQL aliases, aggregates or joins.
    pub fn with_projection(&self, projection: Projection) -> Self {
        Self {
            binding: Arc::clone(&self.binding),
            projection,
        }
    }

    pub(crate) async fn raw_rows(&self, statement: Statement) -> Result<Json, DbErr> {
        SendWrapper::new(async {
            let options = Object::new();
            Reflect::set(&options, &JsValue::from_str("columnNames"), &JsValue::TRUE)
                .map_err(js_error)?;
            let result = JsFuture::from(self.prepare(&statement)?.raw(&options).map_err(js_error)?)
                .await
                .map_err(js_error)?;
            serde_wasm_bindgen::from_value(result).map_err(|_| error("invalid D1 raw query result"))
        })
        .await
    }

    fn prepare(&self, statement: &Statement) -> Result<D1PreparedStatement, DbErr> {
        if statement.db_backend != DbBackend::Sqlite {
            return Err(error("D1 requires a SQLite statement"));
        }
        let values = Array::new();
        if let Some(parameters) = &statement.values {
            if parameters.0.len() > D1_MAX_BIND_PARAMETERS {
                return Err(error(
                    "D1 supports at most 100 bound parameters per statement",
                ));
            }
            for value in &parameters.0 {
                let value = match codec::parameter(value)? {
                    codec::Parameter::Null => JsValue::NULL,
                    codec::Parameter::Number(value) => JsValue::from_f64(value),
                    codec::Parameter::Text(value) => JsValue::from_str(&value),
                    codec::Parameter::Bytes(value) => {
                        Uint8Array::from(value.as_slice()).buffer().into()
                    }
                };
                values.push(&value);
            }
        }
        self.binding
            .prepare(&statement.sql)
            .and_then(|prepared| prepared.bind(&values))
            .map_err(js_error)
    }

    /// Execute a write batch in one D1 transaction, returning logical changes per statement.
    ///
    /// All parameters are validated before sending the batch. SQL failures roll
    /// back all writes; zero affected rows do not. Encode business preconditions
    /// in every dependent write. A rejected future or result-decoding failure
    /// after dispatch does not prove rollback: recover by a durable operation ID,
    /// rather than automatically repeating non-idempotent writes.
    ///
    /// Returned rows (including RETURNING rows) are not decoded by this write API.
    pub async fn atomic_batch(&self, statements: &[Statement]) -> Result<Vec<ExecResult>, DbErr> {
        BatchConnectionTrait::atomic_batch(self, statements).await
    }
}

#[async_trait::async_trait]
impl BatchConnectionTrait for D1Connection {
    fn max_bind_parameters(&self) -> Option<usize> {
        Some(D1_MAX_BIND_PARAMETERS)
    }

    async fn batch(&self, statements: &[BatchStatement]) -> Result<Vec<BatchResult>, DbErr> {
        SendWrapper::new(async {
            if statements.is_empty() {
                return Ok(Vec::new());
            }
            let prepared = Array::new();
            for step in statements {
                step.validate(DbBackend::Sqlite)?;
                prepared.push(&self.prepare(step.statement())?.into());
            }
            let result = JsFuture::from(self.binding.batch(&prepared).map_err(js_error)?)
                .await
                .map_err(js_error)?;
            let results: Vec<Json> = serde_wasm_bindgen::from_value(result)
                .map_err(|_| error("invalid D1 batch result after dispatch"))?;
            if results.len() != statements.len() {
                return Err(error("D1 batch result count mismatch after dispatch"));
            }
            statements
                .iter()
                .zip(&results)
                .map(|(step, result)| match step {
                    BatchStatement::Execute(_) => {
                        codec::execution(result).map(|result| BatchResult::Executed(result.into()))
                    }
                    BatchStatement::Query(query) => codec::batch_rows(result, &query.projection)
                        .map(|rows| BatchResult::Rows(rows.into_iter().map(Into::into).collect())),
                })
                .collect()
        })
        .await
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for D1Connection {
    fn get_database_backend(&self) -> DbBackend {
        DbBackend::Sqlite
    }

    fn support_returning(&self) -> bool {
        true
    }

    async fn execute_raw(&self, statement: Statement) -> Result<ExecResult, DbErr> {
        SendWrapper::new(async {
            let result = JsFuture::from(self.prepare(&statement)?.run().map_err(js_error)?)
                .await
                .map_err(js_error)?;
            let result: Json = serde_wasm_bindgen::from_value(result)
                .map_err(|_| error("invalid D1 execution result after dispatch"))?;
            codec::execution(&result).map(Into::into)
        })
        .await
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        // Deliberately one statement; D1 itself rejects unsupported SQL transaction control.
        self.execute_raw(Statement::from_string(DbBackend::Sqlite, sql))
            .await
    }

    async fn query_one_raw(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
        Ok(self.query_all_raw(statement).await?.into_iter().next())
    }

    async fn query_all_raw(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
        let result = self.raw_rows(statement).await?;
        Ok(codec::rows(&result, &self.projection)?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}
