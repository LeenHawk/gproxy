use super::{CasOutcome, cas};
use crate::{
    Repository, Result,
    entity::upstream::credential::{self, CredentialStatus},
    error::invalid,
};
use gproxy_seaorm::BatchConnectionTrait;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryTrait};

#[derive(Clone, Debug)]
pub struct CredentialRefresh {
    pub id: String,
    pub expected_version: i64,
    pub secret: Vec<u8>,
    pub expires_at_ms: Option<i64>,
}

/// A version-checked lifecycle change without touching the secret. Bumping the
/// version makes peers reload the credential like any other refresh.
#[derive(Clone, Debug)]
pub struct CredentialStatusUpdate {
    pub id: String,
    pub expected_version: i64,
    pub status: CredentialStatus,
    /// Human-readable cause for Dead; ignored for Active, which clears it.
    pub reason: Option<String>,
}

impl<C: BatchConnectionTrait> Repository<'_, C, credential::Entity> {
    /// A successful refresh also returns the credential to Active.
    pub async fn refresh_many(&self, updates: Vec<CredentialRefresh>) -> Result<Vec<CasOutcome>> {
        let statements = updates
            .into_iter()
            .map(|update| {
                let next = update
                    .expected_version
                    .checked_add(1)
                    .ok_or_else(|| invalid("credential version overflow"))?;
                Ok(credential::Entity::update_many()
                    .col_expr(credential::Column::Secret, Expr::val(update.secret))
                    .col_expr(
                        credential::Column::ExpiresAtMs,
                        Expr::val(update.expires_at_ms),
                    )
                    .col_expr(
                        credential::Column::Status,
                        Expr::val(CredentialStatus::Active),
                    )
                    .col_expr(credential::Column::StatusReason, Expr::val(None::<String>))
                    .col_expr(credential::Column::Version, Expr::val(next))
                    .filter(credential::Column::Id.eq(update.id))
                    .filter(credential::Column::Version.eq(update.expected_version))
                    .build(self.db.get_database_backend()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(self
            .db
            .atomic_batch(&statements)
            .await?
            .iter()
            .map(|r| cas(r.rows_affected()))
            .collect())
    }

    pub async fn set_status_many(
        &self,
        updates: Vec<CredentialStatusUpdate>,
    ) -> Result<Vec<CasOutcome>> {
        let statements = updates
            .into_iter()
            .map(|update| {
                let next = update
                    .expected_version
                    .checked_add(1)
                    .ok_or_else(|| invalid("credential version overflow"))?;
                let reason = match update.status {
                    CredentialStatus::Active => None,
                    CredentialStatus::Dead => update.reason,
                };
                Ok(credential::Entity::update_many()
                    .col_expr(credential::Column::Status, Expr::val(update.status))
                    .col_expr(credential::Column::StatusReason, Expr::val(reason))
                    .col_expr(credential::Column::Version, Expr::val(next))
                    .filter(credential::Column::Id.eq(update.id))
                    .filter(credential::Column::Version.eq(update.expected_version))
                    .build(self.db.get_database_backend()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(self
            .db
            .atomic_batch(&statements)
            .await?
            .iter()
            .map(|r| cas(r.rows_affected()))
            .collect())
    }
}
