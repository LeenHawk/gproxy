use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::channel::{
    Channel, ChannelCredential, ChannelSettings, CommonChannelSettings, OAuthCredentialResult,
    OAuthFlow,
};
use crate::count_tokens::CountStrategy;
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Kiro / Amazon Q Runtime channel.
///
/// Kiro does not expose an OpenAI/Claude/Gemini-compatible route surface. The
/// decompiled 0.12.224 client sends chat through Smithy REST-JSON
/// `POST /generateAssistantResponse`, whose response body is AWS eventstream.
/// This channel therefore performs the Kiro-specific request/response shaping
/// locally and does not rely on provider handler special cases.
pub struct KiroChannel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiroSettings {
    #[serde(default = "default_kiro_base_url")]
    pub base_url: String,
    #[serde(default = "default_kiro_rest_base_url")]
    pub rest_base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_task_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amz_target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_prefix: Option<String>,
    #[serde(default = "default_kiro_auth_base_url")]
    pub auth_base_url: String,
    #[serde(default = "default_kiro_auth_portal_url")]
    pub auth_portal_url: String,
    #[serde(default = "default_kiro_oauth_redirect_uri")]
    pub oauth_redirect_uri: String,
    #[serde(default = "default_kiro_idc_redirect_uri")]
    pub idc_redirect_uri: String,
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

impl Default for KiroSettings {
    fn default() -> Self {
        Self {
            base_url: default_kiro_base_url(),
            rest_base_url: default_kiro_rest_base_url(),
            profile_arn: None,
            agent_mode: None,
            origin: None,
            agent_task_type: None,
            amz_target: None,
            scope_prefix: None,
            auth_base_url: default_kiro_auth_base_url(),
            auth_portal_url: default_kiro_auth_portal_url(),
            oauth_redirect_uri: default_kiro_oauth_redirect_uri(),
            idc_redirect_uri: default_kiro_idc_redirect_uri(),
            common: CommonChannelSettings::default(),
        }
    }
}

fn default_kiro_base_url() -> String {
    "https://q.us-east-1.amazonaws.com".to_string()
}

fn default_kiro_rest_base_url() -> String {
    "https://codewhisperer.us-east-1.amazonaws.com".to_string()
}

fn default_kiro_auth_base_url() -> String {
    "https://prod.us-east-1.auth.desktop.kiro.dev".to_string()
}

fn default_kiro_auth_portal_url() -> String {
    "https://app.kiro.dev".to_string()
}

fn default_kiro_oauth_redirect_uri() -> String {
    "http://localhost:3128".to_string()
}

fn default_kiro_idc_redirect_uri() -> String {
    "http://127.0.0.1/oauth/callback".to_string()
}

const DEFAULT_KIRO_ORIGIN: &str = "AI_EDITOR";
const DEFAULT_KIRO_AGENT_MODE: &str = "vibe";
const DEFAULT_KIRO_AGENT_TASK_TYPE: &str = "vibe";
const KIRO_STREAM_ID_HEADER: &str = "x-gproxy-kiro-stream-id";
const MAX_KIRO_TOOL_DESCRIPTION_LEN: usize = 10_237;
const KIRO_OAUTH_STATE_TTL_MS: u64 = 10 * 60 * 1000;
const KIRO_AUTH_USER_AGENT: &str = "KiroIDE-0.12.224-gproxy";
const KIRO_BUILDER_ID_START_URL: &str = "https://view.awsapps.com/start";
const KIRO_INTERNAL_SSO_START_URL: &str = "https://amzn.awsapps.com/start";
const KIRO_GRANT_SCOPES: &[&str] = &[
    "completions",
    "analysis",
    "conversations",
    "transformations",
    "taskassist",
];

impl ChannelSettings for KiroSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KiroCredential {
    #[serde(default, alias = "api_key", alias = "token")]
    pub access_token: String,
    #[serde(
        default,
        alias = "refreshToken",
        skip_serializing_if = "Option::is_none"
    )]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,
    #[serde(
        default,
        alias = "expiresAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    pub expires_at_ms: Option<u64>,
    #[serde(default, alias = "authMethod", skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, alias = "clientId", skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(
        default,
        alias = "clientSecret",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

impl ChannelCredential for KiroCredential {
    fn apply_update(&mut self, update: &Value) -> bool {
        let Some(object) = update.as_object() else {
            return false;
        };
        let mut changed = false;
        if let Some(value) = object
            .get("access_token")
            .or_else(|| object.get("accessToken"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.access_token = value.to_string();
            changed = true;
        }
        if let Some(value) = object
            .get("refresh_token")
            .or_else(|| object.get("refreshToken"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.refresh_token = Some(value.to_string());
            changed = true;
        }
        if let Some(value) = object
            .get("profile_arn")
            .or_else(|| object.get("profileArn"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.profile_arn = Some(value.to_string());
            changed = true;
        }
        if let Some(value) = object
            .get("expires_at_ms")
            .or_else(|| object.get("expiresAtMs"))
            .and_then(Value::as_u64)
        {
            self.expires_at_ms = Some(value);
            changed = true;
        }
        if let Some(value) = object
            .get("client_id")
            .or_else(|| object.get("clientId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.client_id = Some(value.to_string());
            changed = true;
        }
        if let Some(value) = object
            .get("client_secret")
            .or_else(|| object.get("clientSecret"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.client_secret = Some(value.to_string());
            changed = true;
        }
        if let Some(value) = object
            .get("region")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.region = Some(value.to_string());
            changed = true;
        }
        if let Some(value) = object
            .get("auth_method")
            .or_else(|| object.get("authMethod"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.auth_method = Some(value.to_string());
            changed = true;
        }
        if let Some(value) = object
            .get("provider")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.provider = Some(value.to_string());
            changed = true;
        }
        changed
    }
}

#[derive(Debug, Clone)]
struct KiroOAuthState {
    code_verifier: String,
    redirect_uri: String,
    flow: KiroOAuthStateFlow,
    created_at_unix_ms: u64,
}

#[derive(Debug, Clone)]
enum KiroOAuthStateFlow {
    Social {
        invitation_code: Option<String>,
    },
    Idc {
        client_id: String,
        client_secret: String,
        region: String,
        provider: String,
    },
}

fn kiro_oauth_states() -> &'static DashMap<String, KiroOAuthState> {
    static STATES: OnceLock<DashMap<String, KiroOAuthState>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn prune_kiro_oauth_states(now_unix_ms: u64) {
    kiro_oauth_states().retain(|_, state| {
        now_unix_ms.saturating_sub(state.created_at_unix_ms) <= KIRO_OAUTH_STATE_TTL_MS
    });
}

impl Channel for KiroChannel {
    const ID: &'static str = "kiro";
    type Settings = KiroSettings;
    type Credential = KiroCredential;
    type Health = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        let mut table = RoutingTable::new();
        let pass = |operation: OperationFamily, protocol: ProtocolKind| {
            (
                RouteKey::new(operation, protocol),
                RouteImplementation::Passthrough,
            )
        };
        let xform_to_response = |operation: OperationFamily, protocol: ProtocolKind| {
            (
                RouteKey::new(operation, protocol),
                RouteImplementation::TransformTo {
                    destination: RouteKey::new(
                        OperationFamily::GenerateContent,
                        ProtocolKind::OpenAiResponse,
                    ),
                },
            )
        };
        let xform = |operation: OperationFamily,
                     protocol: ProtocolKind,
                     destination: OperationFamily,
                     destination_protocol: ProtocolKind| {
            (
                RouteKey::new(operation, protocol),
                RouteImplementation::TransformTo {
                    destination: RouteKey::new(destination, destination_protocol),
                },
            )
        };
        let local = |protocol: ProtocolKind| {
            (
                RouteKey::new(OperationFamily::CountToken, protocol),
                RouteImplementation::Local,
            )
        };
        for (key, implementation) in [
            pass(OperationFamily::ModelList, ProtocolKind::OpenAi),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Claude,
                OperationFamily::ModelList,
                ProtocolKind::OpenAi,
            ),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
                OperationFamily::ModelList,
                ProtocolKind::OpenAi,
            ),
            local(ProtocolKind::OpenAi),
            local(ProtocolKind::Claude),
            local(ProtocolKind::Gemini),
            pass(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform_to_response(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform_to_response(OperationFamily::GenerateContent, ProtocolKind::Claude),
            xform_to_response(OperationFamily::GenerateContent, ProtocolKind::Gemini),
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::GeminiNDJson,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
        ] {
            table.set(key, implementation);
        }

        table
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let token = credential.access_token.trim();
        if token.is_empty() {
            return Err(UpstreamError::Channel(
                "missing kiro access_token credential".into(),
            ));
        }

        let profile_arn = credential
            .profile_arn
            .as_deref()
            .or(settings.profile_arn.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if request.route.operation == OperationFamily::ModelList {
            return build_kiro_model_list_request(token, settings, profile_arn);
        }

        let path = kiro_request_path(request)?;
        let url = format!("{}{}", settings.base_url.trim_end_matches('/'), path);
        let body = add_profile_arn_to_kiro_body(&request.body, profile_arn)?;
        let builder =
            kiro_runtime_request_builder(token, settings, request, http::Method::POST, &url, "*/*");

        builder
            .body(body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn finalize_request(
        &self,
        _settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        if matches!(
            request.route.operation,
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent
        ) && request.route.protocol == ProtocolKind::OpenAiResponse
        {
            request.body = openai_response_body_to_kiro_request(
                &request.body,
                request.model.as_deref(),
                _settings,
            )?;
            if request.route.operation == OperationFamily::StreamGenerateContent {
                let stream_id = uuid::Uuid::new_v4().to_string();
                request.headers.insert(
                    KIRO_STREAM_ID_HEADER,
                    http::HeaderValue::from_str(&stream_id)
                        .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?,
                );
            }
        }
        Ok(request)
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        if request.route.operation == OperationFamily::ModelList {
            return kiro_model_list_to_openai_model_list(&body);
        }
        if request.route.operation == OperationFamily::StreamGenerateContent
            && request.route.protocol == ProtocolKind::OpenAiResponse
        {
            return normalize_kiro_stream_chunk(request, body);
        }
        if request.route.operation == OperationFamily::GenerateContent
            && request.route.protocol == ProtocolKind::OpenAiResponse
            && looks_like_aws_eventstream(&body)
        {
            return kiro_eventstream_to_openai_response(request, &body).unwrap_or(body);
        }
        body
    }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        _body: &[u8],
    ) -> ResponseClassification {
        match status {
            200..=299 => ResponseClassification::Success,
            401 | 403 => ResponseClassification::AuthDead,
            429 => {
                let retry_after_ms = headers
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|seconds| seconds * 1000);
                ResponseClassification::RateLimited { retry_after_ms }
            }
            500..=599 => ResponseClassification::TransientError,
            _ => ResponseClassification::PermanentError,
        }
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::Local
    }

    fn handle_local(
        &self,
        operation: OperationFamily,
        protocol: ProtocolKind,
        _model: Option<&str>,
        _query: Option<&str>,
        body: &[u8],
    ) -> Option<Result<Vec<u8>, UpstreamError>> {
        (operation == OperationFamily::CountToken)
            .then(|| crate::count_tokens::local_count_response_for_protocol(protocol, body))
    }

    fn needs_refresh(&self, credential: &Self::Credential) -> bool {
        if credential.access_token.trim().is_empty() {
            return credential
                .refresh_token
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        }
        credential.expires_at_ms.is_some_and(|expires_at_ms| {
            expires_at_ms <= kiro_current_unix_ms().saturating_add(60_000)
        })
    }

    fn refresh_credential_with_settings<'a>(
        &'a self,
        client: &'a wreq::Client,
        settings: &'a Self::Settings,
        credential: &'a mut Self::Credential,
    ) -> impl std::future::Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        refresh_kiro_credential(client, settings, credential)
    }

    fn prepare_quota_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        let token = credential.access_token.trim();
        if token.is_empty() {
            return Err(UpstreamError::Channel(
                "missing kiro access_token credential".into(),
            ));
        }
        let profile_arn = credential
            .profile_arn
            .as_deref()
            .or(settings.profile_arn.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        build_kiro_usage_request(token, settings, profile_arn).map(Some)
    }

    fn oauth_start<'a>(
        &'a self,
        client: &'a wreq::Client,
        settings: &'a Self::Settings,
        params: &'a BTreeMap<String, String>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<OAuthFlow>, UpstreamError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let now_unix_ms = kiro_current_unix_ms();
            prune_kiro_oauth_states(now_unix_ms);

            if kiro_direct_idc_oauth_requested(params) {
                let region = crate::utils::oauth::parse_query_value(params, "region")
                    .unwrap_or_else(|| "us-east-1".to_string());
                let provider = kiro_idc_provider_from_params(params);
                let start_url = kiro_idc_start_url(params, &provider)?;
                let redirect_uri = crate::utils::oauth::parse_query_value(params, "redirect_uri")
                    .unwrap_or_else(|| settings.idc_redirect_uri.clone());
                let scopes = kiro_scopes_for_settings(settings);
                let registration =
                    register_kiro_oidc_client(client, &region, &start_url, &redirect_uri, &scopes)
                        .await?;
                let state = crate::utils::oauth::generate_state();
                let code_verifier = crate::utils::oauth::generate_code_verifier();
                let code_challenge = crate::utils::oauth::generate_code_challenge(&code_verifier);
                let authorize_url = build_kiro_oidc_authorize_url(
                    &region,
                    &registration.client_id,
                    &redirect_uri,
                    &scopes,
                    &state,
                    &code_challenge,
                )?;

                kiro_oauth_states().insert(
                    state.clone(),
                    KiroOAuthState {
                        code_verifier,
                        redirect_uri: redirect_uri.clone(),
                        flow: KiroOAuthStateFlow::Idc {
                            client_id: registration.client_id,
                            client_secret: registration.client_secret,
                            region,
                            provider,
                        },
                        created_at_unix_ms: now_unix_ms,
                    },
                );

                return Ok(Some(OAuthFlow {
                    authorize_url,
                    state,
                    redirect_uri: Some(redirect_uri),
                    verification_uri: None,
                    user_code: None,
                    mode: Some("authorization_code".to_string()),
                    scope: Some(scopes.join(",")),
                    instructions: Some(
                        "Open authorize_url and paste the final OIDC callback URL into oauth_finish."
                            .to_string(),
                    ),
                }));
            }

            let redirect_uri = crate::utils::oauth::parse_query_value(params, "redirect_uri")
                .unwrap_or_else(|| settings.oauth_redirect_uri.clone());
            let invitation_code = crate::utils::oauth::parse_query_value(params, "invitation_code");
            let state = crate::utils::oauth::generate_state();
            let code_verifier = crate::utils::oauth::generate_code_verifier();
            let code_challenge = crate::utils::oauth::generate_code_challenge(&code_verifier);
            let authorize_url = build_kiro_portal_authorize_url(
                &settings.auth_portal_url,
                &state,
                &code_challenge,
                &redirect_uri,
                kiro_param_is_true(params, "from_amazon_internal"),
            )?;

            kiro_oauth_states().insert(
                state.clone(),
                KiroOAuthState {
                    code_verifier,
                    redirect_uri: redirect_uri.clone(),
                    flow: KiroOAuthStateFlow::Social { invitation_code },
                    created_at_unix_ms: now_unix_ms,
                },
            );

            Ok(Some(OAuthFlow {
                authorize_url,
                state,
                redirect_uri: Some(redirect_uri),
                verification_uri: None,
                user_code: None,
                mode: Some("authorization_code".to_string()),
                scope: None,
                instructions: Some(
                    "Open authorize_url, finish Kiro Portal social login, then paste the localhost callback URL into oauth_finish."
                        .to_string(),
                ),
            }))
        })
    }

    fn oauth_finish<'a>(
        &'a self,
        client: &'a wreq::Client,
        settings: &'a Self::Settings,
        params: &'a BTreeMap<String, String>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<OAuthCredentialResult<Self::Credential>>, UpstreamError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if let Some(error) = kiro_oauth_param(params, "error") {
                let detail = kiro_oauth_param(params, "error_description").unwrap_or(error);
                return Err(UpstreamError::Channel(detail));
            }

            prune_kiro_oauth_states(kiro_current_unix_ms());
            let (code, state_param) = crate::utils::oauth::resolve_code_and_state(params)
                .map_err(|e| UpstreamError::Channel(format!("kiro oauth callback: {e}")))?;
            let state_id = state_param.ok_or_else(|| {
                UpstreamError::Channel("kiro oauth callback: missing state".to_string())
            })?;
            let (_, oauth_state) =
                kiro_oauth_states()
                    .remove(state_id.as_str())
                    .ok_or_else(|| {
                        UpstreamError::Channel("kiro oauth callback: missing state".to_string())
                    })?;

            match oauth_state.flow {
                KiroOAuthStateFlow::Social { invitation_code } => {
                    let login_option =
                        kiro_oauth_param(params, "login_option").ok_or_else(|| {
                            UpstreamError::Channel(
                                "kiro oauth callback: missing login_option".to_string(),
                            )
                        })?;
                    let provider = match login_option.as_str() {
                        "google" => "Google",
                        "github" => "Github",
                        "builderid" | "awsidc" | "internal" | "external_idp" => {
                            return Err(UpstreamError::Channel(format!(
                                "kiro oauth callback: login_option={login_option} requires direct IdC/external IdP OAuth; call oauth_start with auth_method=idc for AWS IdC"
                            )));
                        }
                        other => {
                            return Err(UpstreamError::Channel(format!(
                                "kiro oauth callback: unsupported login_option={other}"
                            )));
                        }
                    };
                    let callback_url =
                        crate::utils::oauth::parse_query_value(params, "callback_url");
                    let redirect_uri = build_kiro_social_exchange_redirect_uri(
                        &oauth_state.redirect_uri,
                        callback_url.as_deref(),
                        &login_option,
                    );
                    let token = exchange_kiro_social_code_for_token(
                        client,
                        settings,
                        &code,
                        &oauth_state.code_verifier,
                        &redirect_uri,
                        invitation_code.as_deref(),
                    )
                    .await?;
                    let credential = kiro_token_response_to_credential(token, provider)?;

                    Ok(Some(OAuthCredentialResult {
                        details: json!({
                            "access_token": credential.access_token.clone(),
                            "refresh_token": credential.refresh_token.clone(),
                            "profile_arn": credential.profile_arn.clone(),
                            "expires_at_ms": credential.expires_at_ms,
                            "auth_method": credential.auth_method.clone(),
                            "provider": credential.provider.clone(),
                        }),
                        credential,
                    }))
                }
                KiroOAuthStateFlow::Idc {
                    client_id,
                    client_secret,
                    region,
                    provider,
                } => {
                    let token = exchange_kiro_oidc_code_for_token(
                        client,
                        &region,
                        &client_id,
                        &client_secret,
                        &code,
                        &oauth_state.code_verifier,
                        &oauth_state.redirect_uri,
                    )
                    .await?;
                    let credential = kiro_oidc_token_response_to_credential(
                        token,
                        client_id,
                        client_secret,
                        region,
                        provider,
                    )?;

                    Ok(Some(OAuthCredentialResult {
                        details: json!({
                            "access_token": credential.access_token.clone(),
                            "refresh_token": credential.refresh_token.clone(),
                            "expires_at_ms": credential.expires_at_ms,
                            "auth_method": credential.auth_method.clone(),
                            "provider": credential.provider.clone(),
                            "client_id": credential.client_id.clone(),
                            "region": credential.region.clone(),
                        }),
                        credential,
                    }))
                }
            }
        })
    }
}

