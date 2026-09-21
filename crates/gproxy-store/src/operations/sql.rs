use crate::{Result, error::invalid};
use sea_orm::sea_query::{Expr, InsertStatement, Query};
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, Condition, EntityTrait, Iterable};

/// Insert an ActiveModel only if a database condition still holds. Column casts
/// are exactly those used by ordinary ORM inserts, including fixed-point values.
pub(crate) fn insert_if<E: EntityTrait>(
    model: E::ActiveModel,
    condition: Condition,
) -> Result<InsertStatement> {
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for column in E::Column::iter() {
        if let ActiveValue::Set(value) | ActiveValue::Unchanged(value) = model.get(column) {
            columns.push(column);
            values.push(column.save_as(Expr::val(value)));
        }
    }
    let select = Query::select()
        .exprs(values)
        .cond_where(condition)
        .to_owned();
    let mut insert = Query::insert();
    insert
        .into_table(E::default().table_ref())
        .columns(columns)
        .select_from(select)
        .map_err(|e| invalid(e.to_string()))?;
    Ok(insert)
}
