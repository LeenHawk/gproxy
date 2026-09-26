//! Vendor service dispatch: the CLI calls a channel exposes through
//! `ChannelServices` (profile, usage, plugins, remote control, ...) that have
//! no `OperationKey`. The upper layer resolves the provider and the permitted
//! credentials exactly as for an operation; core validates the requested
//! view against the caller's role, picks the credentials the channel may use,
//! refreshes material about to expire, and supplies the facts a synthesized
//! view is rendered from.
//!
//! Three views (`ServiceView`):
//! * `Caller` — the caller's own picture from the gateway's accounting for
//!   the explicit user ID and the resources bound to `scope`. Never reflects any
//!   credential's real state; the only view a `Member` may use.
//! * `Pool` — the target's credentials as one synthesized account: usage
//!   merged across them, resources bound to any of them, a synthetic
//!   identity derived from the provider and the credential set.
//! * `Credential(id)` — one named credential, forwarded raw with its own
//!   authentication; sees the real account.
//!
//! Core never decides who is an admin of what. The host expresses the
//! organization boundary by choosing `target.credentials`: `CallerRole::Admin`
//! means "admin over the credentials in this target", `Pool` aggregates
//! that set only, and `Credential(id)` must name one of them.
//!
//! Services run outside the observation funnel: no attempt record, no usage
//! settlement, no capture, no retry on another credential and no session
//! affinity. They are account plumbing for the CLI, not model traffic.

use crate::{
    BudgetOwner, Core, CoreError, CoreResult, CredentialBlocks, CredentialData, CredentialStatus,
    CredentialVersion, ExecutionTarget, RefreshMode, api::lifecycle::now_ms, ids,
};
pub use gproxy_channel::channel::{CallerRole, ServiceView};
use gproxy_channel::{
    ChannelError,
    channel::{
        CallerIdentity, CallerUsage, CallerUsageWindow, CredentialContext, OperationFuture,
        QuotaScope, ResourceBindingRecord, ServiceCaller, ServiceContext,
    },
};
use gproxy_protocol::{HttpBody, WireRequest, WireResponse, capability::UpstreamConnection};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::{resource::resource_binding, usage::usage_record};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

/// One vendor service call. `B` is `HttpBody` for `call_service` and `()`
/// for `connect_service`; a socket is never an HTTP body.
pub struct ServiceRequest<B = HttpBody> {
    /// Opaque isolation scope, as for `RequestContext::scope`; used for
    /// resource bindings and the synthesized identity, never as a user ID.
    pub scope: String,
    /// Authenticated user whose historical usage the Caller view may read.
    /// Supply the same ID as `RequestContext::attribution.user_id` on model
    /// requests. None returns no token/cost history; budget windows still work.
    /// Ignored by Pool and Credential views.
    pub user_id: Option<String>,
    /// The caller's role over `target.credentials`, decided by the host.
    pub caller: CallerRole,
    pub view: ServiceView,
    /// The admitted provider and credentials. `upstream_model` is ignored:
    /// a service has no model.
    pub target: ExecutionTarget,
    /// The owners whose budgets the `Caller` view reports as usage windows,
    /// as `RequestContext::budgets` names them for model traffic. Empty
    /// means the view shows no windows.
    pub budgets: Vec<BudgetOwner>,
    /// The client's request as received, vendor path included.
    pub request: WireRequest<B>,
}

/// A block that makes the credential unusable for everything, not only for
/// one model or operation: services have neither.
fn blocked_credential_wide(blocks: &CredentialBlocks, now_ms: i64) -> bool {
    blocks.blocks.iter().any(|block| {
        block.until_ms > now_ms
            && block.operation.is_none()
            && matches!(block.scope, QuotaScope::All | QuotaScope::Unknown)
    })
}

fn host_error(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::Host(error.to_string())
}

/// Which facts back a synthesized view.
enum Facts {
    /// `resource_bindings.scope`, usage rows attributed to the user, and
    /// the budgets of the named owners.
    Scope {
        scope: String,
        user_id: Option<String>,
        budgets: Vec<BudgetOwner>,
    },
    /// Bindings and quota cycles of the target's credentials.
    Pool,
}