fn kiro_routing_table() -> RoutingTable {
    KiroChannel.routing_table()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KiroAuthTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    profile_arn: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KiroOidcClientRegistration {
    client_id: String,
    client_secret: String,
}

async fn refresh_kiro_credential(
    client: &wreq::Client,
    settings: &KiroSettings,
    credential: &mut KiroCredential,
) -> Result<bool, UpstreamError> {
    if credential
        .client_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        && credential
            .client_secret
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    {
        return refresh_kiro_oidc_credential(client, credential).await;
    }
    refresh_kiro_social_credential(client, settings, credential).await
}

async fn refresh_kiro_social_credential(
    client: &wreq::Client,
    settings: &KiroSettings,
    credential: &mut KiroCredential,
) -> Result<bool, UpstreamError> {
    let refresh_token = match credential
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(refresh_token) => refresh_token.to_string(),
        None => return Ok(false),
    };
    let url = format!(
        "{}/refreshToken",
        settings.auth_base_url.trim_end_matches('/')
    );
    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", kiro_auth_user_agent(settings))
        .json(&json!({ "refreshToken": refresh_token }))
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro refresh token request failed: {e}")))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro refresh token response failed: {e}")))?;
    if !status.is_success() {
        return Err(UpstreamError::Channel(format!(
            "kiro refresh token failed: HTTP {} {}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        )));
    }
    let mut token: KiroAuthTokenResponse = serde_json::from_slice(&body)
        .map_err(|e| UpstreamError::Channel(format!("kiro refresh token decode failed: {e}")))?;
    if token.profile_arn.is_none() {
        token.profile_arn = credential.profile_arn.clone();
    }
    let updated =
        kiro_token_response_to_credential(token, credential.provider.as_deref().unwrap_or("Kiro"))?;
    credential.access_token = updated.access_token;
    credential.refresh_token = updated.refresh_token;
    credential.profile_arn = updated.profile_arn;
    credential.expires_at_ms = updated.expires_at_ms;
    credential.auth_method = updated.auth_method.or_else(|| Some("social".to_string()));
    if credential.provider.is_none() {
        credential.provider = updated.provider;
    }
    Ok(true)
}

async fn refresh_kiro_oidc_credential(
    client: &wreq::Client,
    credential: &mut KiroCredential,
) -> Result<bool, UpstreamError> {
    let refresh_token = match credential
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(refresh_token) => refresh_token.to_string(),
        None => return Ok(false),
    };
    let client_id = credential
        .client_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpstreamError::Channel("kiro oidc refresh requires clientId".to_string()))?
        .to_string();
    let client_secret = credential
        .client_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            UpstreamError::Channel("kiro oidc refresh requires clientSecret".to_string())
        })?
        .to_string();
    let region = credential
        .region
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("us-east-1")
        .to_string();
    let url = format!("{}/token", kiro_oidc_base_url(&region));
    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", KIRO_AUTH_USER_AGENT)
        .json(&json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "refreshToken": refresh_token,
            "grantType": "refresh_token",
        }))
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc refresh request failed: {e}")))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc refresh response failed: {e}")))?;
    if !status.is_success() {
        return Err(UpstreamError::Channel(format!(
            "kiro oidc refresh failed: HTTP {} {}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        )));
    }
    let token: KiroAuthTokenResponse = serde_json::from_slice(&body)
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc refresh decode failed: {e}")))?;
    let access_token = token
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpstreamError::Channel("kiro oidc refresh missing accessToken".to_string()))?
        .to_string();
    credential.access_token = access_token;
    if let Some(refresh_token) = token
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        credential.refresh_token = Some(refresh_token.to_string());
    }
    if let Some(profile_arn) = token
        .profile_arn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        credential.profile_arn = Some(profile_arn.to_string());
    }
    credential.expires_at_ms = Some(
        kiro_current_unix_ms()
            .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000)),
    );
    credential
        .auth_method
        .get_or_insert_with(|| "IdC".to_string());
    credential.region.get_or_insert(region);
    Ok(true)
}

fn build_kiro_portal_authorize_url(
    portal_url: &str,
    state: &str,
    code_challenge: &str,
    redirect_uri: &str,
    from_amazon_internal: bool,
) -> Result<String, UpstreamError> {
    let mut url = url::Url::parse(portal_url.trim_end_matches('/'))
        .map_err(|e| UpstreamError::Channel(format!("invalid kiro auth portal url: {e}")))?;
    url.set_path("/signin");
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("state", state);
        query.append_pair("code_challenge", code_challenge);
        query.append_pair("code_challenge_method", "S256");
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("redirect_from", "KiroIDE");
        if from_amazon_internal {
            query.append_pair("from_amazon_internal", "true");
        }
    }
    Ok(url.to_string())
}

