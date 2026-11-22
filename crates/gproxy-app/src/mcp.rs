//! The MCP gateway: a caller's request to `/connect/mcp`, served from the MCP
//! servers registered as providers.
//!
//! This is where `gproxy-mcp`'s pieces meet the instance: which servers a
//! caller has a credential for, which credential is used, the session each
//! server is addressed through, the cached tool lists, the scope rules and
//! the audit row. The caller only ever presents its gateway key; signing in
//! to a server and keeping that sign-in alive is the gateway's business.

use std::{sync::Arc, time::Duration};

use gproxy_core::{
    CoreData, CredentialData, CredentialStatus, ProtocolState, ProviderData, RefreshMode,
    ScopedState, StateScope,
};
use gproxy_mcp::{
    Tool,
    audit::{Outcome, ToolCall},
    binding::{self, Binding},
    client::{self, Answer, CallError, Context, Mode, Target},
    server::{self, Inbound, Reply},
    tool::{self, Nature},
};
use gproxy_protocol::connection::Bytes;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::audit_event;
use gproxy_upstream::{CredentialView, UpstreamKind};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};

use crate::{
    App, AppData, AppError, AuditEntry, Caller, Result,
    admission::{credential::visible_credentials, permission::require_oauth_scope},
    audit::Audit,
    dto::McpToolDto,
    snapshot::Owner,
};

/// What an OAuth-grant caller's token has to carry to use MCP at all.
pub const OAUTH_SCOPE: &str = "gproxy:mcp";
/// The `action` of a tool call's audit row.
pub const AUDIT_ACTION: &str = "mcp.tools.call";
/// How long a server's tool list is reused before it is asked again.
const TOOLS_TTL: Duration = Duration::from_secs(300);

/// The upstream kind a host registers to have MCP providers at all.
pub fn kind() -> Arc<dyn UpstreamKind> {
    Arc::new(gproxy_mcp::Mcp)
}

/// Which servers a request addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpEndpoint {
    /// `/connect/mcp/{name}`: one server, its tools under their own names.
    Server(String),
    /// `/connect/mcp`: every server the caller can use, or the named ones,
    /// their tools as `{server}__{tool}`.
    All { servers: Option<Vec<String>> },
}

pub struct McpRequest {
    pub endpoint: McpEndpoint,
    pub method: Method,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub source_ip: Option<String>,
}

/// One registered server and the credentials usable for it, best first.
struct Server {
    name: String,
    provider: Arc<ProviderData>,
    credentials: Vec<Arc<CredentialData>>,
}

impl Server {
    fn endpoint(&self) -> &str {
        // `servers` only yields providers that have one.
        self.provider.entity.base_url.as_deref().unwrap_or_default()
    }
}

/// A server reached with one credential.
struct Connection {
    credential: Arc<CredentialData>,
    headers: HeaderMap,
    mode: Mode,
}

/// Why a server gave no answer. The text reaches the caller, so it names the
/// server and never anything the server said about credentials.
struct Failure(String);

type Served<T> = std::result::Result<T, Failure>;

fn is_mcp(provider: &ProviderData) -> bool {
    provider.upstream().id() == gproxy_mcp::ID
}

fn live(credential: &CredentialData) -> bool {
    credential.enabled && credential.state.load().status == CredentialStatus::Active
}

fn headers(provider: &ProviderData, credential: &CredentialData) -> Served<HeaderMap> {
    let version = credential.state.load();
    gproxy_mcp::request_headers(
        provider.view(),
        CredentialView {
            id: &credential.id,
            provider_id: &credential.provider_id,
            auth_kind: &credential.auth_kind,
            secret: &version.secret,
            metadata: &version.metadata,
            version: version.version,
            expires_at_ms: version.expires_at_ms,
        },
    )
    .map_err(|error| Failure(error.to_string()))
}

