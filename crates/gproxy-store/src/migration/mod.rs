//! Versioned schema migration: the migrator, the convention for adding to it,
//! and the single gate that decides whether this build may touch a database.
//!
//! # The story
//!
//! Fresh databases are created from the entity registry. Managed databases run
//! explicit migrations for changes that need them; `Store::sync` then uses the
//! connection's schema sync to add missing tables, defaulted columns and indexes.
//! Existing column types and data transformations are not inferred from entities.
//! The schema gate rejects foreign databases and unknown migration versions.
//!
//! # Adding a migration
//!
//! 1. **Change the entities first.** The registry is the schema's definition;
//!    a migration only carries existing databases to it.
//! 2. **Add one file** next to this one, named
//!    `m<YYYYMMDD>_<NNNNNN>_<what_it_does>.rs` — the date it was written, a
//!    six-digit ordinal within that date, and a short snake_case summary. The
//!    file name is the migration's identity: [`MigrationName::name`] returns it
//!    verbatim, and it is what lands in the ledger.
//! 3. **Append it** to [`Migrator::migrations`]. Append, never insert: order in
//!    that list is the order of history.
//! 4. **Guard it.** This is the rule that is particular to this workspace, and
//!    the reason it exists is [`m20260921_000001_baseline`]: the baseline is not
//!    a frozen transcript of the schema as it stood in September 2026, it is a
//!    live read of the entity registry, so on a fresh database it already
//!    creates whatever the entities say *today* — including the column your
//!    migration adds. A migration must therefore check before it acts:
//!
//!    ```ignore
//!    use gproxy_seaorm::SchemaProbeExt;
//!
//!    if !manager.probe_column("providers", "description").await? {
//!        manager.alter_table(/* … */).await?;
//!    }
//!    ```
//!
//!    `SchemaProbeExt` rather than `SchemaManager`'s own `has_column`: the
//!    inherent ones are compiled per backend behind `sea-orm-migration`'s
//!    `sqlx-*` features, which this workspace does not enable, and answer
//!    `BackendNotSupported` instead of the question. The probes work on every
//!    backend including a D1 binding.
//!
//! 5. **Never edit a released migration.** Once a version of gproxy that
//!    contains it has run anywhere, its name is in somebody's ledger and it will
//!    not run again; changing its body changes only what *new* databases get,
//!    and quietly forks the schema in two. A mistake in a released migration is
//!    fixed by appending another one.
//!
//! Data backfills belong in migrations too, and they need no guard beyond the
//! obvious one: on a fresh database there are no rows to backfill, so a
//! backfill is naturally a no-op there.

use gproxy_seaorm::{
    SchemaSyncConnectionTrait,
    sea_orm_migration::{
        MigrationStatus, MigrationTrait, MigratorTrait,
        sea_orm::{ActiveValue, EntityTrait},
        seaql_migrations,
    },
};

use crate::{Result, Store, StoreError};

mod m20260921_000001_baseline;
mod m20260926_000001_credential_cycles;

/// Every migration this build carries, oldest first.
///
/// See the module documentation before adding to it. The short version: append
/// only, and make the new entry a no-op when its change is already present.
pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260921_000001_baseline::Migration),
            Box::new(m20260926_000001_credential_cycles::Migration),
        ]
    }
}

/// What this build found in the database it was pointed at.
///
/// Read from the table census alone, which is the cheapest fact that is also
/// portable to every backend the adapter reaches, D1 included. Nothing here
/// inspects a column: this answers *whose* database this is, not whether it is
/// up to date — that second question is the ledger's, and only a database that
/// gets past this gate has a ledger worth reading.
///
/// # Adding a diagnosis
///
/// [`SchemaState::Foreign`] deliberately carries the table names rather than
/// just a message, so that a layer which recognises a *particular* foreign
/// schema can say something better than the generic refusal. gproxy v3's
/// databases are recognisable that way — its ledger is a table called
/// `schema_migrations`, where SeaORM's is `seaql_migrations` — and that
/// recognition belongs in `crates/gproxy/src/v3`, matching on the tables this
/// returns. Keep such knowledge there and the census here: one place decides
/// *whether* a database may be opened, and callers decide how much they can
/// explain about a refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaState {
    /// No tables at all. A fresh install; [`Store::install`] applies.
    Empty,
    /// This build's migration ledger is present, so the database is one of
    /// ours and the migrator decides what, if anything, is outstanding.
    Managed,
    /// Tables, but not this build's ledger. Somebody else owns this database.
    Foreign { tables: Vec<String> },
}

impl SchemaState {
    /// Take the census and classify it.
    pub async fn read<C: SchemaSyncConnectionTrait>(db: &C) -> Result<Self> {
        let tables = db.table_names().await?;
        Ok(Self::classify(tables))
    }