async fn exchange_kiro_social_code_for_token(
    client: &wreq::Client,
    settings: &KiroSettings,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    invitation_code: Option<&str>,
) -> Result<KiroAuthTokenResponse, UpstreamError> {
    let url = format!(
        "{}/oauth/token",
        settings.auth_base_url.trim_end_matches('/')
    );
    let mut body = json!({
        "code": code,
        "code_verifier": code_verifier,
        "redirect_uri": redirect_uri,
    });
    if let Some(invitation_code) = invitation_code
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && let Some(object) = body.as_object_mut()
    {
        object.insert(
            "invitation_code".to_string(),
            Value::String(invitation_code.to_string()),
        );
    }
    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", kiro_auth_user_agent(settings))
        .json(&body)
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oauth token request failed: {e}")))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oauth token response failed: {e}")))?;
    if !status.is_success() {
        return Err(UpstreamError::Channel(format!(
            "kiro oauth token exchange failed: HTTP {} {}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| UpstreamError::Channel(format!("kiro oauth token decode failed: {e}")))
}

async fn register_kiro_oidc_client(
    client: &wreq::Client,
    region: &str,
    start_url: &str,
    redirect_uri: &str,
    scopes: &[String],
) -> Result<KiroOidcClientRegistration, UpstreamError> {
    let url = format!("{}/client/register", kiro_oidc_base_url(region));
    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", KIRO_AUTH_USER_AGENT)
        .json(&json!({
            "clientName": "Kiro IDE",
            "clientType": "public",
            "scopes": scopes,
            "grantTypes": ["authorization_code", "refresh_token"],
            "redirectUris": [redirect_uri],
            "issuerUrl": start_url,
        }))
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc register request failed: {e}")))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc register response failed: {e}")))?;
    if !status.is_success() {
        return Err(UpstreamError::Channel(format!(
            "kiro oidc register failed: HTTP {} {}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc register decode failed: {e}")))
}

async fn exchange_kiro_oidc_code_for_token(
    client: &wreq::Client,
    region: &str,
    client_id: &str,
    client_secret: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<KiroAuthTokenResponse, UpstreamError> {
    let url = format!("{}/token", kiro_oidc_base_url(region));
    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", KIRO_AUTH_USER_AGENT)
        .json(&json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "grantType": "authorization_code",
            "redirectUri": redirect_uri,
            "code": code,
            "codeVerifier": code_verifier,
        }))
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc token request failed: {e}")))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc token response failed: {e}")))?;
    if !status.is_success() {
        return Err(UpstreamError::Channel(format!(
            "kiro oidc token exchange failed: HTTP {} {}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| UpstreamError::Channel(format!("kiro oidc token decode failed: {e}")))
}

fn kiro_token_response_to_credential(
    token: KiroAuthTokenResponse,
    provider: &str,
) -> Result<KiroCredential, UpstreamError> {
    let access_token = token
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            UpstreamError::Channel("kiro oauth callback: missing accessToken".to_string())
        })?
        .to_string();
    let refresh_token = token
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            UpstreamError::Channel("kiro oauth callback: missing refreshToken".to_string())
        })?
        .to_string();
    let expires_at_ms = kiro_current_unix_ms()
        .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000));
    Ok(KiroCredential {
        access_token,
        refresh_token: Some(refresh_token),
        profile_arn: token.profile_arn,
        expires_at_ms: Some(expires_at_ms),
        auth_method: Some("social".to_string()),
        provider: Some(provider.to_string()),
        client_id: None,
        client_secret: None,
        region: None,
    })
}

fn kiro_oidc_token_response_to_credential(
    token: KiroAuthTokenResponse,
    client_id: String,
    client_secret: String,
    region: String,
    provider: String,
) -> Result<KiroCredential, UpstreamError> {
    let access_token = token
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpstreamError::Channel("kiro oidc token missing accessToken".to_string()))?
        .to_string();
    let refresh_token = token
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpstreamError::Channel("kiro oidc token missing refreshToken".to_string()))?
        .to_string();
    let expires_at_ms = kiro_current_unix_ms()
        .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000));
    Ok(KiroCredential {
        access_token,
        refresh_token: Some(refresh_token),
        profile_arn: token.profile_arn,
        expires_at_ms: Some(expires_at_ms),
        auth_method: Some("IdC".to_string()),
        provider: Some(provider),
        client_id: Some(client_id),
        client_secret: Some(client_secret),
        region: Some(region),
    })
}

fn build_kiro_oidc_authorize_url(
    region: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    code_challenge: &str,
) -> Result<String, UpstreamError> {
    let mut url = url::Url::parse(&format!("{}/authorize", kiro_oidc_base_url(region)))
        .map_err(|e| UpstreamError::Channel(format!("invalid kiro oidc authorize url: {e}")))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("scopes", &scopes.join(","));
        query.append_pair("state", state);
        query.append_pair("code_challenge", code_challenge);
        query.append_pair("code_challenge_method", "S256");
    }
    Ok(url.to_string())
}

fn build_kiro_social_exchange_redirect_uri(
    redirect_uri: &str,
    callback_url: Option<&str>,
    login_option: &str,
) -> String {
    let callback_path = callback_url
        .and_then(kiro_callback_path)
        .unwrap_or_else(|| "/oauth/callback".to_string());
    if let Ok(mut url) = url::Url::parse(redirect_uri) {
        if url.path().is_empty() || url.path() == "/" {
            url.set_path(&callback_path);
        }
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("login_option", login_option);
        return url.to_string();
    }
    format!(
        "{}{}?login_option={}",
        redirect_uri.trim_end_matches('/'),
        callback_path,
        crate::utils::oauth::percent_encode(login_option)
    )
}

fn kiro_callback_path(callback_url: &str) -> Option<String> {
    let normalized = callback_url.trim().replace("&amp;", "&");
    let parsed = url::Url::parse(&normalized).ok()?;
    let path = parsed.path();
    (!path.is_empty()).then(|| path.to_string())
}

fn kiro_oauth_param(params: &BTreeMap<String, String>, key: &str) -> Option<String> {
    crate::utils::oauth::parse_query_value(params, key).or_else(|| {
        let callback_url = crate::utils::oauth::parse_query_value(params, "callback_url")?;
        kiro_callback_query_param(&callback_url, key)
    })
}

fn kiro_callback_query_param(callback_url: &str, key: &str) -> Option<String> {
    let normalized = callback_url.trim().replace("&amp;", "&");
    if let Ok(parsed) = url::Url::parse(&normalized)
        && let Some(query) = parsed.query()
    {
        return url::form_urlencoded::parse(query.as_bytes()).find_map(|(name, value)| {
            (name == key && !value.trim().is_empty()).then(|| value.into_owned())
        });
    }
    let query = normalized
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_else(|| normalized.trim_start_matches('?'));
    url::form_urlencoded::parse(query.as_bytes()).find_map(|(name, value)| {
        (name == key && !value.trim().is_empty()).then(|| value.into_owned())
    })
}

fn kiro_param_is_true(params: &BTreeMap<String, String>, key: &str) -> bool {
    crate::utils::oauth::parse_query_value(params, key).is_some_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "y"
        )
    })
}

fn kiro_direct_idc_oauth_requested(params: &BTreeMap<String, String>) -> bool {
    let auth_method = crate::utils::oauth::parse_query_value(params, "auth_method")
        .or_else(|| crate::utils::oauth::parse_query_value(params, "login_option"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        auth_method.as_str(),
        "idc" | "iam_sso" | "awsidc" | "builderid" | "internal"
    ) || crate::utils::oauth::parse_query_value(params, "start_url").is_some()
}

fn kiro_idc_provider_from_params(params: &BTreeMap<String, String>) -> String {
    let value = crate::utils::oauth::parse_query_value(params, "provider")
        .or_else(|| crate::utils::oauth::parse_query_value(params, "login_option"))
        .or_else(|| crate::utils::oauth::parse_query_value(params, "auth_provider"))
        .unwrap_or_else(|| "Enterprise".to_string());
    match value.to_ascii_lowercase().as_str() {
        "builderid" | "builder_id" | "builder" => "BuilderId".to_string(),
        "internal" => "Internal".to_string(),
        _ => "Enterprise".to_string(),
    }
}

fn kiro_idc_start_url(
    params: &BTreeMap<String, String>,
    provider: &str,
) -> Result<String, UpstreamError> {
    if let Some(start_url) = crate::utils::oauth::parse_query_value(params, "start_url")
        .or_else(|| crate::utils::oauth::parse_query_value(params, "issuer_url"))
    {
        return Ok(start_url);
    }
    match provider {
        "BuilderId" => Ok(KIRO_BUILDER_ID_START_URL.to_string()),
        "Internal" => Ok(KIRO_INTERNAL_SSO_START_URL.to_string()),
        _ => Err(UpstreamError::Channel(
            "kiro idc oauth requires start_url for Enterprise/AWS IdC".to_string(),
        )),
    }
}

fn kiro_scopes_for_settings(settings: &KiroSettings) -> Vec<String> {
    let prefix = settings
        .scope_prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("codewhisperer");
    KIRO_GRANT_SCOPES
        .iter()
        .map(|scope| format!("{prefix}:{scope}"))
        .collect()
}

fn kiro_auth_user_agent(settings: &KiroSettings) -> &str {
    settings.user_agent().unwrap_or(KIRO_AUTH_USER_AGENT)
}

fn kiro_oidc_base_url(region: &str) -> String {
    format!("https://oidc.{}.amazonaws.com", region.trim())
}

fn kiro_current_unix_ms() -> u64 {
    crate::utils::oauth::current_unix_ms()
}

fn kiro_request_path(request: &PreparedRequest) -> Result<&'static str, UpstreamError> {
    match (request.route.operation, request.route.protocol) {
        (
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => Ok("/generateAssistantResponse"),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported kiro request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn effective_kiro_origin(settings: &KiroSettings) -> &str {
    settings
        .origin
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_KIRO_ORIGIN)
}

fn effective_kiro_agent_mode(settings: &KiroSettings) -> &str {
    settings
        .agent_mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_KIRO_AGENT_MODE)
}

fn effective_kiro_agent_task_type(settings: &KiroSettings) -> &str {
    settings
        .agent_task_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_KIRO_AGENT_TASK_TYPE)
}

fn kiro_runtime_request_builder(
    token: &str,
    settings: &KiroSettings,
    request: &PreparedRequest,
    method: http::Method,
    url: &str,
    accept: &str,
) -> http::request::Builder {
    let mut builder = http::Request::builder()
        .method(method)
        .uri(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", accept)
        .header(
            "x-amzn-kiro-agent-mode",
            effective_kiro_agent_mode(settings),
        )
        .header("x-amzn-codewhisperer-optout", "true")
        .header("Amz-Sdk-Request", "attempt=1; max=3")
        .header("Amz-Sdk-Invocation-Id", uuid::Uuid::new_v4().to_string());

    if let Some(user_agent) = settings.user_agent() {
        builder = builder.header("User-Agent", user_agent);
    }
    if let Some(amz_target) = settings
        .amz_target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        builder = builder.header("X-Amz-Target", amz_target);
    }
    if let Some(opt_out) = request.headers.get("x-amzn-codewhisperer-optout") {
        builder = builder.header("x-amzn-codewhisperer-optout", opt_out);
    }

    builder
}

fn build_kiro_model_list_request(
    token: &str,
    settings: &KiroSettings,
    profile_arn: Option<&str>,
) -> Result<http::Request<Vec<u8>>, UpstreamError> {
    let mut url = format!(
        "{}/ListAvailableModels",
        settings.rest_base_url.trim_end_matches('/')
    );
    append_kiro_query(
        &mut url,
        [
            ("origin", Some(effective_kiro_origin(settings))),
            ("maxResults", Some("50")),
            ("profileArn", profile_arn),
        ],
    );
    let mut builder = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("x-amzn-codewhisperer-optout", "true")
        .header("Amz-Sdk-Request", "attempt=1; max=3")
        .header("Amz-Sdk-Invocation-Id", uuid::Uuid::new_v4().to_string());
    if let Some(user_agent) = settings.user_agent() {
        builder = builder.header("User-Agent", user_agent);
    }
    builder
        .body(Vec::new())
        .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
}

fn build_kiro_usage_request(
    token: &str,
    settings: &KiroSettings,
    profile_arn: Option<&str>,
) -> Result<http::Request<Vec<u8>>, UpstreamError> {
    let mut url = format!(
        "{}/getUsageLimits",
        settings.rest_base_url.trim_end_matches('/')
    );
    append_kiro_query(
        &mut url,
        [
            ("origin", Some(effective_kiro_origin(settings))),
            ("resourceType", Some("AGENTIC_REQUEST")),
            ("isEmailRequired", Some("true")),
            ("profileArn", profile_arn),
        ],
    );
    let mut builder = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("x-amzn-codewhisperer-optout", "true")
        .header("Amz-Sdk-Request", "attempt=1; max=3")
        .header("Amz-Sdk-Invocation-Id", uuid::Uuid::new_v4().to_string());
    if let Some(user_agent) = settings.user_agent() {
        builder = builder.header("User-Agent", user_agent);
    }
    builder
        .body(Vec::new())
        .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
}