/// One caller-facing session over the instance's servers.
struct Gateway<'a, C> {
    app: &'a App<C>,
    core: Arc<CoreData>,
    /// The key bindings are stored under: the caller and its session.
    session: String,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Gateway<'_, C> {
    fn state(&self, server: &Server) -> ScopedState<C> {
        ScopedState::new(
            ProtocolState::new(self.app.gproxy().core(), self.core.limits.capability(None)),
            StateScope {
                scope: "mcp".to_owned(),
                provider_id: server.provider.entity.id.clone(),
                conversation: None,
            },
        )
    }

    fn failure(server: &Server, error: impl std::fmt::Display) -> Failure {
        Failure(format!("MCP server `{}`: {error}", server.name))
    }

    /// One exchange with a server, bounded by the provider's request timeout.
    async fn bounded<T>(
        &self,
        server: &Server,
        exchange: impl Future<Output = std::result::Result<T, CallError>>,
    ) -> std::result::Result<T, CallError> {
        let limits = self.core.limits.for_provider(&server.provider.entity);
        tokio::time::timeout(limits.request_timeout, exchange)
            .await
            .unwrap_or_else(|_| Err(CallError::Invalid("no answer in time".to_owned())))
    }

    async fn refresh(
        &self,
        server: &Server,
        credential: &CredentialData,
        mode: RefreshMode,
    ) -> Served<()> {
        self.app
            .gproxy()
            .core()
            .refresh_credential(&server.provider.entity.id, &credential.id, mode)
            .await
            .map(drop)
            .map_err(|error| {
                tracing::warn!(%error, credential = %credential.id, "MCP credential refresh failed");
                Self::failure(
                    server,
                    "the gateway's credential was refused and could not be renewed; sign in to the server again",
                )
            })
    }

    /// Open the server with `credential`, renewing the credential once when
    /// the server refuses it.
    async fn open(
        &self,
        server: &Server,
        credential: &CredentialData,
    ) -> Served<(HeaderMap, Mode)> {
        let mut sent = headers(&server.provider, credential)?;
        for renewed in [false, true] {
            let target = Target {
                client: credential.client.as_ref(),
                endpoint: server.endpoint(),
                headers: &sent,
            };
            match self.bounded(server, client::open(target)).await {
                Ok(mode) => return Ok((sent, mode)),
                Err(CallError::Unauthorized) if !renewed => {
                    self.refresh(server, credential, RefreshMode::UnauthorizedRecovery)
                        .await?;
                    sent = headers(&server.provider, credential)?;
                }
                Err(error) => return Err(Self::failure(server, error)),
            }
        }
        unreachable!("the second attempt returns")
    }

    /// The caller's connection to a server: the one its session is bound to,
    /// or a new one with the best credential, recorded for the next request
    /// on any instance.
    async fn connect(&self, server: &Server) -> Served<Connection> {
        let state = self.state(server);
        let stored = |error| Self::failure(server, format!("session storage failed: {error}"));
        // Two rounds: losing the race to record a binding means reading the winner's.
        for _ in 0..2 {
            if let Some(found) = binding::load(&state, &self.session).await.map_err(stored)? {
                let bound = server
                    .credentials
                    .iter()
                    .find(|credential| credential.id == found.binding.credential_id);
                match bound {
                    Some(credential) => {
                        self.refresh(server, credential, RefreshMode::IfNeeded)
                            .await?;
                        return Ok(Connection {
                            headers: headers(&server.provider, credential)?,
                            credential: credential.clone(),
                            mode: found.binding.mode,
                        });
                    }
                    // The credential the session started with is gone.
                    None => binding::remove(&state, &self.session, found)
                        .await
                        .map_err(stored)?,
                }
            }
            let credential = &server.credentials[0];
            self.refresh(server, credential, RefreshMode::IfNeeded)
                .await?;
            let (sent, mode) = self.open(server, credential).await?;
            let binding = Binding {
                credential_id: credential.id.clone(),
                mode: mode.clone(),
            };
            if binding::create(&state, &self.session, binding)
                .await
                .map_err(stored)?
                .is_some()
            {
                return Ok(Connection {
                    credential: credential.clone(),
                    headers: sent,
                    mode,
                });
            }
            let target = Target {
                client: credential.client.as_ref(),
                endpoint: server.endpoint(),
                headers: &sent,
            };
            client::close(target, &mode).await;
        }
        Err(Self::failure(server, "the session could not be opened"))
    }

    /// One request on the caller's connection. A refused credential is
    /// renewed and a session the server ended is reopened, once each.
    async fn request(
        &self,
        server: &Server,
        connection: &mut Connection,
        context: &Context<'_>,
        method: &str,
        params: &Value,
    ) -> Served<Answer> {
        let mut recovered = false;
        loop {
            let target = Target {
                client: connection.credential.client.as_ref(),
                endpoint: server.endpoint(),
                headers: &connection.headers,
            };
            let outcome = self
                .bounded(
                    server,
                    client::request(target, &connection.mode, context, method, params.clone()),
                )
                .await;
            match outcome {
                Ok(answer) => return Ok(answer),
                Err(CallError::Unauthorized) if !recovered => {
                    self.refresh(
                        server,
                        &connection.credential,
                        RefreshMode::UnauthorizedRecovery,
                    )
                    .await?;
                    connection.headers = headers(&server.provider, &connection.credential)?;
                }
                Err(CallError::SessionExpired) if !recovered => {
                    let state = self.state(server);
                    if let Ok(Some(found)) = binding::load(&state, &self.session).await {
                        let _ = binding::remove(&state, &self.session, found).await;
                    }
                    *connection = self.connect(server).await?;
                }
                Err(error) => return Err(Self::failure(server, error)),
            }
            recovered = true;
        }
    }

    /// The server's tools, from the cache when it has them.
    async fn tools(
        &self,
        server: &Server,
        connection: &mut Connection,
        fresh: bool,
    ) -> Served<Vec<Tool>> {
        let cache = self.app.gproxy().cache();
        let key = format!(
            "mcp:tools:{}:{}",
            server.provider.entity.id, connection.credential.id
        );
        if !fresh
            && let Ok(Some(entry)) = cache.get(&key).await
            && let Ok(tools) = serde_json::from_slice(&entry.value)
        {
            return Ok(tools);
        }
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        // A server that never stops paging is not followed forever.
        for _ in 0..50 {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let context = Context { capabilities: None };
            let result = self
                .request(server, connection, &context, "tools/list", &params)
                .await?
                .map_err(|error| Self::failure(server, format!("tools/list failed: {error}")))?;
            let (page, next) =
                client::tools_page(&result).map_err(|error| Self::failure(server, error))?;
            tools.extend(page);
            cursor = next;
            if cursor.is_none() {
                break;
            }
        }
        if let Ok(encoded) = serde_json::to_vec(&tools)
            && let Err(error) = cache.put(&key, encoded, TOOLS_TTL).await
        {
            tracing::warn!(%error, "MCP tool list could not be cached");
        }
        Ok(tools)
    }
}

