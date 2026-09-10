use sea_query::{Alias, Cond, Expr, ExprTrait, Query, SimpleExpr};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::json;
use crate::records::ProviderInput;

pub(crate) fn invalidate(id: i64, input: &ProviderInput) -> Result<Vec<Statement>, StoreError> {
    let fingerprint = input
        .tls_fingerprint
        .as_ref()
        .map(|value| json(value, "tls_fingerprint"))
        .transpose()?;
    let mut provider = Query::select();
    provider
        .column(Alias::new("id"))
        .from(Alias::new("providers"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .cond_where(
            Cond::any()
                .add(Expr::col(Alias::new("channel")).ne(&input.channel))
                .add(Expr::col(Alias::new("settings_json")).ne(json(&input.settings, "settings")?))
                .add(changed_optional("proxy_url", input.proxy_url.as_deref()))
                .add(changed_optional("tls_fingerprint", fingerprint.as_deref())),
        );
    let mut credentials = Query::select();
    credentials
        .column(Alias::new("id"))
        .from(Alias::new("credentials"))
        .and_where(Expr::col(Alias::new("provider_id")).in_subquery(provider.clone()));
    let mut statements = [
        "credential_quota_sources",
        "credential_quota_response_entries",
    ]
    .into_iter()
    .map(|table| {
        Statement::query(
            Query::delete()
                .from_table(Alias::new(table))
                .and_where(Expr::col(Alias::new("credential_id")).in_subquery(credentials.clone())),
        )
    })
    .collect::<Result<Vec<_>, _>>()?;
    statements.push(Statement::query(
        Query::update()
            .table(Alias::new("credentials"))
            .value(
                Alias::new("version"),
                Expr::col(Alias::new("version")).add(1),
            )
            .and_where(Expr::col(Alias::new("provider_id")).in_subquery(provider)),
    )?);
    Ok(statements)
}

fn changed_optional(column: &str, value: Option<&str>) -> SimpleExpr {
    let column = Expr::col(Alias::new(column));
    match value {
        Some(value) => column.clone().is_null().or(column.ne(value)),
        None => column.is_not_null(),
    }
}