    fn classify(tables: Vec<String>) -> Self {
        if tables.is_empty() {
            Self::Empty
        } else if tables.iter().any(|table| table == MIGRATION_LEDGER) {
            Self::Managed
        } else {
            Self::Foreign { tables }
        }
    }

    /// The tables found, when this is a database we will not open. `None` for
    /// the two states that are not a refusal.
    pub fn foreign_tables(&self) -> Option<&[String]> {
        match self {
            Self::Foreign { tables } => Some(tables),
            _ => None,
        }
    }

    /// The refusal, as an operator can act on it, or `None` if this state is
    /// one this build can proceed from.
    pub fn refusal(&self) -> Option<String> {
        let tables = self.foreign_tables()?;
        let mut named: Vec<&str> = tables.iter().map(String::as_str).collect();
        named.sort_unstable();
        let shown = named
            .iter()
            .take(FOREIGN_TABLES_SHOWN)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let rest = named.len().saturating_sub(FOREIGN_TABLES_SHOWN);
        let listed = if rest == 0 {
            shown
        } else {
            format!("{shown} and {rest} more")
        };
        Some(format!(
            "this database holds {} table(s) but no `{MIGRATION_LEDGER}` ledger, so it was not \
             created by this build and there is no recorded point to migrate it forward from \
             ({listed}). Point gproxy at an empty database, or at one this build created.",
            named.len(),
        ))
    }
}

/// How many table names a refusal names before it starts counting.
const FOREIGN_TABLES_SHOWN: usize = 6;

/// The table [`Migrator`] keeps its ledger in. A constant because the schema
/// gate and anything diagnosing a foreign database both need to name it;
/// `ledger_matches_the_migrator` below keeps it honest.
pub const MIGRATION_LEDGER: &str = "seaql_migrations";

/// What a call to [`Store::migrate`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaReport {
    /// The database was empty and the whole schema was created in one pass.
    pub installed: bool,
    /// Migrations this call applied, in the order they ran. Empty when the
    /// database was already up to date.
    pub applied: Vec<String>,
    /// Every migration the ledger now records, oldest first.
    pub ledger: Vec<String>,
}

impl SchemaReport {
    /// One line for a log or a terminal.
    pub fn summary(&self) -> String {
        if self.installed {
            format!(
                "created the schema and recorded {} migration(s) as applied",
                self.ledger.len()
            )
        } else if self.applied.is_empty() {
            format!(
                "schema is up to date at {}",
                self.ledger.last().map(String::as_str).unwrap_or("nothing")
            )
        } else {
            format!("applied {}", self.applied.join(", "))
        }
    }
}

impl<C: SchemaSyncConnectionTrait> Store<C> {
    /// Classify the database without touching it.
    pub async fn schema_state(&self) -> Result<SchemaState> {
        SchemaState::read(&self.db).await
    }

    /// Bring the database to the schema this build owns, or refuse.
    ///
    /// Empty: create everything and record the whole migration history as
    /// applied, because the registry the baseline reads already includes every
    /// later migration's effect. Ours: apply whatever the ledger says is
    /// outstanding. Anybody else's: refuse, having changed nothing.
    ///
    /// There is no fourth branch in which this alters a table it did not
    /// recognise.
    pub async fn migrate(&self) -> Result<SchemaReport> {
        match self.schema_state().await? {
            SchemaState::Empty => {
                self.install().await?;
                let ledger = applied_names(&self.migration_report().await?);
                Ok(SchemaReport {
                    installed: true,
                    applied: ledger.clone(),
                    ledger,
                })
            }
            SchemaState::Managed => {
                let before = self.migration_report().await?;
                let pending: Vec<String> = before
                    .iter()
                    .filter(|(_, status)| *status == MigrationStatus::Pending)
                    .map(|(name, _)| name.clone())
                    .collect();
                if pending.is_empty() {
                    return Ok(SchemaReport {
                        installed: false,
                        applied: Vec::new(),
                        ledger: applied_names(&before),
                    });
                }
                self.db.run_migrations::<Migrator>(None).await?;
                let after = self.migration_report().await?;
                let ledger = applied_names(&after);
                Ok(SchemaReport {
                    installed: false,
                    applied: pending
                        .into_iter()
                        .filter(|name| ledger.contains(name))
                        .collect(),
                    ledger,
                })
            }
            state @ SchemaState::Foreign { .. } => {
                Err(StoreError::Invalid(state.refusal().unwrap_or_default()))
            }
        }
    }

