use sea_query::{Alias, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;

pub(crate) fn delete_price_rate(id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("price_rates"))
        .and_where(Expr::col(Alias::new("id")).eq(id));
    Statement::query(&query)
}