/// The `ServiceCaller` core supplies. Under `Facts::Scope` it is the caller's
/// own picture; under `Facts::Pool` the target's credentials merged. Both
/// are limited to `credential_ids`, the host-chosen boundary.
pub struct TargetCaller<'a, C> {
    core: &'a Core<C>,
    role: CallerRole,
    identity: CallerIdentity,
    facts: Facts,
    provider_id: String,
    credential_ids: Vec<String>,
}

impl<'a, C> TargetCaller<'a, C> {
    fn new(core: &'a Core<C>, request: &ServiceRequest<impl Sized>) -> Self {
        let provider = &request.target.provider.entity;
        let mut credential_ids = request
            .target
            .credentials
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>();
        credential_ids.sort_unstable();
        credential_ids.dedup();
        let (identity, facts) = match request.view {
            ServiceView::Caller => (
                CallerIdentity {
                    id: request.scope.clone(),
                    display_name: None,
                },
                Facts::Scope {
                    scope: request.scope.clone(),
                    user_id: request.user_id.clone(),
                    budgets: request.budgets.clone(),
                },
            ),
            // The pool identity names the provider and the exact credential
            // set, so two admins over different subsets get different ids.
            ServiceView::Pool | ServiceView::Credential(_) => (
                CallerIdentity {
                    id: format!("{}:{}", provider.id, credential_ids.join(",")),
                    display_name: Some(provider.name.clone()),
                },
                Facts::Pool,
            ),
        };
        Self {
            core,
            role: request.caller,
            identity,
            facts,
            provider_id: provider.id.clone(),
            credential_ids,
        }
    }
}

/// Percentages seen, earliest start and earliest reset of one dimension.
type WindowAccumulator = (Vec<f64>, Option<i64>, Option<i64>);

fn record_of(row: resource_binding::Model) -> ResourceBindingRecord {
    ResourceBindingRecord {
        upstream_id: row.upstream_id.unwrap_or(row.public_id),
        kind: row.kind,
        credential_id: row.credential_id,
        summary: row.summary,
    }
}