fn append_kiro_query<'a>(
    url: &mut String,
    pairs: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            serializer.append_pair(key, value);
        }
    }
    let query = serializer.finish();
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query);
    }
}

fn add_profile_arn_to_kiro_body(
    body: &[u8],
    profile_arn: Option<&str>,
) -> Result<Vec<u8>, UpstreamError> {
    let Some(profile_arn) = profile_arn else {
        return Ok(body.to_vec());
    };
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|e| UpstreamError::Channel(format!("deserialize kiro request body: {e}")))?;
    if value.get("profileArn").is_none()
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "profileArn".to_string(),
            Value::String(profile_arn.to_string()),
        );
    }
    serde_json::to_vec(&value)
        .map_err(|e| UpstreamError::Channel(format!("serialize kiro request body: {e}")))
}

fn openai_response_body_to_kiro_request(
    body: &[u8],
    model: Option<&str>,
    settings: &KiroSettings,
) -> Result<Vec<u8>, UpstreamError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| UpstreamError::Channel(format!("deserialize openai response body: {e}")))?;
    reject_openai_response_features_without_kiro_mapping(&value)?;
    let conversation_state = openai_response_to_kiro_conversation_state(&value, model, settings)?;
    let mut request = json!({ "conversationState": conversation_state });
    if let Some(inference_config) = inference_config_from_openai_response(&value)
        && let Some(object) = request.as_object_mut()
    {
        object.insert("inferenceConfig".to_string(), inference_config);
    }
    serde_json::to_vec(&request)
        .map_err(|e| UpstreamError::Channel(format!("serialize kiro request body: {e}")))
}

fn reject_openai_response_features_without_kiro_mapping(
    value: &Value,
) -> Result<(), UpstreamError> {
    if value.get("response_format").is_some() || has_structured_text_format(value.get("text")) {
        return Err(UpstreamError::Channel(
            "kiro channel does not yet support structured-output conversion".into(),
        ));
    }
    Ok(())
}

fn has_structured_text_format(text: Option<&Value>) -> bool {
    let Some(text) = text else {
        return false;
    };
    let Some(format_type) = text
        .get("format")
        .and_then(|format| format.get("type").or_else(|| format.get("name")))
        .and_then(Value::as_str)
    else {
        return false;
    };
    !matches!(format_type, "text")
}

fn openai_response_to_kiro_conversation_state(
    value: &Value,
    model: Option<&str>,
    settings: &KiroSettings,
) -> Result<Value, UpstreamError> {
    let mut messages = Vec::new();
    let instructions = text_from_optional_value(value.get("instructions"))?;
    let request_model = model
        .or_else(|| value.get("model").and_then(Value::as_str))
        .map(map_kiro_model);
    let origin = effective_kiro_origin(settings);
    let input = value.get("input").ok_or_else(|| {
        UpstreamError::Channel("kiro request conversion requires OpenAI Responses input".into())
    })?;

    match input {
        Value::String(text) => {
            if let Some(instructions) = instructions.as_deref() {
                push_kiro_system_priming(
                    &mut messages,
                    instructions,
                    request_model.as_deref(),
                    origin,
                );
            }
            messages.push(kiro_user_message(
                fallback_kiro_user_content(text, false),
                request_model.as_deref(),
                Vec::new(),
                origin,
            ));
        }
        Value::Array(items) => {
            let mut pending_system = instructions.unwrap_or_default();
            for item in items {
                let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
                let (text, images) =
                    text_and_images_from_content(item.get("content").unwrap_or(item))?;
                if matches!(role, "system" | "developer") {
                    pending_system =
                        join_nonempty([Some(pending_system.as_str()), Some(text.as_str())]);
                    continue;
                }
                if !pending_system.is_empty() && role == "user" {
                    push_kiro_system_priming(
                        &mut messages,
                        pending_system.as_str(),
                        request_model.as_deref(),
                        origin,
                    );
                    pending_system.clear();
                }

                match role {
                    "assistant" => messages.push(kiro_assistant_message(text)),
                    "user" => {
                        let content = fallback_kiro_user_content(&text, !images.is_empty());
                        messages.push(kiro_user_message(
                            content,
                            request_model.as_deref(),
                            images,
                            origin,
                        ));
                    }
                    other => {
                        return Err(UpstreamError::Channel(format!(
                            "kiro channel does not support OpenAI message role '{other}'"
                        )));
                    }
                }
            }
            if !pending_system.is_empty() {
                push_kiro_system_priming(
                    &mut messages,
                    pending_system.as_str(),
                    request_model.as_deref(),
                    origin,
                );
            }
        }
        Value::Object(_) => {
            let (text, images) =
                text_and_images_from_content(input.get("content").unwrap_or(input))?;
            if let Some(instructions) = instructions.as_deref() {
                push_kiro_system_priming(
                    &mut messages,
                    instructions,
                    request_model.as_deref(),
                    origin,
                );
            }
            messages.push(kiro_user_message(
                fallback_kiro_user_content(&text, !images.is_empty()),
                request_model.as_deref(),
                images,
                origin,
            ));
        }
        _ => {
            return Err(UpstreamError::Channel(
                "kiro channel only supports text OpenAI Responses input".into(),
            ));
        }
    }

    let Some(mut current_message) = messages.pop() else {
        return Err(UpstreamError::Channel(
            "kiro request conversion produced no messages".into(),
        ));
    };
    if current_message.get("userInputMessage").is_none() {
        return Err(UpstreamError::Channel(
            "kiro request conversion requires the final message to be a user message".into(),
        ));
    }
    let tools = kiro_tools_from_openai_response(value.get("tools"))?;
    if !tools.is_empty() {
        insert_kiro_tools(&mut current_message, tools);
    }

    let conversation_id = value
        .get("conversation")
        .and_then(|conversation| conversation.get("id"))
        .and_then(Value::as_str)
        .or_else(|| value.get("previous_response_id").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    Ok(json!({
        "conversationId": conversation_id,
        "history": messages,
        "currentMessage": current_message,
        "chatTriggerType": "MANUAL",
        "agentTaskType": effective_kiro_agent_task_type(settings),
        "agentContinuationId": uuid::Uuid::new_v4().to_string()
    }))
}

fn kiro_tools_from_openai_response(tools: Option<&Value>) -> Result<Vec<Value>, UpstreamError> {
    let Some(tools) = tools.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut converted = Vec::new();
    for tool in tools {
        let function = tool.get("function");
        let tool_type = tool.get("type").and_then(Value::as_str);
        if tool_type.is_some_and(|value| value != "function")
            && function.is_none()
            && tool.get("name").is_none()
        {
            continue;
        }
        let name = function
            .and_then(|value| value.get("name"))
            .or_else(|| tool.get("name"))
            .and_then(Value::as_str)
            .ok_or_else(|| UpstreamError::Channel("kiro tool conversion requires name".into()))?;
        let sanitized_name = shorten_kiro_tool_name(&sanitize_kiro_tool_name(name));
        let description = function
            .and_then(|value| value.get("description"))
            .or_else(|| tool.get("description"))
            .and_then(Value::as_str)
            .map(truncate_kiro_tool_description)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("Tool: {sanitized_name}"));
        let schema = function
            .and_then(|value| value.get("parameters"))
            .or_else(|| tool.get("parameters"))
            .or_else(|| tool.get("input_schema"))
            .or_else(|| tool.get("inputSchema"));
        converted.push(json!({
            "toolSpecification": {
                "name": sanitized_name,
                "description": description,
                "inputSchema": {
                    "json": ensure_kiro_object_schema(schema)
                }
            }
        }));
    }
    Ok(converted)
}

fn insert_kiro_tools(current_message: &mut Value, tools: Vec<Value>) {
    let Some(user_input) = current_message
        .get_mut("userInputMessage")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let context = user_input
        .entry("userInputMessageContext".to_string())
        .or_insert_with(|| json!({ "editorState": {} }));
    if let Some(context) = context.as_object_mut() {
        context.insert("tools".to_string(), Value::Array(tools));
    }
}

fn ensure_kiro_object_schema(schema: Option<&Value>) -> Value {
    let mut schema = schema
        .cloned()
        .unwrap_or_else(|| json!({ "type": "object" }));
    if !schema.is_object() {
        return json!({ "type": "object" });
    }
    clean_kiro_schema(&mut schema);
    if let Some(object) = schema.as_object_mut() {
        object
            .entry("type".to_string())
            .or_insert_with(|| Value::String("object".to_string()));
    }
    schema
}

fn clean_kiro_schema(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("additionalProperties");
            if let Some(required) = object.get("required")
                && required.as_array().is_none_or(|items| items.is_empty())
            {
                object.remove("required");
            }
            for child in object.values_mut() {
                clean_kiro_schema(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                clean_kiro_schema(item);
            }
        }
        _ => {}
    }
}

fn sanitize_kiro_tool_name(name: &str) -> String {
    let mut out = String::new();
    for (index, part) in name
        .split(|ch: char| {
            ch == '_' || ch == '-' || ch == ' ' || ch == '.' || ch == '/' || ch == ':'
        })
        .filter(|part| !part.is_empty())
        .enumerate()
    {
        let mut chars = part.chars().filter(|ch| ch.is_ascii_alphanumeric());
        let Some(first) = chars.next() else {
            continue;
        };
        if index == 0 {
            out.push(first.to_ascii_lowercase());
        } else {
            out.push(first.to_ascii_uppercase());
        }
        out.extend(chars);
    }
    if out.is_empty() {
        "tool".to_string()
    } else {
        out
    }
}

fn shorten_kiro_tool_name(name: &str) -> String {
    if name.chars().count() <= 64 {
        return name.to_string();
    }
    name.chars().take(64).collect()
}

fn truncate_kiro_tool_description(desc: &str) -> String {
    if desc.chars().count() <= MAX_KIRO_TOOL_DESCRIPTION_LEN {
        return desc.to_string();
    }
    let mut truncated = desc
        .chars()
        .take(MAX_KIRO_TOOL_DESCRIPTION_LEN)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn text_from_optional_value(value: Option<&Value>) -> Result<Option<String>, UpstreamError> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok((!text.is_empty()).then(|| text.clone())),
        _ => Ok(Some(text_and_images_from_content(value)?.0)),
    }
}

fn text_and_images_from_content(value: &Value) -> Result<(String, Vec<Value>), UpstreamError> {
    match value {
        Value::String(text) => Ok((text.clone(), Vec::new())),
        Value::Array(items) => {
            let mut text_parts = Vec::new();
            let mut images = Vec::new();
            for item in items {
                let (text, mut item_images) = text_and_images_from_content(item)?;
                if !text.is_empty() {
                    text_parts.push(text);
                }
                images.append(&mut item_images);
            }
            Ok((text_parts.join("\n"), images))
        }
        Value::Object(object) => {
            let type_ = object.get("type").and_then(Value::as_str);
            match type_ {
                Some("input_text" | "text" | "output_text") => Ok((
                    object
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    Vec::new(),
                )),
                Some("image_url" | "input_image") => {
                    let image_url = object
                        .get("image_url")
                        .and_then(|value| {
                            value
                                .as_str()
                                .or_else(|| value.get("url").and_then(Value::as_str))
                        })
                        .ok_or_else(|| {
                            UpstreamError::Channel(
                                "kiro image conversion requires image_url data URL".into(),
                            )
                        })?;
                    let image = kiro_image_block_from_data_url(image_url)?;
                    Ok((String::new(), vec![image]))
                }
                Some(other) => Err(UpstreamError::Channel(format!(
                    "kiro channel does not support OpenAI content part type '{other}'"
                ))),
                None => {
                    if let Some(content) = object.get("content") {
                        text_and_images_from_content(content)
                    } else {
                        Ok((String::new(), Vec::new()))
                    }
                }
            }
        }
        Value::Null => Ok((String::new(), Vec::new())),
        _ => Err(UpstreamError::Channel(
            "kiro channel only supports text/image content conversion".into(),
        )),
    }
}

fn kiro_user_message(
    content: String,
    model: Option<&str>,
    images: Vec<Value>,
    origin: &str,
) -> Value {
    let mut message = json!({
        "origin": origin,
        "content": content,
        "userInputMessageContext": {
            "editorState": {}
        }
    });
    if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty())
        && let Some(object) = message.as_object_mut()
    {
        object.insert("modelId".to_string(), Value::String(model.to_string()));
    }
    if !images.is_empty()
        && let Some(object) = message.as_object_mut()
    {
        object.insert("images".to_string(), Value::Array(images));
    }
    json!({ "userInputMessage": message })
}