/// How readily `caller` uses a credential of this owner, lowest first: its
/// own, then its team's, its organization's, and the instance's shared
/// ones. `None` for anyone else's: an administrator sees every credential
/// so as to manage it, and a tool call made with another person's sign-in
/// would read that person's account.
fn preference(owner: Option<&Owner>, caller: &Caller, organization: Option<&str>) -> Option<u8> {
    match owner {
        Some(Owner::User(user)) if *user == caller.user_id => Some(0),
        Some(Owner::Team(team)) if caller.team_id.as_deref() == Some(team) => Some(1),
        Some(Owner::Org(org)) if organization == Some(org.as_str()) => Some(2),
        Some(Owner::Shared) => Some(3),
        _ => None,
    }
}

/// The servers `caller` has a usable credential for, by name.
fn servers(data: &AppData, core: &CoreData, caller: &Caller) -> Vec<Server> {
    let organization =
        data.effective_organization(caller.organization_id.as_deref(), caller.team_id.as_deref());
    let rank = |credential: &CredentialData| {
        let owner = data.credential_ownership.owner(&credential.id);
        preference(owner, caller, organization)
    };
    let mut found: Vec<Server> = core
        .providers
        .values()
        .filter(|provider| is_mcp(provider) && provider.entity.base_url.is_some())
        .filter_map(|provider| {
            let visible = visible_credentials(
                data,
                caller,
                &provider.credential_ids.iter().cloned().collect(),
            );
            let mut credentials: Vec<_> = visible
                .iter()
                .filter_map(|id| core.credentials.get(id))
                .filter(|credential| live(credential) && rank(credential).is_some())
                .cloned()
                .collect();
            credentials.sort_by_key(|credential| (rank(credential), credential.id.clone()));
            (!credentials.is_empty()).then(|| Server {
                name: provider.entity.name.clone(),
                provider: provider.clone(),
                credentials,
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// A tool the caller may use, under the name it is offered by.
struct Offered {
    tool: Tool,
    name: String,
    nature: Nature,
}

fn nature(data: &AppData, server: &Server, tool: &Tool) -> Nature {
    data.mcp
        .override_of(&server.provider.entity.id, &tool.name)
        .map_or_else(
            || Nature::declared(tool.annotations.as_ref()),
            |found| found.0,
        )
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> App<C> {
    /// Serve one request to the MCP endpoint. `Err` is a refusal that is the
    /// gateway's own (no such server, a token without the scope); everything
    /// MCP has a shape for is in the reply.
    pub async fn mcp(&self, caller: &Caller, request: McpRequest) -> Result<Reply> {
        require_oauth_scope(caller, OAUTH_SCOPE)?;
        let data = self.data();
        let core = self.gproxy().core().snapshot();
        let mut available = servers(&data, &core, caller);
        let aggregated = match &request.endpoint {
            McpEndpoint::Server(name) => {
                available.retain(|server| server.name == *name);
                if available.is_empty() {
                    return Err(AppError::not_found("mcp server", name.clone()));
                }
                false
            }
            McpEndpoint::All { servers } => {
                if let Some(named) = servers {
                    available.retain(|server| named.contains(&server.name));
                }
                true
            }
        };
        let subject = caller.api_key_id.as_deref().unwrap_or(&caller.user_id);
        let session_of = |id: Option<&str>| format!("{subject}:{}", id.unwrap_or("-"));
        let allow = HeaderValue::from_static("POST, DELETE");
        if request.method == Method::DELETE {
            let gateway = Gateway {
                app: self,
                core,
                session: session_of(
                    request
                        .headers
                        .get("mcp-session-id")
                        .and_then(|value| value.to_str().ok()),
                ),
            };
            for server in &available {
                gateway.end(server).await;
            }
            return Ok(Reply {
                status: StatusCode::NO_CONTENT,
                headers: HeaderMap::new(),
                body: None,
            });
        }
        if request.method != Method::POST {
            // The gateway answers every request in its own response and has
            // nothing to say unasked, so there is no stream to open.
            return Ok(Reply {
                status: StatusCode::METHOD_NOT_ALLOWED,
                headers: HeaderMap::from_iter([(header::ALLOW, allow)]),
                body: None,
            });
        }
        let message = match server::read(&request.headers, &request.body) {
            Ok(message) => message,
            Err(reply) => return Ok(*reply),
        };
        let id = &message.id;
        let gateway = Gateway {
            app: self,
            core,
            session: session_of(message.session.as_deref()),
        };
        Ok(match message.inbound {
            Inbound::Initialize { version } => {
                server::initialize(id, version).with_session(&crate::operations::random_id()?)
            }
            Inbound::Discover => server::discover(id),
            Inbound::Ping => server::pong(id, message.revision),
            Inbound::Notification => Reply::accepted(),
            Inbound::Unknown { method } => Reply::unknown_method(id, &method),
            Inbound::ListTools => {
                let mut tools = Vec::new();
                for server in &available {
                    match gateway.offered(&data, caller, server, aggregated).await {
                        Ok(offered) => tools.extend(
                            offered
                                .iter()
                                .map(|tool| server::offered(&tool.tool, &tool.name, tool.nature)),
                        ),
                        // One server down must not hide the others' tools.
                        Err(failure) if aggregated => {
                            tracing::warn!(reason = %failure.0, "MCP server left out of the tool list");
                        }
                        Err(failure) => return Ok(Reply::bad_gateway(id, failure.0)),
                    }
                }
                server::tools(id, message.revision, tools)
            }
            Inbound::CallTool { name, mut params } => {
                let target = match aggregated {
                    true => tool::resolve(&name, available.iter().map(|s| s.name.as_str())),
                    false => Some((available[0].name.as_str(), name.as_str())),
                };
                let Some((server, tool)) = target.and_then(|(server, tool)| {
                    Some((available.iter().find(|s| s.name == server)?, tool))
                }) else {
                    return Ok(Reply::unknown_tool(id, &name));
                };
                params["name"] = json!(tool);
                let started = web_time::Instant::now();
                let mut record = ToolCall {
                    server: server.name.clone(),
                    tool: tool.to_owned(),
                    credential_id: None,
                    duration_ms: 0,
                    outcome: Outcome::Failed,
                    arguments: data
                        .observation
                        .downstream_log_body
                        .then(|| params.get("arguments").cloned())
                        .flatten(),
                    result: None,
                };
                let capabilities = message.capabilities.as_ref();
                let reply = match gateway
                    .call(
                        &data,
                        caller,
                        server,
                        tool,
                        &params,
                        capabilities,
                        &mut record,
                    )
                    .await
                {
                    Ok(Some(answer)) => {
                        if data.observation.downstream_log_body {
                            record.result = answer.as_ref().ok().cloned();
                        }
                        server::call(id, message.revision, answer)
                    }
                    Ok(None) => Reply::unknown_tool(id, &name),
                    Err(failure) => Reply::bad_gateway(id, failure.0),
                };
                record.duration_ms =
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                self.audit_tool_call(caller, request.source_ip, server, record)
                    .await;
                reply
            }
        })
    }

    async fn audit_tool_call(
        &self,
        caller: &Caller,
        source_ip: Option<String>,
        server: &Server,
        record: ToolCall,
    ) {
        let mut entry = AuditEntry::new(AUDIT_ACTION)
            .by(caller)
            .source_ip(source_ip)
            .entity("provider", server.provider.entity.id.clone());
        if matches!(record.outcome, Outcome::Failed | Outcome::Refused) {
            entry.outcome = audit_event::OUTCOME_ERROR.to_owned();
        }
        let detail = serde_json::to_value(&record).expect("a tool call record serializes");
        Audit::new(self.gproxy().store(), self.config().audit_enabled)
            .try_record(entry.detail(AuditEntry::redacted(detail)))
            .await;
    }

    /// A server's tools as an administrator reads them: asked afresh, with
    /// what the server declares and what the gateway treats each tool as.
    pub async fn mcp_tools(&self, provider_id: &str) -> Result<Vec<McpToolDto>> {
        let data = self.data();
        let core = self.gproxy().core().snapshot();
        let provider = core
            .providers
            .get(provider_id)
            .filter(|provider| is_mcp(provider))
            .ok_or_else(|| AppError::not_found("mcp server", provider_id))?;
        let mut credentials: Vec<_> = provider
            .credential_ids
            .iter()
            .filter_map(|id| core.credentials.get(id))
            .filter(|credential| live(credential))
            .cloned()
            .collect();
        credentials.sort_by(|a, b| a.id.cmp(&b.id));
        if credentials.is_empty() || provider.entity.base_url.is_none() {
            return Err(AppError::invalid(
                "the server needs an address and a usable credential before its tools can be listed",
            ));
        }
        let server = Server {
            name: provider.entity.name.clone(),
            provider: provider.clone(),
            credentials,
        };
        let gateway = Gateway {
            app: self,
            core: core.clone(),
            session: "admin:-".to_owned(),
        };
        let listed = async {
            let mut connection = gateway.connect(&server).await?;
            gateway.tools(&server, &mut connection, true).await
        };
        let tools = listed.await.map_err(|failure: Failure| {
            AppError::Sdk(gproxy_sdk::SdkError::Upstream {
                status: StatusCode::BAD_GATEWAY.as_u16(),
                body: failure.0,
            })
        })?;
        Ok(tools
            .into_iter()
            .map(|tool| {
                let declared = Nature::declared(tool.annotations.as_ref());
                let overridden = data.mcp.override_of(provider_id, &tool.name);
                McpToolDto {
                    title: tool
                        .title
                        .clone()
                        .or_else(|| tool.annotations.as_ref()?.title.clone()),
                    description: tool.description.as_ref().map(|text| text.to_string()),
                    declared_nature: declared.as_str().to_owned(),
                    nature: overridden
                        .map_or(declared, |found| found.0)
                        .as_str()
                        .to_owned(),
                    override_id: overridden.map(|found| found.1.to_owned()),
                    name: tool.name.into_owned(),
                }
            })
            .collect())
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Gateway<'_, C> {
    /// The tools of `server` the caller's scopes allow.
    async fn offered(
        &self,
        data: &AppData,
        caller: &Caller,
        server: &Server,
        aggregated: bool,
    ) -> Served<Vec<Offered>> {
        let mut connection = self.connect(server).await?;
        let tools = self.tools(server, &mut connection, false).await?;
        Ok(tools
            .into_iter()
            .filter_map(|tool| {
                let nature = nature(data, server, &tool);
                let name = match aggregated {
                    true => tool::exposed_name(&server.name, &tool.name)?,
                    false => tool.name.to_string(),
                };
                data.mcp
                    .allows(caller, &server.name, &tool.name, nature)
                    .then_some(Offered { tool, name, nature })
            })
            .collect())
    }

    /// Call one tool. `None` when the caller has no such tool: it does not
    /// exist, or the caller's scopes hide it.
    #[allow(clippy::too_many_arguments)]
    async fn call(
        &self,
        data: &AppData,
        caller: &Caller,
        server: &Server,
        tool: &str,
        params: &Value,
        capabilities: Option<&Value>,
        record: &mut ToolCall,
    ) -> Served<Option<Answer>> {
        let mut connection = self.connect(server).await?;
        record.credential_id = Some(connection.credential.id.clone());
        let tools = self.tools(server, &mut connection, false).await?;
        let Some(listed) = tools.iter().find(|listed| listed.name == tool) else {
            record.outcome = Outcome::Refused;
            return Ok(None);
        };
        if !data
            .mcp
            .allows(caller, &server.name, tool, nature(data, server, listed))
        {
            record.outcome = Outcome::Refused;
            return Ok(None);
        }
        let answer = self
            .request(
                server,
                &mut connection,
                &Context { capabilities },
                "tools/call",
                params,
            )
            .await?;
        record.outcome = match &answer {
            Ok(result) if server::result_type(result) != "complete" => Outcome::InputRequired,
            Ok(result) if result["isError"] == true => Outcome::ToolError,
            Ok(_) => Outcome::Ok,
            Err(_) => Outcome::Failed,
        };
        Ok(Some(answer))
    }

    /// The caller ended its session: end the server's, and forget the binding.
    async fn end(&self, server: &Server) {
        let state = self.state(server);
        let Ok(Some(found)) = binding::load(&state, &self.session).await else {
            return;
        };
        let bound = server
            .credentials
            .iter()
            .find(|credential| credential.id == found.binding.credential_id);
        if let Some(credential) = bound
            && let Ok(sent) = headers(&server.provider, credential)
        {
            let target = Target {
                client: credential.client.as_ref(),
                endpoint: server.endpoint(),
                headers: &sent,
            };
            client::close(target, &found.binding.mode).await;
        }
        let _ = binding::remove(&state, &self.session, found).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::support::caller;

    #[test]
    fn nobody_calls_tools_with_another_persons_credential() {
        let user = |id: &str| Owner::User(id.into());
        let mut member = caller("u1", "member");
        member.team_id = Some("t1".into());
        let ranked = |owner: Owner, who: &Caller| preference(Some(&owner), who, Some("o1"));
        assert_eq!(ranked(user("u1"), &member), Some(0));
        assert_eq!(ranked(Owner::Team("t1".into()), &member), Some(1));
        assert_eq!(ranked(Owner::Org("o1".into()), &member), Some(2));
        assert_eq!(ranked(Owner::Shared, &member), Some(3));
        assert_eq!(ranked(user("u2"), &member), None);
        assert_eq!(ranked(Owner::Team("t2".into()), &member), None);
        // Seeing every credential is for managing them.
        let admin = caller("root", "admin");
        assert!(admin.is_instance_admin());
        assert_eq!(ranked(user("u2"), &admin), None);
        assert_eq!(ranked(user("root"), &admin), Some(0));
    }
}