/// Token counts a host wrote into `usage_records.metrics`, read leniently:
/// `input_tokens`/`output_tokens` at the top level or under `tokens`.
fn tokens_of(metrics: &Value) -> (u64, u64) {
    let read = |name: &str| {
        metrics
            .get(name)
            .or_else(|| metrics.pointer(&format!("/tokens/{name}")))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    (read("input_tokens"), read("output_tokens"))
}

impl<C: BatchConnectionTrait + Send + Sync> TargetCaller<'_, C> {
    fn binding_query(&self, kind: &str) -> sea_orm::Select<resource_binding::Entity> {
        let query = resource_binding::Entity::find()
            .filter(resource_binding::Column::ProviderId.eq(self.provider_id.as_str()))
            .filter(resource_binding::Column::Kind.eq(kind))
            .filter(resource_binding::Column::Generation.eq(0i64))
            .filter(resource_binding::Column::UpstreamId.is_not_null());
        match &self.facts {
            Facts::Scope { scope, .. } => {
                query.filter(resource_binding::Column::Scope.eq(scope.as_str()))
            }
            Facts::Pool => query
                .filter(resource_binding::Column::CredentialId.is_in(self.credential_ids.clone())),
        }
    }

    /// User-wide token totals and settled cost use the same explicit user ID
    /// as StoreObserver attribution. The opaque scope only partitions resources.
    /// Budget windows are independently selected by the request's owner chain.
    async fn caller_usage(
        &self,
        user_id: Option<&str>,
        budgets: &[BudgetOwner],
    ) -> Result<CallerUsage, ChannelError> {
        let rows = match user_id {
            Some(user_id) => self
                .core
                .store()
                .usage_records()
                .query(
                    usage_record::Entity::find()
                        .filter(usage_record::Column::UserId.eq(user_id))
                        .filter(
                            usage_record::Column::Operation.is_in(
                                gproxy_protocol::Operation::usage_operations()
                                    .map(gproxy_protocol::Operation::id),
                            ),
                        ),
                )
                .await
                .map_err(host_error)?,
            None => Vec::new(),
        };
        let mut usage = CallerUsage::default();
        let mut cost = Decimal::ZERO;
        let mut costed = false;
        for row in rows {
            let (input, output) = tokens_of(&row.metrics);
            usage.input_tokens = usage.input_tokens.saturating_add(input);
            usage.output_tokens = usage.output_tokens.saturating_add(output);
            if let Some(value) = row.cost {
                cost += value.decimal();
                costed = true;
            }
        }
        usage.cost = costed.then(|| cost.to_string());
        usage.windows = self
            .core
            .budget_status(budgets, now_ms())
            .await
            .map_err(host_error)?
            .into_iter()
            .map(|status| CallerUsageWindow {
                key: status.window_key,
                used_percent: (status.limit > Decimal::ZERO)
                    .then(|| status.used / status.limit * Decimal::ONE_HUNDRED)
                    .and_then(|p| p.to_f64()),
                period_start_ms: Some(status.starts_at_ms),
                reset_at_ms: status.resets_at_ms,
            })
            .collect();
        Ok(usage)
    }

    /// The pool merged: for each window, the open cycle of every credential
    /// in the target that carries an upstream reading, averaged into one
    /// window with the earliest start and reset. Usage rows carry no
    /// credential, so token totals stay zero.
    async fn pool_usage(&self) -> Result<CallerUsage, ChannelError> {
        let now = now_ms();
        let rows = self
            .core
            .store()
            .credential_cycles()
            .open_of(&self.credential_ids)
            .await
            .map_err(host_error)?;
        let mut merged: BTreeMap<String, WindowAccumulator> = BTreeMap::new();
        // A cycle without a sample is a local guess nobody reported on, and
        // an ended one is history the next reading will close.
        for row in rows
            .into_iter()
            .filter(|row| row.sample_at_ms.is_some() && row.ends_at_ms.is_none_or(|end| end > now))
        {
            let entry = merged.entry(row.window_id).or_default();
            if let Some(percent) = row.sample_used_percent.and_then(|p| p.decimal().to_f64()) {
                entry.0.push(percent);
            }
            entry.1 = match (entry.1, Some(row.starts_at_ms)) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            entry.2 = match (entry.2, row.ends_at_ms) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        Ok(CallerUsage {
            windows: merged
                .into_iter()
                .map(|(key, (percents, start, reset))| CallerUsageWindow {
                    key,
                    used_percent: (!percents.is_empty())
                        .then(|| percents.iter().sum::<f64>() / percents.len() as f64),
                    period_start_ms: start,
                    reset_at_ms: reset,
                })
                .collect(),
            ..CallerUsage::default()
        })
    }
}

impl<C: BatchConnectionTrait + Send + Sync> ServiceCaller for TargetCaller<'_, C> {
    fn role(&self) -> CallerRole {
        self.role
    }

    fn identity(&self) -> &CallerIdentity {
        &self.identity
    }

    fn usage<'a>(&'a self) -> OperationFuture<'a, CallerUsage> {
        Box::pin(async move {
            match &self.facts {
                Facts::Scope {
                    user_id, budgets, ..
                } => self.caller_usage(user_id.as_deref(), budgets).await,
                Facts::Pool => self.pool_usage().await,
            }
        })
    }

    fn find_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> OperationFuture<'a, Option<ResourceBindingRecord>> {
        Box::pin(async move {
            let rows = self
                .core
                .store()
                .resource_bindings()
                .query(
                    self.binding_query(kind)
                        .filter(resource_binding::Column::UpstreamId.eq(upstream_id)),
                )
                .await
                .map_err(host_error)?;
            Ok(rows.into_iter().next().map(record_of))
        })
    }

    fn list_bindings<'a>(
        &'a self,
        kind: &'a str,
    ) -> OperationFuture<'a, Vec<ResourceBindingRecord>> {
        Box::pin(async move {
            let rows = self
                .core
                .store()
                .resource_bindings()
                .query(self.binding_query(kind))
                .await
                .map_err(host_error)?;
            Ok(rows.into_iter().map(record_of).collect())
        })
    }

    fn save_binding<'a>(&'a self, record: ResourceBindingRecord) -> OperationFuture<'a, ()> {
        Box::pin(async move {
            let scope = match &self.facts {
                Facts::Scope { scope, .. } => scope.clone(),
                // A pool-created resource belongs to the pool, keyed by the
                // provider so any admin over the credential sees it.
                Facts::Pool => format!("pool:{}", self.provider_id),
            };
            let now = now_ms();
            self.core
                .store()
                .resource_bindings()
                .create_many(vec![resource_binding::ActiveModel {
                    id: Set(ids::random_id()),
                    scope: Set(scope),
                    kind: Set(record.kind),
                    public_id: Set(record.upstream_id.clone()),
                    generation: Set(0),
                    assignment_id: Set(None),
                    provider_id: Set(self.provider_id.clone()),
                    upstream_id: Set(Some(record.upstream_id)),
                    user_id: Set(None),
                    credential_id: Set(record.credential_id),
                    parent_binding_id: Set(None),
                    secret: Set(None),
                    summary: Set(record.summary),
                    file_id: Set(None),
                    created_at_ms: Set(now),
                    updated_at_ms: Set(now),
                    expires_at_ms: Set(None),
                }])
                .await
                .map_err(host_error)?;
            Ok(())
        })
    }

    fn delete_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> OperationFuture<'a, ()> {
        Box::pin(async move {
            let bindings = self.core.store().resource_bindings();
            let ids = bindings
                .query(
                    self.binding_query(kind)
                        .filter(resource_binding::Column::UpstreamId.eq(upstream_id)),
                )
                .await
                .map_err(host_error)?
                .into_iter()
                .map(|row| row.id)
                .collect::<Vec<_>>();
            if !ids.is_empty() {
                bindings.delete_many(&ids).await.map_err(host_error)?;
            }
            Ok(())
        })
    }
}