fn kiro_assistant_message(content: String) -> Value {
    json!({ "assistantResponseMessage": { "content": content } })
}

fn push_kiro_system_priming(
    messages: &mut Vec<Value>,
    system_prompt: &str,
    model: Option<&str>,
    origin: &str,
) {
    let system_prompt = system_prompt.trim();
    if system_prompt.is_empty() {
        return;
    }
    messages.push(kiro_user_message(
        system_prompt.to_string(),
        model,
        Vec::new(),
        origin,
    ));
    messages.push(kiro_assistant_message(
        "I will follow these instructions.".to_string(),
    ));
}

fn fallback_kiro_user_content(text: &str, has_images: bool) -> String {
    let text = text.trim();
    if !text.is_empty() {
        return text.to_string();
    }
    if has_images {
        "Please analyze the attached image.".to_string()
    } else {
        ".".to_string()
    }
}

fn join_nonempty<'a>(parts: impl IntoIterator<Item = Option<&'a str>>) -> String {
    parts
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn kiro_image_block_from_data_url(data_url: &str) -> Result<Value, UpstreamError> {
    let (metadata, bytes) = data_url.split_once(',').ok_or_else(|| {
        UpstreamError::Channel("kiro image conversion only supports data URLs".into())
    })?;
    if !metadata.starts_with("data:image/") {
        return Err(UpstreamError::Channel(
            "kiro image conversion only supports image data URLs".into(),
        ));
    }
    let format = metadata
        .strip_prefix("data:image/")
        .and_then(|value| value.split(';').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("jpeg")
        .to_ascii_lowercase();
    Ok(json!({
        "format": format,
        "source": { "bytes": bytes }
    }))
}

fn map_kiro_model(model: &str) -> String {
    let lower = model.to_ascii_lowercase().replace('_', "-");
    for (needle, replacement) in [
        ("claude-sonnet-4-20250514", "claude-sonnet-4"),
        ("claude-sonnet-4-5", "claude-sonnet-4.5"),
        ("claude-sonnet-4.5", "claude-sonnet-4.5"),
        ("claude-sonnet-4-6", "claude-sonnet-4.6"),
        ("claude-sonnet-4.6", "claude-sonnet-4.6"),
        ("claude-opus-4-7", "claude-opus-4.7"),
        ("claude-opus-4.7", "claude-opus-4.7"),
        ("claude-haiku-4-5", "claude-haiku-4.5"),
        ("claude-haiku-4.5", "claude-haiku-4.5"),
        ("claude-opus-4-5", "claude-opus-4.5"),
        ("claude-opus-4.5", "claude-opus-4.5"),
        ("claude-opus-4-6", "claude-opus-4.6"),
        ("claude-opus-4.6", "claude-opus-4.6"),
        ("claude-sonnet-4", "claude-sonnet-4"),
        ("claude-3-5-sonnet", "claude-sonnet-4.5"),
        ("claude-3-opus", "claude-sonnet-4.5"),
        ("claude-3-sonnet", "claude-sonnet-4"),
        ("claude-3-haiku", "claude-haiku-4.5"),
        ("gpt-4-turbo", "claude-sonnet-4.5"),
        ("gpt-4o", "claude-sonnet-4.5"),
        ("gpt-4", "claude-sonnet-4.5"),
        ("gpt-3.5-turbo", "claude-sonnet-4.5"),
    ] {
        if lower.contains(needle) {
            return replacement.to_string();
        }
    }
    model.to_string()
}

fn inference_config_from_openai_response(value: &Value) -> Option<Value> {
    let mut object = serde_json::Map::new();
    if let Some(max_tokens) = value.get("max_output_tokens").and_then(Value::as_u64) {
        object.insert("maxTokens".to_string(), json!(max_tokens));
    }
    if let Some(temperature) = value.get("temperature").and_then(Value::as_f64) {
        object.insert("temperature".to_string(), json!(temperature));
    }
    if let Some(top_p) = value.get("top_p").and_then(Value::as_f64) {
        object.insert("topP".to_string(), json!(top_p));
    }
    (!object.is_empty()).then_some(Value::Object(object))
}

fn kiro_model_list_to_openai_model_list(body: &[u8]) -> Vec<u8> {
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(models) = payload
        .get("models")
        .or_else(|| payload.get("data"))
        .and_then(Value::as_array)
    else {
        return body.to_vec();
    };
    let created = unix_timestamp_secs();
    let data = models
        .iter()
        .filter_map(kiro_model_id)
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": created,
                "owned_by": "amazon",
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({ "object": "list", "data": data })).unwrap_or_else(|_| body.to_vec())
}

fn kiro_model_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) => Some(id.clone()),
        Value::Object(_) => value
            .get("modelId")
            .or_else(|| value.get("model_id"))
            .or_else(|| value.get("id"))
            .or_else(|| value.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

#[derive(Debug)]
struct KiroEvent {
    event_type: Option<String>,
    payload: Value,
}

#[derive(Debug, Default)]
struct KiroResponseParts {
    content: String,
    reasoning_content: String,
    conversation_id: Option<String>,
    model_id: Option<String>,
    usage: Option<Value>,
    error: Option<String>,
    last_assistant_content: String,
    last_reasoning_content: String,
}

#[derive(Debug)]
struct KiroStreamState {
    pending: Vec<u8>,
    parts: KiroResponseParts,
    response_id: String,
    message_id: String,
    reasoning_id: String,
    model: String,
    initialized: bool,
    content_started: bool,
    reasoning_started: bool,
    sequence_number: u64,
}

impl KiroStreamState {
    fn new(request: &PreparedRequest) -> Self {
        let response_id = request_conversation_id(request)
            .unwrap_or_else(|| format!("resp_{}", uuid::Uuid::new_v4().simple()));
        Self {
            pending: Vec::new(),
            parts: KiroResponseParts::default(),
            response_id,
            message_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            reasoning_id: format!("rs_{}", uuid::Uuid::new_v4().simple()),
            model: request.model.clone().unwrap_or_else(|| "kiro".to_string()),
            initialized: false,
            content_started: false,
            reasoning_started: false,
            sequence_number: 0,
        }
    }

    fn next_sequence(&mut self) -> u64 {
        let sequence = self.sequence_number;
        self.sequence_number += 1;
        sequence
    }
}

fn kiro_stream_states() -> &'static DashMap<String, Mutex<KiroStreamState>> {
    static STATES: OnceLock<DashMap<String, Mutex<KiroStreamState>>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn kiro_stream_state_key(request: &PreparedRequest) -> String {
    request
        .headers
        .get(KIRO_STREAM_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| request_conversation_id(request))
        .unwrap_or_else(|| "kiro-stream-default".to_string())
}

fn normalize_kiro_stream_chunk(request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
    let key = kiro_stream_state_key(request);
    if !body.is_empty()
        && !kiro_stream_states().contains_key(&key)
        && looks_like_normalized_stream_chunk(&body)
    {
        return body;
    }
    if body.is_empty() {
        let Some((_, state_mutex)) = kiro_stream_states().remove(&key) else {
            return Vec::new();
        };
        let mut state = match state_mutex.into_inner() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        return finish_kiro_stream_state(&mut state);
    }

    let state_ref = kiro_stream_states()
        .entry(key)
        .or_insert_with(|| Mutex::new(KiroStreamState::new(request)));
    let Ok(mut state) = state_ref.lock() else {
        return Vec::new();
    };
    if !body.is_empty() && state.pending.is_empty() && looks_like_normalized_stream_chunk(&body) {
        return body;
    }

    let mut out = Vec::new();
    emit_kiro_stream_start(&mut state, &mut out);
    state.pending.extend_from_slice(&body);
    match drain_aws_eventstream(&mut state.pending) {
        Ok(events) => {
            for event in events {
                emit_kiro_stream_event(&mut state, event, &mut out);
            }
        }
        Err(error) => {
            push_sse_json(
                &mut out,
                json!({
                    "type": "error",
                    "sequence_number": state.next_sequence(),
                    "error": {
                        "type": "kiro_eventstream_error",
                        "code": "kiro_eventstream_error",
                        "message": error.to_string()
                    }
                }),
            );
        }
    }
    out
}

fn looks_like_normalized_stream_chunk(body: &[u8]) -> bool {
    let Some(first) = body
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
    else {
        return true;
    };
    matches!(first, b'{' | b'[' | b'd')
}

fn emit_kiro_stream_start(state: &mut KiroStreamState, out: &mut Vec<u8>) {
    if state.initialized {
        return;
    }
    state.initialized = true;
    push_sse_json(
        out,
        json!({
            "type": "response.created",
            "sequence_number": state.next_sequence(),
            "response": openai_response_stream_body(state, "in_progress", false)
        }),
    );
}

fn emit_kiro_message_start(state: &mut KiroStreamState, out: &mut Vec<u8>) {
    if state.content_started {
        return;
    }
    state.content_started = true;
    push_sse_json(
        out,
        json!({
            "type": "response.output_item.added",
            "sequence_number": state.next_sequence(),
            "output_index": 0,
            "item": openai_message_item(&state.message_id, "", "in_progress")
        }),
    );
    push_sse_json(
        out,
        json!({
            "type": "response.content_part.added",
            "sequence_number": state.next_sequence(),
            "output_index": 0,
            "item_id": state.message_id,
            "content_index": 0,
            "part": { "type": "output_text", "text": "", "annotations": [] }
        }),
    );
}

fn emit_kiro_reasoning_start(state: &mut KiroStreamState, out: &mut Vec<u8>) {
    if state.reasoning_started {
        return;
    }
    state.reasoning_started = true;
    push_sse_json(
        out,
        json!({
            "type": "response.output_item.added",
            "sequence_number": state.next_sequence(),
            "output_index": 1,
            "item": openai_reasoning_item(&state.reasoning_id, "", "in_progress")
        }),
    );
}

fn emit_kiro_stream_event(state: &mut KiroStreamState, event: KiroEvent, out: &mut Vec<u8>) {
    let Some(event_type) = event.event_type.as_deref() else {
        return;
    };
    let payload = kiro_event_payload(&event);
    if let Some(usage) = find_kiro_usage(payload) {
        state.parts.usage = Some(usage);
    }

    match event_type {
        "assistantResponseEvent" => {
            if let Some(content) = payload.get("content").and_then(Value::as_str) {
                let delta = decode_kiro_text(&normalize_kiro_chunk(
                    content,
                    &mut state.parts.last_assistant_content,
                ));
                if !delta.is_empty() {
                    emit_kiro_message_start(state, out);
                    state.parts.content.push_str(&delta);
                    push_sse_json(
                        out,
                        json!({
                            "type": "response.output_text.delta",
                            "sequence_number": state.next_sequence(),
                            "output_index": 0,
                            "item_id": state.message_id,
                            "content_index": 0,
                            "delta": delta
                        }),
                    );
                }
            }
            if state.parts.model_id.is_none() {
                state.parts.model_id = payload
                    .get("modelId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
        "reasoningContentEvent" => {
            if let Some(text) = payload
                .get("text")
                .or_else(|| payload.get("content"))
                .and_then(Value::as_str)
            {
                let delta = decode_kiro_text(&normalize_kiro_chunk(
                    text,
                    &mut state.parts.last_reasoning_content,
                ));
                if !delta.is_empty() {
                    emit_kiro_reasoning_start(state, out);
                    state.parts.reasoning_content.push_str(&delta);
                    push_sse_json(
                        out,
                        json!({
                            "type": "response.reasoning_text.delta",
                            "sequence_number": state.next_sequence(),
                            "output_index": 1,
                            "item_id": state.reasoning_id,
                            "content_index": 0,
                            "delta": delta
                        }),
                    );
                }
            }
        }
        "messageMetadataEvent" if state.parts.conversation_id.is_none() => {
            state.parts.conversation_id = payload
                .get("conversationId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        "metadataEvent" if state.parts.usage.is_none() => {
            state.parts.usage = payload.get("tokenUsage").cloned();
        }
        "invalidStateEvent" | "InternalServerException" | "internalServerException" => {
            let message = payload
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| payload.get("reason").and_then(Value::as_str))
                .unwrap_or("kiro upstream stream error");
            push_sse_json(
                out,
                json!({
                    "type": "error",
                    "sequence_number": state.next_sequence(),
                    "error": {
                        "type": "kiro_error",
                        "code": "kiro_eventstream_error",
                        "message": message
                    }
                }),
            );
        }
        _ => {}
    }
}

fn finish_kiro_stream_state(state: &mut KiroStreamState) -> Vec<u8> {
    let mut out = Vec::new();
    emit_kiro_stream_start(state, &mut out);
    if state.reasoning_started {
        push_sse_json(
            &mut out,
            json!({
                "type": "response.reasoning_text.done",
                "sequence_number": state.next_sequence(),
                "output_index": 1,
                "item_id": state.reasoning_id,
                "content_index": 0,
                "text": state.parts.reasoning_content
            }),
        );
        push_sse_json(
            &mut out,
            json!({
                "type": "response.output_item.done",
                "sequence_number": state.next_sequence(),
                "output_index": 1,
                "item": openai_reasoning_item(&state.reasoning_id, &state.parts.reasoning_content, "completed")
            }),
        );
    }
    if state.content_started {
        push_sse_json(
            &mut out,
            json!({
                "type": "response.output_text.done",
                "sequence_number": state.next_sequence(),
                "output_index": 0,
                "item_id": state.message_id,
                "content_index": 0,
                "text": state.parts.content
            }),
        );
        push_sse_json(
            &mut out,
            json!({
                "type": "response.content_part.done",
                "sequence_number": state.next_sequence(),
                "output_index": 0,
                "item_id": state.message_id,
                "content_index": 0,
                "part": { "type": "output_text", "text": state.parts.content, "annotations": [] }
            }),
        );
        push_sse_json(
            &mut out,
            json!({
                "type": "response.output_item.done",
                "sequence_number": state.next_sequence(),
                "output_index": 0,
                "item": openai_message_item(&state.message_id, &state.parts.content, "completed")
            }),
        );
    }
    push_sse_json(
        &mut out,
        json!({
            "type": "response.completed",
            "sequence_number": state.next_sequence(),
            "response": openai_response_stream_body(state, "completed", true)
        }),
    );
    out
}

fn push_sse_json(out: &mut Vec<u8>, value: Value) {
    out.extend_from_slice(b"data: ");
    if let Ok(bytes) = serde_json::to_vec(&value) {
        out.extend(bytes);
    } else {
        out.extend_from_slice(
            br#"{"type":"error","error":{"message":"serialize stream event failed"}}"#,
        );
    }
    out.extend_from_slice(b"\n\n");
}

fn openai_response_stream_body(
    state: &KiroStreamState,
    status: &str,
    include_output: bool,
) -> Value {
    let mut output = Vec::new();
    if include_output && state.reasoning_started {
        output.push(openai_reasoning_item(
            &state.reasoning_id,
            &state.parts.reasoning_content,
            "completed",
        ));
    }
    if include_output && state.content_started {
        output.push(openai_message_item(
            &state.message_id,
            &state.parts.content,
            "completed",
        ));
    }
    let mut response = json!({
        "id": state.response_id,
        "created_at": unix_timestamp_secs(),
        "metadata": {},
        "model": state.parts.model_id.as_deref().unwrap_or(&state.model),
        "object": "response",
        "output": output,
        "parallel_tool_calls": false,
        "temperature": 1.0,
        "tool_choice": "auto",
        "tools": [],
        "top_p": 1.0,
        "output_text": state.parts.content,
        "status": status
    });
    if include_output
        && let Some(usage) = state
            .parts
            .usage
            .clone()
            .and_then(openai_usage_from_kiro_usage)
        && let Some(object) = response.as_object_mut()
    {
        object.insert("usage".to_string(), usage);
    }
    response
}

fn openai_message_item(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id,
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": text,
            "annotations": []
        }]
    })
}

