use crate::StoreError;
use crate::backend::Statement;

pub(crate) fn delete_permission(id: i64) -> Result<Statement, StoreError> {
    crate::query::delete_by_id("permissions", id)
}

pub(crate) fn delete_rate_limit(id: i64) -> Result<Statement, StoreError> {
    crate::query::delete_by_id("rate_limits", id)
}
