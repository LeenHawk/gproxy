//! Hold a session advisory lock across MySQL's implicit DDL commits and the
//! shared resumable import. A private one-connection pool pins the session.
use crate::{Error, Result};
use gproxy_app::{AppConfig, config::StoreBackendConfig};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend};

pub(crate) async fn run(config: &AppConfig) -> Result<()> {
    let StoreBackendConfig::Url { dsn } = &config.store else {
        unreachable!()
    };
    let mut options = ConnectOptions::new(dsn.clone());
    options
        .min_connections(1)
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .sqlx_logging(false);
    let db = Database::connect(options).await?;
    let sql = match db.get_database_backend() {
        DbBackend::Postgres => "SELECT pg_try_advisory_lock(6770726, 34) AS locked",
        DbBackend::MySql => "SELECT GET_LOCK('gproxy:v3-to-v4', 0) AS locked",
        _ => {
            return Err(Error::other(
                "remote v3 migration requires PostgreSQL or MySQL",
            ));
        }
    };
    let result = async {
        let row = db
            .query_one_raw(sea_orm::Statement::from_string(
                db.get_database_backend(),
                sql,
            ))
            .await?
            .ok_or_else(|| Error::other("migration lock returned no result"))?;
        let locked = if db.get_database_backend() == DbBackend::Postgres {
            row.try_get::<bool>("", "locked")?
        } else {
            row.try_get::<i64>("", "locked")? == 1
        };
        if !locked {
            return Err(Error::other(
                "another instance is migrating this database; retry after it finishes",
            ));
        }
        gproxy_app::v3::upgrade::run(db.clone(), config, true).await?;
        Ok(())
    }
    .await;
    db.close().await?;
    result
}
