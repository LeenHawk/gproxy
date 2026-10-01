//! Agent session assignments: which credential a long-lived agent session is
//! bound to, across requests and instances. The upper layer creates the
//! `agent_sessions` row and names it on the request; core reads the active
//! assignment, keeps the session on that credential while it is usable,
//! reserves a new generation when the credential is durably unusable
//! (dead, disabled, retired or quota-blocked), and activates or fails the
//! reservation from the outcome of the attempt that prepared it. Transient
//! trouble (rate limits, failure cooldowns, a 5xx on this request) does not
//! move the session: the request uses another credential unbound.

use crate::{
    AgentAssignmentRef, BlockSource, Core, CoreResult, CredentialBlocks, CredentialData,
    CredentialStatus, RequestContext, ids,
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    entity::resource::{
        agent_assignment::{self, AssignmentReason},
        agent_session::{self, AgentSessionState},
    },
    operations::{
        CasOutcome,
        agents::{AssignmentActivation, AssignmentFailure, AssignmentReservation},
    },
};
use std::sync::Arc;

/// Concurrent first requests race on the session version; a few retries
/// converge on whichever reservation won.
const RESERVE_RETRIES: usize = 3;

/// The assignment an attempt runs under.
#[derive(Clone, Debug)]
pub(crate) struct AssignmentHandle {
    pub reference: AgentAssignmentRef,
    /// Reserved by this request: its first attempt's outcome activates or
    /// fails the generation. A converged or already active assignment is
    /// someone else's to settle.
    pub reserved: bool,
}

/// How the attempt that prepared a reserved assignment ended.
#[derive(Debug)]
pub(crate) enum AssignmentOutcome {
    /// The upstream answered on this credential: the target is established.
    Activated,
    /// Preparation did not complete. `uncertain` when a send may have had
    /// upstream effects the next generation must not repeat blindly.
    Failed { uncertain: bool, error: String },
}

/// Why an active assignment's credential cannot serve this request.
enum Unusable {
    /// Not permitted, dead, disabled, retired or quota-blocked: switch.
    Durable { exhausted_cycle_id: Option<String> },
    /// Rate limited, cooling down or excluded on this request only.
    Transient,
}

impl<C: BatchConnectionTrait> Core<C> {
    /// Session-bound choice among `eligible`. `chosen` is the strategy's pick,
    /// used when the session needs a new target. Returns the eligible index to
    /// use and its assignment, or None when the request runs unbound.
    pub(crate) async fn session_pick(
        &self,
        request: &RequestContext,
        eligible: &[(Arc<CredentialData>, CredentialBlocks)],
        chosen: usize,
        now_ms: i64,
    ) -> CoreResult<Option<(usize, AssignmentHandle)>> {
        let Some(session_id) = request
            .session
            .as_ref()
            .and_then(|s| s.agent_session_id.as_deref())
        else {
            return Ok(None);
        };
        let store = self.store();
        let active = store
            .agent_sessions()
            .current_assignments_many(&[session_id.to_owned()], now_ms)
            .await?
            .into_iter()
            .next()
            .flatten();
        let mut exhausted_cycle_id = None;
        if let Some(active) = &active {
            if let Some(index) = eligible
                .iter()
                .position(|(c, _)| c.id == active.credential_id)
            {
                return Ok(Some((
                    index,
                    AssignmentHandle {
                        reference: AgentAssignmentRef {
                            session_id: session_id.to_owned(),
                            assignment_id: active.id.clone(),
                            generation: active.generation,
                        },
                        reserved: false,
                    },
                )));
            }
            match self.why_unusable(request, active, now_ms).await? {
                Unusable::Transient => return Ok(None),
                Unusable::Durable {
                    exhausted_cycle_id: cycle,
                } => exhausted_cycle_id = cycle,
            }
        }
        let Some((credential, _)) = eligible.get(chosen) else {
            return Ok(None);
        };
        for _ in 0..RESERVE_RETRIES {
            let Some(row) = store
                .agent_sessions()
                .get_many(&[session_id.to_owned()])
                .await?
                .into_iter()
                .next()
                .flatten()
            else {
                return Ok(None);
            };
            match row.state {
                AgentSessionState::Closed => return Ok(None),
                AgentSessionState::Switching => {
                    // Another request is preparing a target; converge on it
                    // when it is usable here, otherwise run unbound.
                    return Ok(self.converge(session_id, &row, eligible).await?.or(None));
                }
                _ => {}
            }
            let (reason, previous_generation) = match (&active, row.active_generation) {
                (Some(active), Some(generation)) if active.generation == generation => {
                    (AssignmentReason::QuotaExhausted, Some(generation))
                }
                (None, None) => (AssignmentReason::Initial, None),
                // The row moved under us; read it again.
                _ => continue,
            };
            let assignment_id = ids::random_id();
            let outcome = store
                .agent_sessions()
                .reserve_assignments_many(vec![AssignmentReservation {
                    id: assignment_id.clone(),
                    session_id: session_id.to_owned(),
                    expected_version: row.version,
                    previous_generation,
                    provider_id: credential.provider_id.clone(),
                    credential_id: credential.id.clone(),
                    reason,
                    exhausted_cycle_id: exhausted_cycle_id.clone(),
                    trigger_request_id: Some(request.request_id.clone()),
                    now_ms,
                }])
                .await?;
            if matches!(outcome.first(), Some(CasOutcome::Applied)) {
                return Ok(Some((
                    chosen,
                    AssignmentHandle {
                        reference: AgentAssignmentRef {
                            session_id: session_id.to_owned(),
                            assignment_id,
                            generation: row.version + 1,
                        },
                        reserved: true,
                    },
                )));
            }
        }
        Ok(None)
    }

