use sea_query::{Alias, Condition, Expr, ExprTrait, JoinType, OnConflict, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{json, value};
use crate::records::BindingInput;

const COLUMNS: &[&str] = &[
    "provider_id",
    "owner_user_id",
    "kind",
    "resource_id",
    "credential_id",
    "summary_json",
    "created_at",
];

pub(crate) fn upsert_binding(input: &BindingInput, now: i64) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("surface_bindings"))
        .columns(
            [
                "provider_id",
                "owner_user_id",
                "kind",
                "resource_id",
                "credential_id",
                "summary_json",
                "created_at",
                "updated_at",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .values_panic([
            value(input.provider_id),
            value(input.owner_user_id),
            value(input.kind.clone()),
            value(input.resource_id.clone()),
            value(input.credential_id),
            value(json(&input.summary, "binding summary")?),
            value(now),
            value(now),
        ])
        .on_conflict(
            OnConflict::columns(
                ["provider_id", "owner_user_id", "kind", "resource_id"]
                    .into_iter()
                    .map(Alias::new),
            )
            .update_columns([
                Alias::new("credential_id"),
                Alias::new("summary_json"),
                Alias::new("updated_at"),
            ])
            .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn find_binding(
    provider_id: i64,
    owner_user_id: i64,
    kind: &str,
    resource_id: &str,
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(COLUMNS.iter().copied().map(Alias::new))
        .from(Alias::new("surface_bindings"))
        .and_where(Expr::col(Alias::new("provider_id")).eq(provider_id))
        .and_where(Expr::col(Alias::new("owner_user_id")).eq(owner_user_id))
        .and_where(Expr::col(Alias::new("kind")).eq(kind))
        .and_where(Expr::col(Alias::new("resource_id")).eq(resource_id))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn delete_binding(
    provider_id: i64,
    owner_user_id: i64,
    kind: &str,
    resource_id: &str,
) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("surface_bindings"))
        .and_where(Expr::col(Alias::new("provider_id")).eq(provider_id))
        .and_where(Expr::col(Alias::new("owner_user_id")).eq(owner_user_id))
        .and_where(Expr::col(Alias::new("kind")).eq(kind))
        .and_where(Expr::col(Alias::new("resource_id")).eq(resource_id));
    Statement::query(&query)
}

pub(crate) fn list_bindings(
    provider_id: i64,
    owner_user_id: i64,
    kind: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<Statement, StoreError> {
    let binding = Alias::new("binding");
    let cursor_row = Alias::new("cursor_row");
    let mut query = Query::select();
    query
        .columns(
            COLUMNS
                .iter()
                .map(|column| (binding.clone(), Alias::new(*column))),
        )
        .from_as(Alias::new("surface_bindings"), binding.clone())
        .and_where(Expr::col((binding.clone(), Alias::new("provider_id"))).eq(provider_id))
        .and_where(Expr::col((binding.clone(), Alias::new("owner_user_id"))).eq(owner_user_id))
        .and_where(Expr::col((binding.clone(), Alias::new("kind"))).eq(kind));
    if let Some(cursor) = cursor {
        query
            .join_as(
                JoinType::InnerJoin,
                Alias::new("surface_bindings"),
                cursor_row.clone(),
                Condition::all()
                    .add(Expr::col((cursor_row.clone(), Alias::new("provider_id"))).eq(provider_id))
                    .add(
                        Expr::col((cursor_row.clone(), Alias::new("owner_user_id")))
                            .eq(owner_user_id),
                    )
                    .add(Expr::col((cursor_row.clone(), Alias::new("kind"))).eq(kind))
                    .add(Expr::col((cursor_row.clone(), Alias::new("resource_id"))).eq(cursor)),
            )
            .and_where(
                Condition::any()
                    .add(
                        Expr::col((binding.clone(), Alias::new("created_at")))
                            .lt(Expr::col((cursor_row.clone(), Alias::new("created_at")))),
                    )
                    .add(
                        Condition::all()
                            .add(
                                Expr::col((binding.clone(), Alias::new("created_at")))
                                    .eq(Expr::col((cursor_row.clone(), Alias::new("created_at")))),
                            )
                            .add(
                                Expr::col((binding.clone(), Alias::new("id")))
                                    .lt(Expr::col((cursor_row, Alias::new("id")))),
                            ),
                    )
                    .into(),
            );
    }
    query
        .order_by((binding.clone(), Alias::new("created_at")), Order::Desc)
        .order_by((binding, Alias::new("id")), Order::Desc)
        .limit(u64::from(limit).saturating_add(1));
    Statement::query(&query)
}