fn openai_reasoning_item(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id,
        "type": "reasoning",
        "status": status,
        "summary": [],
        "content": [{
            "type": "reasoning_text",
            "text": text
        }]
    })
}

fn drain_aws_eventstream(pending: &mut Vec<u8>) -> Result<Vec<KiroEvent>, UpstreamError> {
    let mut events = Vec::new();
    let mut offset = 0usize;
    while pending.len().saturating_sub(offset) >= 12 {
        let total_len = u32::from_be_bytes([
            pending[offset],
            pending[offset + 1],
            pending[offset + 2],
            pending[offset + 3],
        ]) as usize;
        if total_len < 16 {
            return Err(UpstreamError::Channel(
                "invalid AWS eventstream frame length".into(),
            ));
        }
        if pending.len().saturating_sub(offset) < total_len {
            break;
        }
        events.extend(decode_aws_eventstream(
            &pending[offset..offset + total_len],
        )?);
        offset += total_len;
    }
    if offset > 0 {
        pending.drain(..offset);
    }
    Ok(events)
}

fn looks_like_aws_eventstream(body: &[u8]) -> bool {
    if body.len() < 16 {
        return false;
    }
    let total_len = u32::from_be_bytes([body[0], body[1], body[2], body[3]]) as usize;
    (16..=body.len()).contains(&total_len)
}

fn kiro_eventstream_to_openai_response(
    request: &PreparedRequest,
    body: &[u8],
) -> Result<Vec<u8>, UpstreamError> {
    let events = decode_aws_eventstream(body)?;
    let mut parts = KiroResponseParts::default();
    for event in events {
        apply_kiro_event(&mut parts, event);
    }
    if let Some(error) = parts.error {
        return serde_json::to_vec(&json!({
            "error": {
                "message": error,
                "type": "kiro_error",
                "code": "kiro_eventstream_error"
            }
        }))
        .map_err(|e| UpstreamError::Channel(format!("serialize kiro error response: {e}")));
    }

    let text = decode_kiro_text(&parts.content);
    let id = parts
        .conversation_id
        .or_else(|| request_conversation_id(request))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let model = parts
        .model_id
        .or_else(|| request.model.clone())
        .unwrap_or_else(|| "kiro".to_string());
    let created_at = unix_timestamp_secs();
    let usage = parts.usage.and_then(openai_usage_from_kiro_usage);

    let mut response = json!({
        "id": id,
        "created_at": created_at,
        "metadata": {},
        "model": model,
        "object": "response",
        "output": [{
            "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": text,
                "annotations": []
            }]
        }],
        "parallel_tool_calls": false,
        "temperature": 1.0,
        "tool_choice": "auto",
        "tools": [],
        "top_p": 1.0,
        "output_text": text,
        "status": "completed"
    });
    if let Some(usage) = usage
        && let Some(object) = response.as_object_mut()
    {
        object.insert("usage".to_string(), usage);
    }
    serde_json::to_vec(&response)
        .map_err(|e| UpstreamError::Channel(format!("serialize openai response body: {e}")))
}

fn decode_aws_eventstream(body: &[u8]) -> Result<Vec<KiroEvent>, UpstreamError> {
    let mut events = Vec::new();
    let mut offset = 0usize;
    while offset < body.len() {
        if body.len().saturating_sub(offset) < 16 {
            return Err(UpstreamError::Channel(
                "truncated AWS eventstream frame".into(),
            ));
        }
        let total_len = u32::from_be_bytes([
            body[offset],
            body[offset + 1],
            body[offset + 2],
            body[offset + 3],
        ]) as usize;
        let headers_len = u32::from_be_bytes([
            body[offset + 4],
            body[offset + 5],
            body[offset + 6],
            body[offset + 7],
        ]) as usize;
        if total_len < 16 || offset + total_len > body.len() {
            return Err(UpstreamError::Channel(
                "invalid AWS eventstream frame length".into(),
            ));
        }
        let headers_start = offset + 12;
        let headers_end = headers_start + headers_len;
        let payload_end = offset + total_len - 4;
        if headers_end > payload_end {
            return Err(UpstreamError::Channel(
                "invalid AWS eventstream headers length".into(),
            ));
        }
        let headers = parse_aws_eventstream_headers(&body[headers_start..headers_end])?;
        let payload = if headers_end == payload_end {
            Value::Null
        } else {
            serde_json::from_slice(&body[headers_end..payload_end]).map_err(|e| {
                UpstreamError::Channel(format!("deserialize kiro event payload: {e}"))
            })?
        };
        events.push(KiroEvent {
            event_type: headers.get(":event-type").cloned(),
            payload,
        });
        offset += total_len;
    }
    Ok(events)
}

fn parse_aws_eventstream_headers(
    mut bytes: &[u8],
) -> Result<std::collections::BTreeMap<String, String>, UpstreamError> {
    let mut headers = std::collections::BTreeMap::new();
    while !bytes.is_empty() {
        let name_len = bytes[0] as usize;
        bytes = &bytes[1..];
        if bytes.len() < name_len + 1 {
            return Err(UpstreamError::Channel(
                "truncated AWS eventstream header".into(),
            ));
        }
        let name = std::str::from_utf8(&bytes[..name_len])
            .map_err(|e| UpstreamError::Channel(format!("decode event header name: {e}")))?
            .to_string();
        bytes = &bytes[name_len..];
        let value_type = bytes[0];
        bytes = &bytes[1..];
        match value_type {
            0 => {
                headers.insert(name, "true".to_string());
            }
            1 => {
                headers.insert(name, "false".to_string());
            }
            2 => {
                if bytes.is_empty() {
                    return Err(UpstreamError::Channel("truncated byte event header".into()));
                }
                headers.insert(name, bytes[0].to_string());
                bytes = &bytes[1..];
            }
            3 => {
                if bytes.len() < 2 {
                    return Err(UpstreamError::Channel(
                        "truncated short event header".into(),
                    ));
                }
                headers.insert(name, i16::from_be_bytes([bytes[0], bytes[1]]).to_string());
                bytes = &bytes[2..];
            }
            4 => {
                if bytes.len() < 4 {
                    return Err(UpstreamError::Channel(
                        "truncated integer event header".into(),
                    ));
                }
                headers.insert(
                    name,
                    i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
                );
                bytes = &bytes[4..];
            }
            5 | 8 => {
                if bytes.len() < 8 {
                    return Err(UpstreamError::Channel("truncated long event header".into()));
                }
                headers.insert(
                    name,
                    i64::from_be_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
                        bytes[7],
                    ])
                    .to_string(),
                );
                bytes = &bytes[8..];
            }
            6 | 7 => {
                if bytes.len() < 2 {
                    return Err(UpstreamError::Channel(
                        "truncated string event header".into(),
                    ));
                }
                let len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
                bytes = &bytes[2..];
                if bytes.len() < len {
                    return Err(UpstreamError::Channel(
                        "truncated string event header".into(),
                    ));
                }
                if value_type == 7 {
                    let value = std::str::from_utf8(&bytes[..len])
                        .map_err(|e| UpstreamError::Channel(format!("decode event header: {e}")))?
                        .to_string();
                    headers.insert(name, value);
                }
                bytes = &bytes[len..];
            }
            9 => {
                if bytes.len() < 16 {
                    return Err(UpstreamError::Channel("truncated uuid event header".into()));
                }
                bytes = &bytes[16..];
            }
            other => {
                return Err(UpstreamError::Channel(format!(
                    "unsupported AWS eventstream header value type {other}"
                )));
            }
        }
    }
    Ok(headers)
}

