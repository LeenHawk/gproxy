use super::{CasOutcome, cas, sql::insert_if};
use crate::{
    Repository, Result, StoreError,
    entity::{
        resource::{agent_assignment as assignment, agent_session as session, resource_binding},
        subscription::{pool, pool_member},
        upstream::{credential, provider},
    },
    error::invalid,
    repository::affected,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use sea_orm::sea_query::{Expr, ExprTrait, Query, SelectStatement};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryTrait, Set};

#[derive(Clone, Debug)]
pub struct AssignmentReservation {
    pub id: String,
    pub session_id: String,
    pub expected_version: i64,
    pub previous_generation: Option<i64>,
    pub provider_id: String,
    pub credential_id: String,
    pub reason: assignment::AssignmentReason,
    pub exhausted_cycle_id: Option<String>,
    pub trigger_request_id: Option<String>,
    pub now_ms: i64,
}
#[derive(Clone, Debug)]
pub struct AssignmentActivation {
    pub session_id: String,
    pub assignment_id: String,
    pub expected_version: i64,
    pub generation: i64,
    pub now_ms: i64,
}
#[derive(Clone, Debug)]
pub struct AssignmentFailure {
    pub activation: AssignmentActivation,
    pub uncertain: bool,
    pub error: String,
}

fn target_available(provider_id: Expr, credential_id: Expr) -> SelectStatement {
    let member = Query::select()
        .expr(Expr::val(1))
        .from(pool_member::Entity)
        .inner_join(
            pool::Entity,
            Expr::col((pool_member::Entity, pool_member::Column::PoolId))
                .equals((pool::Entity, pool::Column::Id)),
        )
        .and_where(
            Expr::col((pool_member::Entity, pool_member::Column::PoolId))
                .equals((session::Entity, session::Column::PoolId)),
        )
        .and_where(
            Expr::col((pool_member::Entity, pool_member::Column::CredentialId))
                .equals((credential::Entity, credential::Column::Id)),
        )
        .and_where(pool_member::Column::Enabled.eq(true))
        .and_where(pool::Column::Enabled.eq(true))
        .to_owned();
    Query::select()
        .expr(Expr::val(1))
        .from(credential::Entity)
        .inner_join(
            provider::Entity,
            Expr::col((credential::Entity, credential::Column::ProviderId))
                .equals((provider::Entity, provider::Column::Id)),
        )
        .and_where(Expr::col((credential::Entity, credential::Column::Id)).eq(credential_id))
        .and_where(Expr::col((provider::Entity, provider::Column::Id)).eq(provider_id))
        .and_where(credential::Column::Enabled.eq(true))
        .and_where(provider::Column::Enabled.eq(true))
        .cond_where(
            Condition::any()
                .add(session::Column::PoolId.is_null())
                .add(Expr::exists(member)),
        )
        .to_owned()
}
fn live_session(now: i64) -> Condition {
    let mut subscriptions = super::oauth::eligible_subscriptions(now);
    subscriptions.and_where(
        Expr::col((
            crate::entity::subscription::user_subscription::Entity,
            crate::entity::subscription::user_subscription::Column::UserId,
        ))
        .equals((session::Entity, session::Column::UserId)),
    );
    Condition::all()
        .add(
            Condition::any()
                .add(session::Column::ExpiresAtMs.is_null())
                .add(session::Column::ExpiresAtMs.gt(now)),
        )
        .add(
            Condition::any()
                .add(session::Column::SubscriptionId.is_null())
                .add(session::Column::SubscriptionId.in_subquery(subscriptions)),
        )
}
fn proof(input: &AssignmentActivation, next: i64) -> SelectStatement {
    session::Entity::find_by_id(input.session_id.clone())
        .filter(session::Column::Version.eq(next))
        .filter(session::Column::ActiveGeneration.eq(input.generation))
        .filter(session::Column::State.eq(session::AgentSessionState::Ready))
        .into_query()
}

