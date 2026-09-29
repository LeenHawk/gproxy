use super::{LEASE_MS, STATE, now};
use gproxy_seaorm::{BatchConnectionTrait, BatchQuery, BatchResult, BatchStatement};
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Query};
use sea_orm::{ConnectionTrait, DbBackend, DbErr, ExecResult, QueryResult, Statement};

#[derive(Clone)]
pub struct Guard<C> {
    pub db: C,
    pub owner: String,
}
impl<C: BatchConnectionTrait> Guard<C> {
    fn guard(&self) -> Statement {
        let valid = Query::select()
            .expr(Expr::val(1))
            .from(Alias::new(STATE))
            .and_where(Expr::col(Alias::new("id")).eq(1))
            .and_where(Expr::col(Alias::new("owner")).eq(&self.owner))
            .and_where(Expr::col(Alias::new("phase")).lt(5))
            .to_owned();
        // A lost lease must fail the entire batch, not merely update zero rows.
        let select = Query::select()
            .expr(Expr::val(1))
            .and_where(Expr::exists(valid).not())
            .to_owned();
        let insert = Query::insert()
            .into_table(Alias::new(STATE))
            .columns([Alias::new("id")])
            .select_from(select)
            .expect("one guard column")
            .to_owned();
        self.get_database_backend().build(&insert)
    }
    fn renew(&self) -> Statement {
        self.get_database_backend().build(
            Query::update()
                .table(Alias::new(STATE))
                .value(Alias::new("expires_at"), now() + LEASE_MS)
                .and_where(Expr::col(Alias::new("id")).eq(1))
                .and_where(Expr::col(Alias::new("owner")).eq(&self.owner)),
        )
    }
}
#[async_trait::async_trait]
impl<C: BatchConnectionTrait> ConnectionTrait for Guard<C> {
    fn get_database_backend(&self) -> DbBackend {
        self.db.get_database_backend()
    }
    fn support_returning(&self) -> bool {
        self.db.support_returning()
    }
    async fn execute_raw(&self, statement: Statement) -> Result<ExecResult, DbErr> {
        match self
            .batch(&[BatchStatement::Execute(statement)])
            .await?
            .remove(0)
        {
            BatchResult::Executed(result) => Ok(result),
            _ => Err(DbErr::Custom("migration write returned rows".into())),
        }
    }
    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        self.execute_raw(Statement::from_string(self.get_database_backend(), sql))
            .await
    }
    async fn query_one_raw(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
        self.db.query_one_raw(statement).await
    }
    async fn query_all_raw(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
        self.db.query_all_raw(statement).await
    }
}
#[async_trait::async_trait]
impl<C: BatchConnectionTrait> BatchConnectionTrait for Guard<C> {
    fn requires_query_projection(&self) -> bool {
        self.db.requires_query_projection()
    }
    fn max_bind_parameters(&self) -> Option<usize> {
        self.db.max_bind_parameters()
    }
    async fn query_rows(&self, query: BatchQuery) -> Result<Vec<QueryResult>, DbErr> {
        self.db.query_rows(query).await
    }
    async fn query_batch(&self, queries: &[BatchQuery]) -> Result<Vec<Vec<QueryResult>>, DbErr> {
        self.db.query_batch(queries).await
    }
    async fn batch(&self, statements: &[BatchStatement]) -> Result<Vec<BatchResult>, DbErr> {
        let mut guarded = vec![
            BatchStatement::Execute(self.guard()),
            BatchStatement::Execute(self.renew()),
        ];
        guarded.extend_from_slice(statements);
        let mut results = self.db.batch(&guarded).await?;
        results.drain(..2);
        Ok(results)
    }
}
