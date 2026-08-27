mod libsql;
#[cfg(not(target_arch = "wasm32"))]
mod mysql;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
mod postgres;
mod row;
mod statement;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

pub(crate) use row::{DbValue, QueryResult, Row};
pub(crate) use statement::Statement;

use crate::StoreError;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type DbFuture<'a, T> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, StoreError>> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub(crate) type DbFuture<'a, T> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, StoreError>> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait Executor: Send + Sync {
    fn execute<'a>(&'a self, statement: Statement) -> DbFuture<'a, QueryResult>;
    fn batch<'a>(&'a self, statements: Vec<Statement>) -> DbFuture<'a, Vec<QueryResult>>;
}

#[cfg(target_arch = "wasm32")]
pub(crate) trait Executor {
    fn execute<'a>(&'a self, statement: Statement) -> DbFuture<'a, QueryResult>;
    fn batch<'a>(&'a self, statements: Vec<Statement>) -> DbFuture<'a, Vec<QueryResult>>;
}

#[derive(Clone)]
pub enum BackendConfig {
    #[cfg(not(target_arch = "wasm32"))]
    Sqlite {
        path: PathBuf,
    },
    #[cfg(not(target_arch = "wasm32"))]
    Postgres {
        dsn: String,
    },
    #[cfg(not(target_arch = "wasm32"))]
    Mysql {
        dsn: String,
    },
    Libsql {
        url: String,
        auth_token: String,
    },
}

impl std::fmt::Debug for BackendConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Sqlite { path } => formatter
                .debug_struct("Sqlite")
                .field("path", path)
                .finish(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Postgres { .. } => formatter.write_str("Postgres { dsn: <redacted> }"),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Mysql { .. } => formatter.write_str("Mysql { dsn: <redacted> }"),
            Self::Libsql { url, .. } => formatter
                .debug_struct("Libsql")
                .field("url", url)
                .field("auth_token", &"<redacted>")
                .finish(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type SharedExecutor = std::sync::Arc<dyn Executor>;
#[cfg(target_arch = "wasm32")]
pub(crate) type SharedExecutor = std::rc::Rc<dyn Executor>;

pub(crate) async fn open(config: BackendConfig) -> Result<SharedExecutor, StoreError> {
    match config {
        #[cfg(not(target_arch = "wasm32"))]
        BackendConfig::Sqlite { path } => {
            Ok(std::sync::Arc::new(native::NativeSql::open(path).await?))
        }
        #[cfg(not(target_arch = "wasm32"))]
        BackendConfig::Postgres { dsn } => Ok(std::sync::Arc::new(
            postgres::Postgres::connect(&dsn).await?,
        )),
        #[cfg(not(target_arch = "wasm32"))]
        BackendConfig::Mysql { dsn } => Ok(std::sync::Arc::new(mysql::Mysql::connect(&dsn)?)),
        BackendConfig::Libsql { url, auth_token } => {
            let executor = libsql::LibsqlHttp::new(url, auth_token);
            #[cfg(not(target_arch = "wasm32"))]
            {
                Ok(std::sync::Arc::new(executor))
            }
            #[cfg(target_arch = "wasm32")]
            {
                Ok(std::rc::Rc::new(executor))
            }
        }
    }
}