impl<C: BatchConnectionTrait> Repository<'_, C, session::Entity> {
    pub async fn current_assignments_many(
        &self,
        ids: &[String],
        now: i64,
    ) -> Result<Vec<Option<assignment::Model>>> {
        use gproxy_seaorm::SelectProjection;
        use sea_orm::FromQueryResult;
        let queries = ids
            .iter()
            .map(|id| {
                assignment::Entity::find()
                    .inner_join(session::Entity)
                    .filter(session::Column::Id.eq(id))
                    .filter(session::Column::State.eq(session::AgentSessionState::Ready))
                    .filter(
                        Expr::col((assignment::Entity, assignment::Column::Generation))
                            .equals((session::Entity, session::Column::ActiveGeneration)),
                    )
                    .filter(assignment::Column::State.eq(assignment::AssignmentState::Active))
                    .filter(live_session(now))
                    .filter(Expr::exists(target_available(
                        Expr::col((assignment::Entity, assignment::Column::ProviderId)),
                        Expr::col((assignment::Entity, assignment::Column::CredentialId)),
                    )))
                    .batch_query(self.db.get_database_backend())
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        self.db
            .query_batch(&queries)
            .await?
            .into_iter()
            .map(|rows| {
                rows.first()
                    .map(|r| assignment::Model::from_query_result(r, "").map_err(Into::into))
                    .transpose()
            })
            .collect()
    }
    /// Core establishes quota exhaustion/permissions. Store reserves the target
    /// and generation atomically and rechecks target/pool/subscription liveness.
    pub async fn reserve_assignments_many(
        &self,
        requests: Vec<AssignmentReservation>,
    ) -> Result<Vec<CasOutcome>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        let count = requests.len();
        for request in requests {
            let next = request
                .expected_version
                .checked_add(1)
                .ok_or_else(|| invalid("session version overflow"))?;
            if request.expected_version < 0 {
                return Err(invalid("negative session version"));
            }
            let reason = match request.reason {
                assignment::AssignmentReason::Initial => {
                    if request.previous_generation.is_some() {
                        return Err(invalid(
                            "initial assignment cannot have an active predecessor",
                        ));
                    }
                    Condition::all()
                        .add(session::Column::ActiveGeneration.is_null())
                        .add(session::Column::State.is_in([
                            session::AgentSessionState::Pending,
                            session::AgentSessionState::Blocked,
                        ]))
                }
                assignment::AssignmentReason::QuotaExhausted => {
                    if request.previous_generation.is_none()
                        || (request.exhausted_cycle_id.is_none()
                            && request.trigger_request_id.is_none())
                    {
                        return Err(invalid(
                            "exhaustion switch requires a predecessor and evidence reference",
                        ));
                    }
                    Condition::all()
                        .add(session::Column::ActiveGeneration.eq(request.previous_generation))
                        .add(session::Column::State.is_in([
                            session::AgentSessionState::Ready,
                            session::AgentSessionState::Blocked,
                        ]))
                }
            };
            let existing = assignment::Entity::find_by_id(request.id.clone()).into_query();
            let update = session::Entity::update_many()
                .col_expr(session::Column::Version, Expr::val(next))
                .col_expr(
                    session::Column::PendingAssignmentId,
                    Expr::val(request.id.clone()),
                )
                .col_expr(
                    session::Column::State,
                    Expr::val(session::AgentSessionState::Switching),
                )
                .col_expr(session::Column::UpdatedAtMs, Expr::val(request.now_ms))
                .filter(session::Column::Id.eq(&request.session_id))
                .filter(session::Column::Version.eq(request.expected_version))
                .filter(reason)
                .filter(live_session(request.now_ms))
                .filter(Expr::exists(existing.clone()).not())
                .filter(Expr::exists(target_available(
                    Expr::val(request.provider_id.clone()),
                    Expr::val(request.credential_id.clone()),
                )));
            batch.push(BatchStatement::Execute(update.build(backend)));
            let reserved = session::Entity::find_by_id(request.session_id.clone())
                .filter(session::Column::Version.eq(next))
                .filter(session::Column::PendingAssignmentId.eq(request.id.clone()))
                .filter(session::Column::State.eq(session::AgentSessionState::Switching))
                .into_query();
            let insert = insert_if::<assignment::Entity>(
                assignment::ActiveModel {
                    id: Set(request.id),
                    session_id: Set(request.session_id),
                    generation: Set(next),
                    previous_generation: Set(request.previous_generation),
                    provider_id: Set(request.provider_id),
                    credential_id: Set(request.credential_id),
                    reason: Set(request.reason),
                    exhausted_cycle_id: Set(request.exhausted_cycle_id),
                    trigger_request_id: Set(request.trigger_request_id),
                    created_at_ms: Set(request.now_ms),
                    ..Default::default()
                },
                Condition::all()
                    .add(Expr::exists(reserved))
                    .add(Expr::exists(existing).not()),
            )?;
            batch.push(BatchStatement::Execute(backend.build(&insert)));
        }
        let mut results = self.db.batch(&batch).await?.into_iter();
        (0..count)
            .map(|_| {
                let outcome = cas(affected(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?);
                results.next().ok_or(StoreError::UnexpectedResult)?;
                Ok(outcome)
            })
            .collect()
    }

    /// Publish a successfully prepared assignment. No remote network actions
    /// occur here. Old resource targets and in-flight capture IDs are untouched.
    pub async fn activate_assignments_many(
        &self,
        requests: Vec<AssignmentActivation>,
    ) -> Result<Vec<CasOutcome>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        let count = requests.len();
        for request in requests {
            let next = request
                .expected_version
                .checked_add(1)
                .ok_or_else(|| invalid("session version overflow"))?;
            let target = assignment::Entity::find_by_id(request.assignment_id.clone())
                .filter(assignment::Column::SessionId.eq(&request.session_id))
                .filter(assignment::Column::Generation.eq(request.generation))
                .filter(assignment::Column::State.eq(assignment::AssignmentState::Preparing))
                .filter(Expr::exists(target_available(
                    Expr::col((assignment::Entity, assignment::Column::ProviderId)),
                    Expr::col((assignment::Entity, assignment::Column::CredentialId)),
                )))
                .into_query();
            let bad = Query::select()
                .expr(Expr::val(1))
                .from(resource_binding::Entity)
                .inner_join(
                    assignment::Entity,
                    Expr::col((
                        resource_binding::Entity,
                        resource_binding::Column::AssignmentId,
                    ))
                    .equals((assignment::Entity, assignment::Column::Id)),
                )
                .and_where(resource_binding::Column::AssignmentId.eq(&request.assignment_id))
                .cond_where(
                    Condition::any()
                        .add(
                            Expr::col((
                                resource_binding::Entity,
                                resource_binding::Column::Generation,
                            ))
                            .not_equals((assignment::Entity, assignment::Column::Generation)),
                        )
                        .add(
                            Expr::col((
                                resource_binding::Entity,
                                resource_binding::Column::ProviderId,
                            ))
                            .not_equals((assignment::Entity, assignment::Column::ProviderId)),
                        )
                        .add(
                            Expr::col((
                                resource_binding::Entity,
                                resource_binding::Column::CredentialId,
                            ))
                            .not_equals((assignment::Entity, assignment::Column::CredentialId)),
                        )
                        .add(resource_binding::Column::UserId.is_null())
                        .add(
                            Expr::col((resource_binding::Entity, resource_binding::Column::UserId))
                                .not_equals((session::Entity, session::Column::UserId)),
                        ),
                )
                .to_owned();
            batch.push(BatchStatement::Execute(
                session::Entity::update_many()
                    .col_expr(session::Column::Version, Expr::val(next))
                    .col_expr(
                        session::Column::ActiveGeneration,
                        Expr::val(request.generation),
                    )
                    .col_expr(
                        session::Column::PendingAssignmentId,
                        Expr::val(Option::<String>::None),
                    )
                    .col_expr(
                        session::Column::State,
                        Expr::val(session::AgentSessionState::Ready),
                    )
                    .col_expr(session::Column::UpdatedAtMs, Expr::val(request.now_ms))
                    .filter(session::Column::Id.eq(&request.session_id))
                    .filter(session::Column::Version.eq(request.expected_version))
                    .filter(session::Column::PendingAssignmentId.eq(&request.assignment_id))
                    .filter(session::Column::State.eq(session::AgentSessionState::Switching))
                    .filter(live_session(request.now_ms))
                    .filter(Expr::exists(target))
                    .filter(Expr::exists(bad).not())
                    .build(backend),
            ));
            batch.push(BatchStatement::Execute(
                assignment::Entity::update_many()
                    .col_expr(
                        assignment::Column::State,
                        Expr::val(assignment::AssignmentState::Active),
                    )
                    .col_expr(assignment::Column::ActivatedAtMs, Expr::val(request.now_ms))
                    .filter(assignment::Column::Id.eq(&request.assignment_id))
                    .filter(assignment::Column::State.eq(assignment::AssignmentState::Preparing))
                    .filter(Expr::exists(proof(&request, next)))
                    .build(backend),
            ));
            batch.push(BatchStatement::Execute(
                assignment::Entity::update_many()
                    .col_expr(
                        assignment::Column::State,
                        Expr::val(assignment::AssignmentState::Replaced),
                    )
                    .col_expr(assignment::Column::ReplacedAtMs, Expr::val(request.now_ms))
                    .filter(assignment::Column::SessionId.eq(&request.session_id))
                    .filter(assignment::Column::Generation.ne(request.generation))
                    .filter(assignment::Column::State.eq(assignment::AssignmentState::Active))
                    .filter(Expr::exists(proof(&request, next)))
                    .build(backend),
            ));
        }
        let mut results = self.db.batch(&batch).await?.into_iter();
        (0..count)
            .map(|_| {
                let outcome = cas(affected(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?);
                for _ in 0..2 {
                    results.next().ok_or(StoreError::UnexpectedResult)?;
                }
                Ok(outcome)
            })
            .collect()
    }

    pub async fn fail_assignments_many(
        &self,
        requests: Vec<AssignmentFailure>,
    ) -> Result<Vec<CasOutcome>> {
        let backend = self.db.get_database_backend();
        let mut statements = Vec::new();
        for failure in requests {
            let request = failure.activation;
            let next = request
                .expected_version
                .checked_add(1)
                .ok_or_else(|| invalid("session version overflow"))?;
            if request.generation != request.expected_version {
                return Err(invalid(
                    "failure must match the reserved generation/version",
                ));
            }
            let pending = assignment::Entity::find_by_id(request.assignment_id.clone())
                .filter(assignment::Column::SessionId.eq(&request.session_id))
                .filter(assignment::Column::Generation.eq(request.generation))
                .filter(assignment::Column::State.eq(assignment::AssignmentState::Preparing))
                .into_query();
            statements.push(
                session::Entity::update_many()
                    .col_expr(session::Column::Version, Expr::val(next))
                    .col_expr(
                        session::Column::PendingAssignmentId,
                        Expr::val(Option::<String>::None),
                    )
                    .col_expr(
                        session::Column::State,
                        Expr::val(session::AgentSessionState::Blocked),
                    )
                    .col_expr(session::Column::UpdatedAtMs, Expr::val(request.now_ms))
                    .filter(session::Column::Id.eq(&request.session_id))
                    .filter(session::Column::Version.eq(request.expected_version))
                    .filter(session::Column::PendingAssignmentId.eq(&request.assignment_id))
                    .filter(session::Column::State.eq(session::AgentSessionState::Switching))
                    .filter(Expr::exists(pending))
                    .build(backend),
            );
            let proof = session::Entity::find_by_id(request.session_id.clone())
                .filter(session::Column::Version.eq(next))
                .filter(session::Column::State.eq(session::AgentSessionState::Blocked))
                .into_query();
            statements.push(
                assignment::Entity::update_many()
                    .col_expr(
                        assignment::Column::State,
                        Expr::val(if failure.uncertain {
                            assignment::AssignmentState::Uncertain
                        } else {
                            assignment::AssignmentState::Failed
                        }),
                    )
                    .col_expr(assignment::Column::Error, Expr::val(failure.error))
                    .filter(assignment::Column::Id.eq(request.assignment_id))
                    .filter(assignment::Column::SessionId.eq(request.session_id))
                    .filter(assignment::Column::Generation.eq(request.generation))
                    .filter(assignment::Column::State.eq(assignment::AssignmentState::Preparing))
                    .filter(Expr::exists(proof))
                    .build(backend),
            );
        }
        Ok(self
            .db
            .atomic_batch(&statements)
            .await?
            .iter()
            .step_by(2)
            .map(|r| cas(r.rows_affected()))
            .collect())
    }
}