fn apply_kiro_event(parts: &mut KiroResponseParts, event: KiroEvent) {
    let Some(event_type) = event.event_type.as_deref() else {
        return;
    };
    let payload = kiro_event_payload(&event);
    if let Some(usage) = find_kiro_usage(payload) {
        parts.usage = Some(usage);
    }
    match event_type {
        "assistantResponseEvent" => {
            if let Some(content) = payload.get("content").and_then(Value::as_str) {
                let delta = decode_kiro_text(&normalize_kiro_chunk(
                    content,
                    &mut parts.last_assistant_content,
                ));
                parts.content.push_str(&delta);
            }
            if parts.model_id.is_none() {
                parts.model_id = payload
                    .get("modelId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
        "reasoningContentEvent" => {
            if let Some(text) = payload
                .get("text")
                .or_else(|| payload.get("content"))
                .and_then(Value::as_str)
            {
                let delta = decode_kiro_text(&normalize_kiro_chunk(
                    text,
                    &mut parts.last_reasoning_content,
                ));
                parts.reasoning_content.push_str(&delta);
            }
        }
        "messageMetadataEvent" if parts.conversation_id.is_none() => {
            parts.conversation_id = payload
                .get("conversationId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        "metadataEvent" if parts.usage.is_none() => {
            parts.usage = payload.get("tokenUsage").cloned();
        }
        "invalidStateEvent" | "InternalServerException" | "internalServerException" => {
            parts.error = payload
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| payload.get("reason").and_then(Value::as_str))
                .map(str::to_string);
        }
        _ => {}
    }
}

fn kiro_event_payload(event: &KiroEvent) -> &Value {
    event
        .event_type
        .as_deref()
        .and_then(|event_type| event.payload.get(event_type))
        .unwrap_or(&event.payload)
}

fn find_kiro_usage(value: &Value) -> Option<Value> {
    if has_any_kiro_usage_key(value) {
        return Some(value.clone());
    }
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let lower = key.to_ascii_lowercase();
                if matches!(lower.as_str(), "usage" | "tokenusage" | "token_usage")
                    && child.is_object()
                {
                    return Some(child.clone());
                }
                if let Some(usage) = find_kiro_usage(child) {
                    return Some(usage);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(find_kiro_usage),
        _ => None,
    }
}

fn has_any_kiro_usage_key(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        [
            "inputTokens",
            "promptTokens",
            "totalInputTokens",
            "uncachedInputTokens",
            "cacheReadInputTokens",
            "cacheWriteInputTokens",
            "cacheCreationInputTokens",
            "outputTokens",
            "completionTokens",
            "totalOutputTokens",
            "totalTokens",
            "input_tokens",
            "prompt_tokens",
            "total_input_tokens",
            "uncached_input_tokens",
            "cache_read_input_tokens",
            "cache_write_input_tokens",
            "cache_creation_input_tokens",
            "output_tokens",
            "completion_tokens",
            "total_output_tokens",
            "total_tokens",
        ]
        .iter()
        .any(|key| object.contains_key(*key))
    })
}

fn openai_usage_from_kiro_usage(usage: Value) -> Option<Value> {
    let output = value_u64_any(
        &usage,
        &[
            "outputTokens",
            "completionTokens",
            "totalOutputTokens",
            "output_tokens",
            "completion_tokens",
            "total_output_tokens",
        ],
    )
    .unwrap_or(0);
    let cache_read =
        value_u64_any(&usage, &["cacheReadInputTokens", "cache_read_input_tokens"]).unwrap_or(0);
    let cache_write = value_u64_any(
        &usage,
        &[
            "cacheWriteInputTokens",
            "cacheCreationInputTokens",
            "cache_write_input_tokens",
            "cache_creation_input_tokens",
        ],
    )
    .unwrap_or(0);
    let uncached =
        value_u64_any(&usage, &["uncachedInputTokens", "uncached_input_tokens"]).unwrap_or(0);
    let input = value_u64_any(
        &usage,
        &[
            "inputTokens",
            "promptTokens",
            "totalInputTokens",
            "input_tokens",
            "prompt_tokens",
            "total_input_tokens",
        ],
    )
    .unwrap_or_else(|| {
        let cache_total = uncached + cache_read + cache_write;
        if cache_total > 0 {
            cache_total
        } else {
            value_u64_any(&usage, &["totalTokens", "total_tokens"])
                .and_then(|total| total.checked_sub(output))
                .unwrap_or(0)
        }
    });
    let total = value_u64_any(&usage, &["totalTokens", "total_tokens"]).unwrap_or(input + output);
    Some(json!({
        "input_tokens": input,
        "input_tokens_details": {
            "cached_tokens": cache_read
        },
        "output_tokens": output,
        "output_tokens_details": {
            "reasoning_tokens": 0
        },
        "total_tokens": total
    }))
}

fn value_u64_any(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(json_u64))
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| {
            value
                .as_f64()
                .filter(|number| number.is_finite() && *number >= 0.0)
                .map(|number| number as u64)
        })
        .or_else(|| {
            value.as_str().and_then(|text| {
                text.parse::<u64>()
                    .ok()
                    .or_else(|| text.parse::<f64>().ok().map(|number| number as u64))
            })
        })
}

