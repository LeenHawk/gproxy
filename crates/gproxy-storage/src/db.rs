use std::sync::{OnceLock, RwLock};

use sea_orm::{Database, DatabaseConnection, DbErr};

struct SharedDb {
    dsn: String,
    connection: DatabaseConnection,
}

static SHARED_DB: OnceLock<RwLock<Option<SharedDb>>> = OnceLock::new();

pub async fn connect_shared(dsn: &str) -> Result<DatabaseConnection, DbErr> {
    let lock = SHARED_DB.get_or_init(|| RwLock::new(None));
    if let Ok(guard) = lock.read()
        && let Some(shared) = guard.as_ref()
            && shared.dsn == dsn {
                return Ok(shared.connection.clone());
            }

    let connection = Database::connect(dsn).await?;
    if let Ok(mut guard) = lock.write() {
        *guard = Some(SharedDb {
            dsn: dsn.to_string(),
            connection: connection.clone(),
        });
    }
    Ok(connection)
}