    /// Create the whole schema on an empty database and record every migration
    /// as applied.
    ///
    /// The second half is the part that matters. Running the migrator from zero
    /// would reach the same schema — the baseline creates the registry and every
    /// later migration is a no-op against it — but it would do so in as many
    /// round trips as there are migrations, over connections that cannot all
    /// reach SeaORM's runner. Recording the history instead says the same thing
    /// the runner would have said, in one pass, on every backend. `migrator_and_
    /// install_agree_on_a_fresh_database` in `tests/migration.rs` is what holds
    /// the two paths to the same result.
    pub async fn install(&self) -> Result<()> {
        let registry = crate::register_entities(self.db.schema_registry());
        self.db.apply_schema(registry).await?;
        Migrator::install(&self.db).await?;
        let applied_at = now_seconds();
        let rows: Vec<seaql_migrations::ActiveModel> = Migrator::migrations()
            .iter()
            .map(|migration| seaql_migrations::ActiveModel {
                version: ActiveValue::Set(migration.name().to_owned()),
                applied_at: ActiveValue::Set(applied_at),
            })
            .collect();
        if !rows.is_empty() {
            seaql_migrations::Entity::insert_many(rows)
                .exec_without_returning(&self.db)
                .await?;
        }
        Ok(())
    }

    /// Finish installation after a host has archived a verified legacy schema.
    /// Unlike `install`, this tolerates interruption between DDL statements.
    /// The caller must keep traffic stopped until its data import completes.
    pub async fn resume_install(&self) -> Result<()> {
        let registry = crate::register_entities(self.db.schema_registry());
        self.db.sync_schema(registry).await?;
        Migrator::install(&self.db).await?;
        let rows = Migrator::migrations()
            .iter()
            .map(|migration| seaql_migrations::ActiveModel {
                version: ActiveValue::Set(migration.name().to_owned()),
                applied_at: ActiveValue::Set(now_seconds()),
            })
            .collect::<Vec<_>>();
        seaql_migrations::Entity::insert_many(rows)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(seaql_migrations::Column::Version)
                    .do_nothing_on([seaql_migrations::Column::Version])
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await?;
        Ok(())
    }

    /// Every migration this build carries, with what the ledger says about it.
    ///
    /// Reads only, including when the ledger is absent. Safe to call against
    /// a database this process is not migrating.
    pub async fn migration_report(&self) -> Result<Vec<(String, MigrationStatus)>> {
        Ok(self.db.migration_report::<Migrator>().await?)
    }
}

fn applied_names(report: &[(String, MigrationStatus)]) -> Vec<String> {
    report
        .iter()
        .filter(|(_, status)| *status == MigrationStatus::Applied)
        .map(|(name, _)| name.clone())
        .collect()
}

fn now_seconds() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_seaorm::ledger_table_name;

    /// The gate names the ledger as a constant so that a diagnosis somewhere
    /// else can match on it. This is what stops that constant drifting from the
    /// migrator that actually writes the table.
    #[test]
    fn ledger_matches_the_migrator() {
        assert_eq!(ledger_table_name::<Migrator>(), MIGRATION_LEDGER);
    }

    #[test]
    fn an_empty_database_is_a_fresh_install() {
        assert_eq!(SchemaState::classify(Vec::new()), SchemaState::Empty);
    }

    #[test]
    fn our_own_ledger_makes_a_database_ours() {
        assert_eq!(
            SchemaState::classify(vec![MIGRATION_LEDGER.into(), "users".into()]),
            SchemaState::Managed
        );
    }

    /// A v3 database is the case this was written for: tables, and a ledger
    /// under a name SeaORM never writes.
    #[test]
    fn tables_without_our_ledger_are_refused_by_name() {
        let state = SchemaState::classify(vec![
            "schema_migrations".into(),
            "users".into(),
            "providers".into(),
        ]);
        assert_eq!(
            state.foreign_tables().unwrap(),
            ["schema_migrations", "users", "providers"]
        );
        let refusal = state.refusal().unwrap();
        assert!(refusal.contains(MIGRATION_LEDGER), "{refusal}");
        assert!(refusal.contains("schema_migrations"), "{refusal}");
    }

    #[test]
    fn a_long_refusal_counts_the_tables_it_does_not_name() {
        let tables: Vec<String> = (0..FOREIGN_TABLES_SHOWN + 3)
            .map(|index| format!("table_{index:02}"))
            .collect();
        let refusal = SchemaState::classify(tables).refusal().unwrap();
        assert!(refusal.contains("and 3 more"), "{refusal}");
    }

    #[test]
    fn a_state_we_can_proceed_from_is_not_a_refusal() {
        assert!(SchemaState::Empty.refusal().is_none());
        assert!(SchemaState::Managed.refusal().is_none());
    }
}