/// The credentials a call may use: every enabled, live one of the target,
/// the selected one first.
struct Selected {
    /// Index into `usable` of the credential the call runs with.
    index: usize,
    usable: Vec<(Arc<CredentialData>, Arc<CredentialVersion>)>,
}

impl<C: BatchConnectionTrait + Send + Sync> Core<C> {
    /// The usable credentials of the target and the one this call runs with:
    /// under `Credential(id)` the named one; otherwise the first that is not
    /// blocked credential-wide, as `select_credential` would pick without
    /// strategy or affinity. Material about to expire is refreshed first.
    ///
    /// Takes the target and the view rather than the whole `ServiceRequest`,
    /// which carries the client's `HttpBody`: a streaming body is `Send` but
    /// not `Sync`, so a `&ServiceRequest<HttpBody>` held across the refresh
    /// and block-lookup awaits below would make `call_service` non-`Send` and
    /// unspawnable by a host.
    async fn service_credentials(
        &self,
        target: &ExecutionTarget,
        view: &ServiceView,
    ) -> CoreResult<Selected> {
        let now = now_ms();
        let provider_id = &target.provider.entity.id;
        let mut usable = Vec::new();
        let mut selected = None;
        let mut any_candidate = false;
        let mut dead: Option<&Arc<CredentialData>> = None;
        for credential in &target.credentials {
            if credential.provider_id != *provider_id
                || !credential.enabled
                || credential.state.is_retired()
            {
                continue;
            }
            any_candidate = true;
            let named = matches!(view, ServiceView::Credential(id) if *id == credential.id);
            let wanted =
                named || (selected.is_none() && !matches!(view, ServiceView::Credential(_)));
            // Material about to expire is refreshed before it is used; a
            // failed refresh still lets the call try the current material.
            if wanted
                && crate::refresh::needs_refresh(&credential.state.load(), now)
                && target.provider.channel.credential_refresh().is_some()
            {
                let _ = self
                    .refresh_credential(
                        &credential.provider_id,
                        &credential.id,
                        RefreshMode::IfNeeded,
                    )
                    .await;
            }
            let version = credential.state.load();
            if version.status == CredentialStatus::Dead {
                if named {
                    return Err(CoreError::CredentialDead {
                        credential_id: credential.id.clone(),
                        reason: version.status_reason.clone(),
                    });
                }
                dead.get_or_insert(credential);
                continue;
            }
            let index = usable.len();
            usable.push((credential.clone(), version));
            if named {
                selected = Some(index);
            } else if selected.is_none() && !matches!(view, ServiceView::Credential(_)) {
                let blocks = self
                    .read_blocks(&credential.provider_id, &credential.id)
                    .await?;
                if !blocked_credential_wide(&blocks, now) {
                    selected = Some(index);
                }
            }
        }
        match selected {
            Some(index) => Ok(Selected { index, usable }),
            None => match (view, dead) {
                (ServiceView::Credential(id), _) => Err(CoreError::InvalidTarget(format!(
                    "credential `{id}` is not a usable credential of this target"
                ))),
                (_, Some(dead)) if any_candidate => {
                    let version = dead.state.load();
                    Err(CoreError::CredentialDead {
                        credential_id: dead.id.clone(),
                        reason: version.status_reason.clone(),
                    })
                }
                _ => Err(CoreError::NoUsableCredential),
            },
        }
    }