fn request_conversation_id(request: &PreparedRequest) -> Option<String> {
    serde_json::from_slice::<Value>(&request.body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/conversationState/conversationId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn decode_kiro_text(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            out.push((high << 4) | low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

fn normalize_kiro_chunk(chunk: &str, previous: &mut String) -> String {
    if chunk.is_empty() {
        return String::new();
    }
    if previous.is_empty() {
        *previous = chunk.to_string();
        return chunk.to_string();
    }

    let prev = previous.as_bytes();
    let current = chunk.as_bytes();
    if current == prev {
        return String::new();
    }
    if current.starts_with(prev) {
        let delta = String::from_utf8_lossy(&current[prev.len()..]).into_owned();
        *previous = chunk.to_string();
        return delta;
    }
    if prev.starts_with(current) {
        return String::new();
    }

    let max_len = prev.len().min(current.len());
    let mut max_overlap = 0usize;
    for len in (1..=max_len).rev() {
        if prev.ends_with(&current[..len]) {
            max_overlap = len;
            break;
        }
    }
    *previous = chunk.to_string();
    if max_overlap > 0 {
        String::from_utf8_lossy(&current[max_overlap..]).into_owned()
    } else {
        chunk.to_string()
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

inventory::submit! { ChannelRegistration::new(KiroChannel::ID, kiro_routing_table) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_table_exposes_stream_model_list_and_local_count() {
        let table = KiroChannel.routing_table();

        assert!(matches!(
            table.resolve(&RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse
            )),
            Some(RouteImplementation::Passthrough)
        ));
        assert!(matches!(
            table.resolve(&RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::Claude
            )),
            Some(RouteImplementation::TransformTo { destination })
                if *destination == RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse)
        ));
        assert!(matches!(
            table.resolve(&RouteKey::new(
                OperationFamily::CountToken,
                ProtocolKind::OpenAi
            )),
            Some(RouteImplementation::Local)
        ));
        assert!(matches!(
            table.resolve(&RouteKey::new(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse
            )),
            Some(RouteImplementation::Passthrough)
        ));
        assert!(matches!(
            table.resolve(&RouteKey::new(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude
            )),
            Some(RouteImplementation::TransformTo { destination })
                if *destination == RouteKey::new(OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse)
        ));
        assert!(matches!(
            table.resolve(&RouteKey::new(
                OperationFamily::ModelList,
                ProtocolKind::Gemini
            )),
            Some(RouteImplementation::TransformTo { destination })
                if *destination == RouteKey::new(OperationFamily::ModelList, ProtocolKind::OpenAi)
        ));
    }

    #[test]
    fn finalize_request_converts_openai_response_body_to_kiro_conversation() {
        let request = PreparedRequest {
            method: http::Method::POST,
            route: RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            model: Some("CLAUDE_SONNET_4_20250514_V1_0".into()),
            query: None,
            body: serde_json::to_vec(&json!({
                "model": "ignored",
                "instructions": "be brief",
                "input": [
                    { "role": "user", "content": [{ "type": "input_text", "text": "hello" }] },
                    { "role": "assistant", "content": [{ "type": "output_text", "text": "hi" }] },
                    { "role": "user", "content": [{ "type": "input_text", "text": "again" }] }
                ]
            }))
            .unwrap(),
            headers: http::HeaderMap::new(),
        };

        let finalized = KiroChannel
            .finalize_request(&KiroSettings::default(), request)
            .unwrap();
        let body: Value = serde_json::from_slice(&finalized.body).unwrap();

        assert_eq!(
            body.pointer("/conversationState/history/0/userInputMessage/content")
                .and_then(Value::as_str),
            Some("be brief")
        );
        assert_eq!(
            body.pointer("/conversationState/history/1/assistantResponseMessage/content")
                .and_then(Value::as_str),
            Some("I will follow these instructions.")
        );
        assert_eq!(
            body.pointer("/conversationState/history/2/userInputMessage/content")
                .and_then(Value::as_str),
            Some("hello")
        );
        assert_eq!(
            body.pointer("/conversationState/history/3/assistantResponseMessage/content")
                .and_then(Value::as_str),
            Some("hi")
        );
        assert_eq!(
            body.pointer("/conversationState/currentMessage/userInputMessage/content")
                .and_then(Value::as_str),
            Some("again")
        );
        assert_eq!(
            body.pointer("/conversationState/currentMessage/userInputMessage/modelId")
                .and_then(Value::as_str),
            Some("claude-sonnet-4")
        );
    }

    #[test]
    fn prepare_request_targets_generate_assistant_response_and_injects_profile() {
        let request = PreparedRequest {
            method: http::Method::POST,
            route: RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            model: None,
            query: None,
            body:
                br#"{"conversationState":{"currentMessage":{"userInputMessage":{"content":"hi"}}}}"#
                    .to_vec(),
            headers: http::HeaderMap::new(),
        };

        let upstream = KiroChannel
            .prepare_request(
                &KiroCredential {
                    access_token: "kiro-token".into(),
                    profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/x".into()),
                    ..KiroCredential::default()
                },
                &KiroSettings {
                    agent_mode: Some("q-developer-converse".into()),
                    ..KiroSettings::default()
                },
                &request,
            )
            .unwrap();
        let body: Value = serde_json::from_slice(upstream.body()).unwrap();

        assert_eq!(
            upstream.uri().to_string(),
            "https://q.us-east-1.amazonaws.com/generateAssistantResponse"
        );
        assert_eq!(
            upstream.headers().get(http::header::AUTHORIZATION).unwrap(),
            "Bearer kiro-token"
        );
        assert_eq!(
            upstream.headers().get("x-amzn-kiro-agent-mode").unwrap(),
            "q-developer-converse"
        );
        assert_eq!(upstream.headers().get("Accept").unwrap(), "*/*");
        assert_eq!(
            upstream
                .headers()
                .get("x-amzn-codewhisperer-optout")
                .unwrap(),
            "true"
        );
        assert_eq!(
            upstream.headers().get("Amz-Sdk-Request").unwrap(),
            "attempt=1; max=3"
        );
        assert_eq!(
            body.get("profileArn").and_then(Value::as_str),
            Some("arn:aws:codewhisperer:us-east-1:1:profile/x")
        );
    }

    #[test]
    fn finalize_request_rejects_structured_output_but_allows_plain_text_config() {
        let plain_text = openai_response_body_to_kiro_request(
            serde_json::to_vec(&json!({
                "model": "ignored",
                "text": { "format": { "type": "text" } },
                "input": "hello"
            }))
            .unwrap()
            .as_slice(),
            None,
            &KiroSettings::default(),
        );
        assert!(plain_text.is_ok());

        let structured = openai_response_body_to_kiro_request(
            serde_json::to_vec(&json!({
                "model": "ignored",
                "text": { "format": { "type": "json_schema", "name": "answer", "schema": {} } },
                "input": "hello"
            }))
            .unwrap()
            .as_slice(),
            None,
            &KiroSettings::default(),
        )
        .unwrap_err();

        assert!(
            structured
                .to_string()
                .contains("structured-output conversion")
        );
    }

    #[test]
    fn finalize_request_converts_openai_function_tools_to_kiro_tools() {
        let request = PreparedRequest {
            method: http::Method::POST,
            route: RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            model: Some("claude-sonnet-4.5".into()),
            query: None,
            body: serde_json::to_vec(&json!({
                "model": "claude-sonnet-4.5",
                "input": "run tool",
                "tools": [{
                    "type": "function",
                    "name": "mcp__ida-pro__status",
                    "description": "",
                    "parameters": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [],
                        "properties": {
                            "server": {
                                "type": "string",
                                "additionalProperties": false
                            }
                        }
                    }
                }]
            }))
            .unwrap(),
            headers: http::HeaderMap::new(),
        };

        let finalized = KiroChannel
            .finalize_request(&KiroSettings::default(), request)
            .unwrap();
        let body: Value = serde_json::from_slice(&finalized.body).unwrap();

        assert_eq!(
            body.pointer("/conversationState/currentMessage/userInputMessage/userInputMessageContext/tools/0/toolSpecification/name")
                .and_then(Value::as_str),
            Some("mcpIdaProStatus")
        );
        assert_eq!(
            body.pointer("/conversationState/currentMessage/userInputMessage/userInputMessageContext/tools/0/toolSpecification/description")
                .and_then(Value::as_str),
            Some("Tool: mcpIdaProStatus")
        );
        let schema = body
            .pointer("/conversationState/currentMessage/userInputMessage/userInputMessageContext/tools/0/toolSpecification/inputSchema/json")
            .unwrap();
        assert!(schema.get("additionalProperties").is_none());
        assert!(schema.get("required").is_none());
        assert!(
            schema
                .pointer("/properties/server/additionalProperties")
                .is_none()
        );
    }

    #[test]
    fn normalize_response_decodes_kiro_eventstream_to_openai_response() {
        let mut body = Vec::new();
        body.extend(eventstream_message(
            "messageMetadataEvent",
            &json!({ "conversationId": "conv-1" }),
        ));
        body.extend(eventstream_message(
            "assistantResponseEvent",
            &json!({ "content": "hello%20world", "modelId": "CLAUDE_SONNET_4_20250514_V1_0" }),
        ));
        body.extend(eventstream_message(
            "metadataEvent",
            &json!({
                "tokenUsage": {
                    "uncachedInputTokens": 3,
                    "cacheReadInputTokens": 2,
                    "outputTokens": 4,
                    "totalTokens": 9
                }
            }),
        ));
        let request = PreparedRequest {
            method: http::Method::POST,
            route: RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            model: None,
            query: None,
            body: br#"{"conversationState":{"conversationId":"fallback"}}"#.to_vec(),
            headers: http::HeaderMap::new(),
        };

        let normalized = KiroChannel.normalize_response(&request, body);
        let json: Value = serde_json::from_slice(&normalized).unwrap();

        assert_eq!(json.get("id").and_then(Value::as_str), Some("conv-1"));
        assert_eq!(
            json.get("output_text").and_then(Value::as_str),
            Some("hello world")
        );
        assert_eq!(
            json.pointer("/usage/input_tokens").and_then(Value::as_u64),
            Some(5)
        );
        assert_eq!(
            json.pointer("/usage/output_tokens").and_then(Value::as_u64),
            Some(4)
        );
    }

    #[test]
    fn normalize_response_decodes_kiro_stream_chunks_to_openai_sse() {
        let mut frame = eventstream_message(
            "assistantResponseEvent",
            &json!({ "content": "hello", "modelId": "claude-sonnet-4.5" }),
        );
        frame.extend(eventstream_message(
            "assistantResponseEvent",
            &json!({ "content": "hello world" }),
        ));
        frame.extend(eventstream_message(
            "reasoningContentEvent",
            &json!({ "text": "thinking" }),
        ));
        frame.extend(eventstream_message(
            "meteringEvent",
            &json!({
                "usage": {
                    "inputTokens": 6,
                    "outputTokens": 3,
                    "totalTokens": 9
                }
            }),
        ));
        let mut headers = http::HeaderMap::new();
        headers.insert(
            KIRO_STREAM_ID_HEADER,
            http::HeaderValue::from_static("test-stream"),
        );
        let request = PreparedRequest {
            method: http::Method::POST,
            route: RouteKey::new(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            model: Some("claude-sonnet-4.5".into()),
            query: None,
            body: br#"{"conversationState":{"conversationId":"conv-stream"}}"#.to_vec(),
            headers,
        };

        let split_at = 10;
        let first = KiroChannel.normalize_response(&request, frame[..split_at].to_vec());
        let second = KiroChannel.normalize_response(&request, frame[split_at..].to_vec());
        let already_normalized = br#"{"type":"response.output_text.delta","sequence_number":99}"#;
        assert_eq!(
            KiroChannel.normalize_response(&request, already_normalized.to_vec()),
            already_normalized
        );
        let tail = KiroChannel.normalize_response(&request, Vec::new());
        let stream = String::from_utf8([first, second, tail].concat()).unwrap();

        assert!(stream.contains(r#""type":"response.created""#));
        assert!(stream.contains(r#""type":"response.output_text.delta""#));
        assert!(stream.contains(r#""delta":"hello""#));
        assert!(stream.contains(r#""delta":" world""#));
        assert!(stream.contains(r#""type":"response.reasoning_text.delta""#));
        assert!(stream.contains(r#""type":"response.completed""#));
        assert!(stream.contains(r#""input_tokens":6"#));
        assert!(stream.contains(r#""output_tokens":3"#));
    }

    #[test]
    fn normalize_response_parses_kiro_model_list_as_openai_list() {
        let request = PreparedRequest {
            method: http::Method::GET,
            route: RouteKey::new(OperationFamily::ModelList, ProtocolKind::OpenAi),
            model: None,
            query: None,
            body: Vec::new(),
            headers: http::HeaderMap::new(),
        };
        let body = br#"{"models":[{"modelId":"claude-sonnet-4.5"},{"id":"claude-haiku-4.5"}]}"#;

        let normalized = KiroChannel.normalize_response(&request, body.to_vec());
        let json: Value = serde_json::from_slice(&normalized).unwrap();

        assert_eq!(json.get("object").and_then(Value::as_str), Some("list"));
        assert_eq!(
            json.pointer("/data/0/id").and_then(Value::as_str),
            Some("claude-sonnet-4.5")
        );
        assert_eq!(
            json.pointer("/data/1/id").and_then(Value::as_str),
            Some("claude-haiku-4.5")
        );
    }

    #[test]
    fn prepare_request_targets_kiro_model_list_rest_api() {
        let request = PreparedRequest {
            method: http::Method::GET,
            route: RouteKey::new(OperationFamily::ModelList, ProtocolKind::OpenAi),
            model: None,
            query: None,
            body: Vec::new(),
            headers: http::HeaderMap::new(),
        };

        let upstream = KiroChannel
            .prepare_request(
                &KiroCredential {
                    access_token: "kiro-token".into(),
                    profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/x".into()),
                    ..KiroCredential::default()
                },
                &KiroSettings::default(),
                &request,
            )
            .unwrap();

        assert_eq!(upstream.method(), http::Method::GET);
        assert_eq!(
            upstream.uri().to_string(),
            "https://codewhisperer.us-east-1.amazonaws.com/ListAvailableModels?origin=AI_EDITOR&maxResults=50&profileArn=arn%3Aaws%3Acodewhisperer%3Aus-east-1%3A1%3Aprofile%2Fx"
        );
    }

    #[test]
    fn prepare_quota_request_targets_get_usage_limits() {
        let request = KiroChannel
            .prepare_quota_request(
                &KiroCredential {
                    access_token: "kiro-token".into(),
                    profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/x".into()),
                    ..KiroCredential::default()
                },
                &KiroSettings::default(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(request.method(), http::Method::GET);
        assert_eq!(
            request.uri().to_string(),
            "https://codewhisperer.us-east-1.amazonaws.com/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST&isEmailRequired=true&profileArn=arn%3Aaws%3Acodewhisperer%3Aus-east-1%3A1%3Aprofile%2Fx"
        );
    }

    #[test]
    fn oauth_start_url_matches_kiro_portal_shape() {
        let authorize_url = build_kiro_portal_authorize_url(
            "https://app.kiro.dev",
            "state-1",
            "challenge-1",
            "http://localhost:3128",
            false,
        )
        .unwrap();
        let parsed = url::Url::parse(&authorize_url).unwrap();
        let params = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(
            parsed.as_str().split('?').next(),
            Some("https://app.kiro.dev/signin")
        );
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
        assert_eq!(
            params.get("code_challenge").map(String::as_str),
            Some("challenge-1")
        );
        assert_eq!(
            params.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(
            params.get("redirect_uri").map(String::as_str),
            Some("http://localhost:3128")
        );
        assert_eq!(
            params.get("redirect_from").map(String::as_str),
            Some("KiroIDE")
        );
    }

    #[test]
    fn oauth_social_exchange_redirect_uri_uses_callback_path_and_login_option() {
        let redirect_uri = build_kiro_social_exchange_redirect_uri(
            "http://localhost:3128",
            Some("http://localhost:3128/oauth/callback?login_option=github&code=abc&state=xyz"),
            "github",
        );

        assert_eq!(
            redirect_uri,
            "http://localhost:3128/oauth/callback?login_option=github"
        );
    }

    #[test]
    fn oauth_token_response_parses_kiro_auth_camel_case_fields() {
        let token: KiroAuthTokenResponse = serde_json::from_value(json!({
            "accessToken": "access-1",
            "refreshToken": "refresh-1",
            "profileArn": "arn:profile",
            "expiresIn": 3600
        }))
        .unwrap();
        let credential = kiro_token_response_to_credential(token, "Google").unwrap();

        assert_eq!(credential.access_token, "access-1");
        assert_eq!(credential.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(credential.profile_arn.as_deref(), Some("arn:profile"));
        assert_eq!(credential.auth_method.as_deref(), Some("social"));
        assert_eq!(credential.provider.as_deref(), Some("Google"));
        assert!(credential.expires_at_ms.unwrap_or(0) > kiro_current_unix_ms());
    }

    #[test]
    fn credential_accepts_kiro_go_oidc_refresh_fields() {
        let credential: KiroCredential = serde_json::from_value(json!({
            "access_token": "access-1",
            "refreshToken": "refresh-1",
            "clientId": "client-1",
            "clientSecret": "secret-1",
            "region": "eu-central-1",
            "authMethod": "IdC",
            "provider": "Enterprise"
        }))
        .unwrap();

        assert_eq!(credential.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(credential.client_id.as_deref(), Some("client-1"));
        assert_eq!(credential.client_secret.as_deref(), Some("secret-1"));
        assert_eq!(credential.region.as_deref(), Some("eu-central-1"));
        assert_eq!(
            kiro_oidc_base_url("eu-central-1"),
            "https://oidc.eu-central-1.amazonaws.com"
        );
    }

    #[test]
    fn oauth_direct_idc_authorize_url_matches_kiro_go_shape() {
        let scopes = kiro_scopes_for_settings(&KiroSettings::default());
        let authorize_url = build_kiro_oidc_authorize_url(
            "us-east-1",
            "client-1",
            "http://127.0.0.1/oauth/callback",
            &scopes,
            "state-1",
            "challenge-1",
        )
        .unwrap();
        let parsed = url::Url::parse(&authorize_url).unwrap();
        let params = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(
            parsed.as_str().split('?').next(),
            Some("https://oidc.us-east-1.amazonaws.com/authorize")
        );
        assert_eq!(
            params.get("response_type").map(String::as_str),
            Some("code")
        );
        assert_eq!(
            params.get("client_id").map(String::as_str),
            Some("client-1")
        );
        assert_eq!(
            params.get("redirect_uri").map(String::as_str),
            Some("http://127.0.0.1/oauth/callback")
        );
        assert_eq!(
            params.get("scopes").map(String::as_str),
            Some(
                "codewhisperer:completions,codewhisperer:analysis,codewhisperer:conversations,codewhisperer:transformations,codewhisperer:taskassist"
            )
        );
        assert_eq!(params.get("state").map(String::as_str), Some("state-1"));
        assert_eq!(
            params.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
    }

    #[test]
    fn oauth_direct_idc_token_response_persists_refresh_inputs() {
        let token: KiroAuthTokenResponse = serde_json::from_value(json!({
            "accessToken": "access-1",
            "refreshToken": "refresh-1",
            "expiresIn": 3600
        }))
        .unwrap();
        let credential = kiro_oidc_token_response_to_credential(
            token,
            "client-1".to_string(),
            "secret-1".to_string(),
            "us-east-1".to_string(),
            "Enterprise".to_string(),
        )
        .unwrap();

        assert_eq!(credential.access_token, "access-1");
        assert_eq!(credential.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(credential.client_id.as_deref(), Some("client-1"));
        assert_eq!(credential.client_secret.as_deref(), Some("secret-1"));
        assert_eq!(credential.region.as_deref(), Some("us-east-1"));
        assert_eq!(credential.auth_method.as_deref(), Some("IdC"));
        assert_eq!(credential.provider.as_deref(), Some("Enterprise"));
    }

    #[test]
    fn local_count_tokens_uses_shared_local_counter() {
        let body = br#"{"model":"gpt-4o-mini","input":"hello"}"#;
        let response = KiroChannel
            .handle_local(
                OperationFamily::CountToken,
                ProtocolKind::OpenAi,
                Some("gpt-4o-mini"),
                None,
                body,
            )
            .unwrap()
            .unwrap();
        let json: Value = serde_json::from_slice(&response).unwrap();

        assert!(
            json.get("input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
        );
    }

    fn eventstream_message(event_type: &str, payload: &Value) -> Vec<u8> {
        let payload = serde_json::to_vec(payload).unwrap();
        let mut headers = Vec::new();
        push_string_header(&mut headers, ":message-type", "event");
        push_string_header(&mut headers, ":event-type", event_type);
        push_string_header(&mut headers, ":content-type", "application/json");
        let total_len = 12 + headers.len() + payload.len() + 4;
        let mut out = Vec::new();
        out.extend((total_len as u32).to_be_bytes());
        out.extend((headers.len() as u32).to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.extend(headers);
        out.extend(payload);
        out.extend(0u32.to_be_bytes());
        out
    }

    fn push_string_header(headers: &mut Vec<u8>, name: &str, value: &str) {
        headers.push(name.len() as u8);
        headers.extend(name.as_bytes());
        headers.push(7);
        headers.extend((value.len() as u16).to_be_bytes());
        headers.extend(value.as_bytes());
    }
}
