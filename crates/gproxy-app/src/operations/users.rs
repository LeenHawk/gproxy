//! Users: the actors everything else hangs off.
//!
//! Two rules live here and nowhere else.
//!
//! **An instance keeps at least one enabled administrator.** Deleting,
//! disabling or demoting the last one is refused. The role is not a
//! membership and cannot be granted by anybody who is not already an admin, so
//! losing it requires host-level recovery. v3 had the same guard for the
//! same reason.
//!
//! **A password change ends the user's sessions**, in the same transaction.
//! A session outlives the credential that opened it otherwise, which is
//! exactly the window a password reset is meant to close.

use gproxy_seaorm::{BatchConnectionTrait, BatchResult, BatchStatement};
use gproxy_store::{
    Repository,
    entity::identity::{user, user_session},
};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QuerySelect, QueryTrait, Select, Set,
};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    AppError, Result,
    auth::password,
    dto::{BatchItem, ListQuery, Page, UserDto, UserPatch, UserWrite},
};

/// The two values `users.role` takes. Organization and team administration is
/// a membership role and is a different column in a different table.
pub const ROLES: [&str; 2] = ["admin", "user"];
const ADMIN: &str = "admin";
/// The fallback account used by host-level administrator setup and recovery.
pub const BOOTSTRAP_ADMIN_ID: &str = "0";

