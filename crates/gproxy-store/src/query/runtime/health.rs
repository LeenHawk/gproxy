use sea_query::{Alias, Cond, Expr, ExprTrait, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::value;
use crate::records::CredentialHealthInput;

pub(crate) fn upsert(input: &CredentialHealthInput) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_health"))
        .columns(
            [
                "credential_id",
                "credential_version",
                "version",
                "state",
                "observed_at",
                "response_status",
                "detail",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .values_panic([
            value(input.credential_id),
            value(i64::try_from(input.credential_version).map_err(|_| {
                StoreError::InvalidData {
                    field: "credential health version",
                    message: "version exceeds SQLite integer range".into(),
                }
            })?),
            value(input.version),
            value(input.state.as_str()),
            value(input.observed_at),
            value(input.response_status.map(i64::from)),
            value(input.detail.clone()),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("credential_id"))
                .update_columns([
                    Alias::new("state"),
                    Alias::new("credential_version"),
                    Alias::new("version"),
                    Alias::new("observed_at"),
                    Alias::new("response_status"),
                    Alias::new("detail"),
                ])
                .action_cond_where(
                    Cond::any()
                        .add(
                            Expr::col((Alias::new("excluded"), Alias::new("credential_version")))
                                .gt(Expr::col((
                                    Alias::new("credential_health"),
                                    Alias::new("credential_version"),
                                ))),
                        )
                        .add(
                            Cond::all()
                                .add(
                                    Expr::col((
                                        Alias::new("excluded"),
                                        Alias::new("credential_version"),
                                    ))
                                    .eq(Expr::col((
                                        Alias::new("credential_health"),
                                        Alias::new("credential_version"),
                                    ))),
                                )
                                .add(
                                    Expr::col((Alias::new("excluded"), Alias::new("version"))).gte(
                                        Expr::col((
                                            Alias::new("credential_health"),
                                            Alias::new("version"),
                                        )),
                                    ),
                                ),
                        ),
                )
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn select_all() -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(
            [
                "credential_id",
                "credential_version",
                "version",
                "state",
                "observed_at",
                "response_status",
                "detail",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new("credential_health"))
        .order_by(Alias::new("credential_id"), sea_query::Order::Asc);
    Statement::query(&query)
}

pub(crate) fn delete(credential_id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("credential_health"))
        .and_where(sea_query::Expr::col(Alias::new("credential_id")).eq(credential_id));
    Statement::query(&query)
}
