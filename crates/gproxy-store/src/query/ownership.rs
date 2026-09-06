use sea_query::{Alias, Expr, ExprTrait, Query, SelectStatement, SimpleExpr};

use crate::StoreError;
use crate::backend::Statement;
use crate::schema::{Ownership, tables};

const SUBJECT_KIND: &str = "subject_kind";
const SUBJECT_ID: &str = "subject_id";

/// Statements that delete one row of `table` together with everything the
/// schema says it owns: children first, deepest first, the row itself last,
/// so a caller can read the parent's affected count off the final result.
pub(crate) fn delete_owned(table: &'static str, id: i64) -> Result<Vec<Statement>, StoreError> {
    let filter = Expr::col(Alias::new("id")).eq(id);
    let mut statements = cascade(table, &filter)?;
    let mut parent = Query::delete();
    parent.from_table(Alias::new(table)).and_where(filter);
    statements.push(Statement::query(&parent)?);
    Ok(statements)
}

/// Statements that remove what rows of `table` matching `filter` own,
/// without touching those rows. Retention pruning uses this with a time
/// filter before it deletes the rows themselves.
pub(crate) fn cascade(
    table: &'static str,
    filter: &SimpleExpr,
) -> Result<Vec<Statement>, StoreError> {
    let spec = tables()
        .find(|spec| spec.name == table)
        .ok_or_else(|| StoreError::Database(format!("unknown table `{table}`")))?;
    let mut statements = Vec::new();
    for ownership in spec.owns {
        let parents = ids_of(table, filter);
        match *ownership {
            Ownership::Owns {
                table: child,
                column,
            } => {
                let child_filter = Expr::col(Alias::new(column)).in_subquery(parents);
                statements.extend(cascade(child, &child_filter)?);
                let mut delete = Query::delete();
                delete.from_table(Alias::new(child)).and_where(child_filter);
                statements.push(Statement::query(&delete)?);
            }
            Ownership::Detaches {
                table: child,
                column,
            } => {
                let mut update = Query::update();
                update
                    .table(Alias::new(child))
                    .value(
                        Alias::new(column),
                        SimpleExpr::Value(sea_query::Value::Int(None)),
                    )
                    .and_where(Expr::col(Alias::new(column)).in_subquery(parents));
                statements.push(Statement::query(&update)?);
            }
            Ownership::Scoped { table: child, kind } => {
                let child_filter = Expr::col(Alias::new(SUBJECT_KIND))
                    .eq(kind)
                    .and(Expr::col(Alias::new(SUBJECT_ID)).in_subquery(parents));
                statements.extend(cascade(child, &child_filter)?);
                let mut delete = Query::delete();
                delete.from_table(Alias::new(child)).and_where(child_filter);
                statements.push(Statement::query(&delete)?);
            }
        }
    }
    Ok(statements)
}

/// Statements that remove rows whose owner no longer exists, for every
/// declared ownership. Orphans can own orphans, so a sweep is applied more
/// than once; each statement is idempotent.
pub(crate) fn orphan_sweep() -> Result<Vec<Statement>, StoreError> {
    let mut statements = Vec::new();
    for spec in tables() {
        for ownership in spec.owns {
            let living = ids_of(spec.name, &Expr::val(1).eq(1));
            match *ownership {
                Ownership::Owns {
                    table: child,
                    column,
                } => {
                    let mut delete = Query::delete();
                    delete.from_table(Alias::new(child)).and_where(
                        Expr::col(Alias::new(column))
                            .is_not_null()
                            .and(Expr::col(Alias::new(column)).not_in_subquery(living)),
                    );
                    statements.push(Statement::query(&delete)?);
                }
                Ownership::Detaches {
                    table: child,
                    column,
                } => {
                    let mut update = Query::update();
                    update
                        .table(Alias::new(child))
                        .value(
                            Alias::new(column),
                            SimpleExpr::Value(sea_query::Value::Int(None)),
                        )
                        .and_where(
                            Expr::col(Alias::new(column))
                                .is_not_null()
                                .and(Expr::col(Alias::new(column)).not_in_subquery(living)),
                        );
                    statements.push(Statement::query(&update)?);
                }
                Ownership::Scoped { table: child, kind } => {
                    let mut delete = Query::delete();
                    delete.from_table(Alias::new(child)).and_where(
                        Expr::col(Alias::new(SUBJECT_KIND))
                            .eq(kind)
                            .and(Expr::col(Alias::new(SUBJECT_ID)).not_in_subquery(living)),
                    );
                    statements.push(Statement::query(&delete)?);
                }
            }
        }
    }
    Ok(statements)
}

fn ids_of(table: &'static str, filter: &SimpleExpr) -> SelectStatement {
    let mut select = Query::select();
    select
        .column(Alias::new("id"))
        .from(Alias::new(table))
        .and_where(filter.clone());
    select
}