    async fn converge(
        &self,
        session_id: &str,
        row: &agent_session::Model,
        eligible: &[(Arc<CredentialData>, CredentialBlocks)],
    ) -> CoreResult<Option<(usize, AssignmentHandle)>> {
        let Some(pending_id) = row.pending_assignment_id.as_deref() else {
            return Ok(None);
        };
        let Some(pending) = self
            .store()
            .agent_assignments()
            .get_many(&[pending_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
        else {
            return Ok(None);
        };
        Ok(eligible
            .iter()
            .position(|(c, _)| c.id == pending.credential_id)
            .map(|index| {
                (
                    index,
                    AssignmentHandle {
                        reference: AgentAssignmentRef {
                            session_id: session_id.to_owned(),
                            assignment_id: pending.id,
                            generation: pending.generation,
                        },
                        reserved: false,
                    },
                )
            }))
    }

    async fn why_unusable(
        &self,
        request: &RequestContext,
        active: &agent_assignment::Model,
        now_ms: i64,
    ) -> CoreResult<Unusable> {
        let Some(credential) = request
            .target
            .credentials
            .iter()
            .find(|c| c.id == active.credential_id)
        else {
            return Ok(Unusable::Durable {
                exhausted_cycle_id: None,
            });
        };
        let version = credential.state.load();
        if !credential.enabled
            || credential.state.is_retired()
            || version.status == CredentialStatus::Dead
        {
            return Ok(Unusable::Durable {
                exhausted_cycle_id: None,
            });
        }
        let mut blocks = self
            .read_blocks(&credential.provider_id, &credential.id)
            .await?;
        let model = request.target.upstream_model.as_deref();
        blocks.retain_enforced(&request.target.provider, credential, model);
        Ok(
            match blocks
                .blocked_by(model, request.operation.operation, now_ms)
                .map(|block| &block.source)
            {
                Some(BlockSource::QuotaExhausted { cycle_id, .. }) => Unusable::Durable {
                    exhausted_cycle_id: cycle_id.clone(),
                },
                Some(BlockSource::Counted { .. }) => Unusable::Durable {
                    exhausted_cycle_id: None,
                },
                _ => Unusable::Transient,
            },
        )
    }

    /// Settle a reservation from its preparing attempt. Not reserved here:
    /// nothing to do. A lost CAS means a peer already settled it.
    pub(crate) async fn settle_assignment(
        &self,
        handle: &AssignmentHandle,
        outcome: AssignmentOutcome,
        now_ms: i64,
    ) -> CoreResult<()> {
        if !handle.reserved {
            return Ok(());
        }
        let activation = AssignmentActivation {
            session_id: handle.reference.session_id.clone(),
            assignment_id: handle.reference.assignment_id.clone(),
            expected_version: handle.reference.generation,
            generation: handle.reference.generation,
            now_ms,
        };
        let sessions = self.store().agent_sessions();
        match outcome {
            AssignmentOutcome::Activated => {
                sessions.activate_assignments_many(vec![activation]).await?;
            }
            AssignmentOutcome::Failed { uncertain, error } => {
                sessions
                    .fail_assignments_many(vec![AssignmentFailure {
                        activation,
                        uncertain,
                        error,
                    }])
                    .await?;
            }
        }
        Ok(())
    }
}