pub struct Users<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Users<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Users<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<UserDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<UserDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: UserWrite) -> Result<UserDto> {
        crud::create(self, write).await
    }

    /// Host-only setup. Explicit credentials update a same-name user's password;
    /// otherwise user 0 is created or enabled and renamed. Without a password,
    /// an existing instance is left unchanged.
    pub async fn bootstrap_admin(&self, name: &str, password: Option<&str>) -> Result<bool> {
        if let Some(password) = password {
            return self.apply_admin_password(name, password).await;
        }
        if self
            .writer
            .store()
            .users()
            .query(user::Entity::find().limit(1))
            .await?
            .is_empty()
        {
            return Err(AppError::invalid(
                "an initial administrator password is required for an empty database",
            ));
        }
        Ok(false)
    }

    /// Prefer a same-name user without renaming or changing their role/status.
    /// Only when that name is absent does the override recover administrator 0.
    pub async fn apply_admin_password(&self, name: &str, plaintext: &str) -> Result<bool> {
        let name = crud::text(name, "administrator name")?;
        password::validate(plaintext)?;
        let users = self.writer.store().users();
        if let Some(existing) = users
            .query(user::Entity::find().filter(user::Column::Name.eq(&name)))
            .await?
            .into_iter()
            .next()
        {
            if existing
                .password_hash
                .as_deref()
                .is_some_and(|hash| password::verify(plaintext, hash))
            {
                return Ok(false);
            }
            self.set_password(&existing.id, plaintext).await?;
            return Ok(true);
        }

        let existing = users
            .get_many(&[BOOTSTRAP_ADMIN_ID.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten();
        let mut row = user::ActiveModel {
            id: Set(BOOTSTRAP_ADMIN_ID.to_owned()),
            name: Set(name.clone()),
            password_hash: Set(Some(password::hash(plaintext)?)),
            role: Set(ADMIN.to_owned()),
            enabled: Set(true),
            ..Default::default()
        };
        let mut statements = Vec::new();
        if existing.is_some() {
            statements.push(BatchStatement::Execute(
                users
                    .update_statement(row)?
                    .ok_or_else(|| AppError::internal("administrator override set no column"))?,
            ));
            statements.push(BatchStatement::Execute(
                self.writer.store().user_sessions().delete_where_statement(
                    user_session::Entity::delete_many()
                        .filter(user_session::Column::UserId.eq(BOOTSTRAP_ADMIN_ID)),
                ),
            ));
        } else {
            row.oauth_client_allowlist = Set(None);
            row.created_at_ms = Set(crate::now_ms());
            // The condition is evaluated in the database, including across isolates.
            let target = user::Entity::find()
                .filter(
                    Condition::any()
                        .add(user::Column::Id.eq(BOOTSTRAP_ADMIN_ID))
                        .add(user::Column::Name.eq(name)),
                )
                .limit(1);
            let condition = Condition::all().add(Expr::exists(target.into_query()).not());
            statements.push(BatchStatement::Execute(
                users.insert_if_statement(row, condition)?,
            ));
        }
        let (_, results) = self
            .writer
            .commit_results(statements, &[Scope::Identity])
            .await?;
        match results.first() {
            Some(BatchResult::Executed(result)) => Ok(result.rows_affected() == 1),
            _ => Err(AppError::internal(
                "administrator override returned no write result",
            )),
        }
    }

    /// Patch a user, refusing the two changes that could leave the instance
    /// without an administrator.
    pub async fn update(&self, id: &str, patch: UserPatch) -> Result<UserDto> {
        let demoted = patch
            .role
            .as_deref()
            .is_some_and(|role| !role.trim().eq_ignore_ascii_case(ADMIN));
        if demoted || patch.enabled == Some(false) {
            self.guard_last_admin(id).await?;
        }
        crud::update(self, id, patch).await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.guard_last_admin(id).await?;
        crud::delete(self, id).await
    }

    pub async fn batch(
        &self,
        items: Vec<BatchItem<UserWrite, UserPatch>>,
    ) -> Result<Vec<Option<UserDto>>> {
        crud::batch(self, items).await
    }

    /// Set or replace a user's console password, and end every session they
    /// hold, as one revision commit.
    ///
    /// The plaintext never leaves this call: it is hashed here and the row
    /// holds only the PHC string.
    pub async fn set_password(&self, id: &str, new_password: &str) -> Result<UserDto> {
        crud::row::<C, Self>(self, id).await?;
        password::validate(new_password)?;
        let hashed = password::hash(new_password)?;
        let statements = vec![
            BatchStatement::Execute(
                self.writer
                    .store()
                    .users()
                    .update_statement(user::ActiveModel {
                        id: Set(id.to_owned()),
                        password_hash: Set(Some(hashed)),
                        ..Default::default()
                    })?
                    .ok_or_else(|| AppError::internal("password patch set no column"))?,
            ),
            BatchStatement::Execute(self.writer.store().user_sessions().delete_where_statement(
                user_session::Entity::delete_many().filter(user_session::Column::UserId.eq(id)),
            )),
        ];
        crud::commit_one::<C, Self>(self, statements, id, &[Scope::Identity]).await
    }

    /// Clear a user's password, leaving an account that can only be used
    /// through its API keys. Sessions go with it, for the same reason.
    pub async fn clear_password(&self, id: &str) -> Result<UserDto> {
        crud::row::<C, Self>(self, id).await?;
        let statements = vec![
            BatchStatement::Execute(
                self.writer
                    .store()
                    .users()
                    .update_statement(user::ActiveModel {
                        id: Set(id.to_owned()),
                        password_hash: Set(None),
                        ..Default::default()
                    })?
                    .ok_or_else(|| AppError::internal("password patch set no column"))?,
            ),
            BatchStatement::Execute(self.writer.store().user_sessions().delete_where_statement(
                user_session::Entity::delete_many().filter(user_session::Column::UserId.eq(id)),
            )),
        ];
        crud::commit_one::<C, Self>(self, statements, id, &[Scope::Identity]).await
    }

    /// Replace or clear a user's OAuth client allowlist without touching
    /// anything else. `None` restores inheritance; an empty list denies every
    /// client for this user.
    pub async fn set_allowlist(&self, id: &str, clients: Option<Vec<String>>) -> Result<UserDto> {
        self.update(
            id,
            UserPatch {
                oauth_client_allowlist: Some(clients),
                ..UserPatch::default()
            },
        )
        .await
    }

    /// Refuse when `id` is the only enabled administrator left.
    ///
    /// Read from the database rather than from the snapshot: the snapshot can
    /// be a revision behind, and being behind is unsafe in exactly this
    /// direction — an admin deleted a moment ago would still be counted, and
    /// the guard would wave through the deletion of the real last one.
    async fn guard_last_admin(&self, id: &str) -> Result<()> {
        let admins = self
            .writer
            .store()
            .users()
            .query(
                user::Entity::find()
                    .filter(user::Column::Role.eq(ADMIN))
                    .filter(user::Column::Enabled.eq(true)),
            )
            .await?;
        if admins.len() == 1 && admins[0].id == id {
            return Err(AppError::Conflict(
                "the last enabled administrator cannot be removed, disabled or demoted".into(),
            ));
        }
        Ok(())
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> Result<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().users(),
            Condition::all().add(user::Column::Name.eq(&name)),
            exclude,
            || format!("a user named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Users<'_, C> {
    type Entity = user::Entity;
    type Dto = UserDto;
    type Write = UserWrite;
    type Patch = UserPatch;

    const ENTITY: &'static str = "user";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().users()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Identity]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = user::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(user::Column::Name.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(user::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: UserWrite) -> Result<(user::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let password_hash = match write.password.as_deref() {
            Some(plaintext) => {
                password::validate(plaintext)?;
                Some(password::hash(plaintext)?)
            }
            None => None,
        };
        let row = user::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            password_hash: Set(password_hash),
            role: Set(crud::one_of(
                write.role.as_deref().unwrap_or("user"),
                "role",
                &ROLES,
            )?),
            enabled: Set(write.enabled.unwrap_or(true)),
            oauth_client_allowlist: Set(write
                .oauth_client_allowlist
                .map(|entries| crud::allowlist(entries, "oauthClientAllowlist"))
                .transpose()?),
            created_at_ms: Set(crate::now_ms()),
        };
        Ok((row, id))
    }

    async fn change(&self, current: &user::Model, patch: UserPatch) -> Result<user::ActiveModel> {
        let mut row = user::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(role) = patch.role {
            row.role = Set(crud::one_of(&role, "role", &ROLES)?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        if let Some(allowlist) = patch.oauth_client_allowlist {
            row.oauth_client_allowlist = Set(allowlist
                .map(|entries| crud::allowlist(entries, "oauthClientAllowlist"))
                .transpose()?);
        }
        Ok(row)
    }
}