    fn check_view(request: &ServiceRequest<impl Sized>) -> CoreResult<()> {
        match (&request.view, request.caller) {
            (ServiceView::Caller, _) | (_, CallerRole::Admin) => Ok(()),
            (ServiceView::Pool, CallerRole::Member) => Err(CoreError::Forbidden(
                "the pool view is available to admins of the target only",
            )),
            (ServiceView::Credential(_), CallerRole::Member) => Err(CoreError::Forbidden(
                "the credential view is available to admins of the target only",
            )),
        }
    }

    /// Call a vendor HTTP service under the requested view. The channel's
    /// answer is returned as-is (streaming bodies and non-2xx included); a
    /// channel without services is `Channel(UnsupportedService)`, a view the
    /// role may not use is `Forbidden`. Runs outside the observation funnel:
    /// no usage, no capture, no retry.
    pub async fn call_service(
        &self,
        request: ServiceRequest,
    ) -> CoreResult<WireResponse<HttpBody>> {
        let Some(services) = request.target.provider.channel.services() else {
            return Err(CoreError::Channel(ChannelError::UnsupportedService));
        };
        Self::check_view(&request)?;
        let selected = self
            .service_credentials(&request.target, &request.view)
            .await?;
        let caller = TargetCaller::new(self, &request);
        let provider = crate::assemble::provider_view(&request.target.provider.entity);
        let accounts = selected
            .usable
            .iter()
            .map(|(credential, version)| CredentialContext {
                provider,
                credential: crate::execute::prepare::credential_view(credential, version),
                client: credential.client.as_ref(),
            })
            .collect::<Vec<_>>();
        services
            .call(ServiceContext {
                account: accounts[selected.index],
                accounts: &accounts,
                caller: &caller,
                view: request.view.clone(),
                request: request.request,
            })
            .await
            .map_err(CoreError::Channel)
    }

    /// Open a vendor WebSocket service under the requested view, through the
    /// credentials' HTTP/1.1 clients. Same validation and error mapping as
    /// `call_service`; equally outside the funnel.
    pub async fn connect_service(
        &self,
        request: ServiceRequest<()>,
    ) -> CoreResult<UpstreamConnection> {
        let Some(services) = request.target.provider.channel.services() else {
            return Err(CoreError::Channel(ChannelError::UnsupportedService));
        };
        Self::check_view(&request)?;
        let selected = self
            .service_credentials(&request.target, &request.view)
            .await?;
        let caller = TargetCaller::new(self, &request);
        let provider = crate::assemble::provider_view(&request.target.provider.entity);
        let accounts = selected
            .usable
            .iter()
            .map(|(credential, version)| CredentialContext {
                provider,
                credential: crate::execute::prepare::credential_view(credential, version),
                client: credential.websocket_client.as_ref(),
            })
            .collect::<Vec<_>>();
        services
            .connect(ServiceContext {
                account: accounts[selected.index],
                accounts: &accounts,
                caller: &caller,
                view: request.view.clone(),
                request: request.request,
            })
            .await
            .map_err(CoreError::Channel)
    }
}
