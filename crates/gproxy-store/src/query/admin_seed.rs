use sea_query::{Alias, Cond, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::value;

pub(crate) fn has_admin_users() -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .expr(Expr::val(1))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn ensure_default_organization() -> Result<Statement, StoreError> {
    let mut exists = Query::select();
    exists
        .expr(Expr::val(1))
        .from(Alias::new("organizations"))
        .and_where(Expr::col(Alias::new("name")).eq("default"))
        .limit(1);
    let mut values = Query::select();
    values
        .exprs([value("default"), value(true)])
        .cond_where(Cond::all().not().add(Expr::exists(exists)));
    let mut query = Query::insert();
    query
        .into_table(Alias::new("organizations"))
        .columns([Alias::new("name"), Alias::new("enabled")])
        .select_from(values)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&query)
}

pub(crate) fn promote_first_admin(
    username: &str,
    password_hash: &str,
) -> Result<Statement, StoreError> {
    let mut exists = Query::select();
    exists
        .expr(Expr::val(1))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .limit(1);
    let mut query = Query::update();
    query
        .table(Alias::new("users"))
        .value(Alias::new("password_hash"), password_hash.to_owned())
        .value(Alias::new("is_admin"), true)
        .value(Alias::new("enabled"), true)
        .and_where(Expr::col(Alias::new("name")).eq(username))
        .cond_where(Cond::all().not().add(Expr::exists(exists)));
    Statement::query(&query)
}

pub(crate) fn insert_first_admin(
    username: &str,
    password_hash: &str,
) -> Result<Statement, StoreError> {
    let mut admin_exists = Query::select();
    admin_exists
        .expr(Expr::val(1))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .limit(1);
    let mut user_exists = Query::select();
    user_exists
        .expr(Expr::val(1))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("name")).eq(username))
        .limit(1);
    let mut default_org = Query::select();
    default_org
        .column(Alias::new("id"))
        .from(Alias::new("organizations"))
        .and_where(Expr::col(Alias::new("name")).eq("default"))
        .limit(1);
    let mut values = Query::select();
    values
        .exprs([
            value(username.to_owned()),
            Expr::SubQuery(None, Box::new(default_org.into())),
            value(Option::<i64>::None),
            value(password_hash.to_owned()),
            value(true),
            value(true),
        ])
        .cond_where(
            Cond::all()
                .add(Cond::all().not().add(Expr::exists(admin_exists)))
                .add(Cond::all().not().add(Expr::exists(user_exists))),
        );
    let mut query = Query::insert();
    query
        .into_table(Alias::new("users"))
        .columns(
            [
                "name",
                "organization_id",
                "team_id",
                "password_hash",
                "enabled",
                "is_admin",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .select_from(values)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&query)
}

pub(crate) fn ensure_admin_permission(username: &str) -> Result<Statement, StoreError> {
    let users = Alias::new("users");
    let mut permission_exists = Query::select();
    permission_exists
        .expr(Expr::val(1))
        .from(Alias::new("permissions"))
        .and_where(Expr::col(Alias::new("subject_kind")).eq("user"))
        .and_where(Expr::col(Alias::new("subject_id")).equals((users.clone(), Alias::new("id"))))
        .and_where(Expr::col(Alias::new("provider_id")).is_null())
        .and_where(Expr::col(Alias::new("operation_group")).is_null())
        .and_where(Expr::col(Alias::new("allowed")).eq(true));
    let mut values = Query::select();
    values
        .exprs([
            value("user"),
            Expr::col((users.clone(), Alias::new("id"))),
            value(Option::<i64>::None),
            value(Option::<String>::None),
            value(true),
        ])
        .from(users.clone())
        .and_where(Expr::col((users.clone(), Alias::new("name"))).eq(username))
        .and_where(Expr::col((users, Alias::new("is_admin"))).eq(true))
        .cond_where(Cond::all().not().add(Expr::exists(permission_exists)));
    let mut query = Query::insert();
    query
        .into_table(Alias::new("permissions"))
        .columns(
            [
                "subject_kind",
                "subject_id",
                "provider_id",
                "operation_group",
                "allowed",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .select_from(values)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&query)
}
