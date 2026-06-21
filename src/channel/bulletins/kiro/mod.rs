//! Kiro channel (Amazon Q / Kiro IDE) — DUAL OAuth + AWS Smithy event-stream.
//!
//! Kiro exposes no OpenAI/Claude/Gemini-compatible surface: chat goes through
//! the Smithy REST-JSON `POST /generateAssistantResponse`, whose RESPONSE is an
//! AWS binary event-stream. The upstream speaks the OpenAI Responses format, so the M2 layer
//! sees Responses on both sides — but the channel must SHAPE both directions:
//!
//!   * **request** ([`prepare`](KiroChannel::prepare)) — convert the inbound
//!     OpenAI Responses body into Kiro's `conversationState` JSON
//!     ([`request::build_request_body`]), lift `profileArn` to the top level, and
//!     inject the Kiro auth + IDE fingerprint headers.
//!   * **response** ([`stream_decoder`](KiroChannel::stream_decoder)) — decode the
//!     Smithy event-stream into Responses SSE ([`response::KiroStreamDecoder`]).
//!
//! Auth is a dual `refresh_token` grant (social vs AWS IdC) — see [`auth`]. This
//! is the heaviest channel; the binary frame parser lives in [`smithy`] and is
//! the most-tested piece. All decode is synchronous, so the channel compiles on
//! the wasm edge target (refresh is async via [`UpstreamClient`], fine on all).

mod auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod model_list;
mod request;
mod request_tools;
mod response;
mod smithy;
mod sse;
mod tool_calls;
mod usage;
use std::sync::Arc;

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, ChannelStreamDecoder, DeviceInit,
    DevicePoll, PrepareCtx, PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{Operation, Provider};

use response::KiroStreamDecoder;

/// Kiro.dev region (the management/runtime hosts are region-scoped). Override
/// via `settings.region`; default us-east-1.
const DEFAULT_REGION: &str = "us-east-1";
/// AWS-JSON 1.0 content type used by both kiro.dev services.
pub(super) const AMZ_JSON: &str = "application/x-amz-json-1.0";
/// Smithy `x-amz-target` for the runtime (chat) service.
const TARGET_GENERATE: &str = "AmazonCodeWhispererStreamingService.GenerateAssistantResponse";
/// Streaming User-Agent the Kiro CLI sends to the runtime host (chat).
const USER_AGENT_VALUE: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererstreaming/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";
/// Smithy `x-amz-target`s on the management host (model-list / usage).
pub(super) const TARGET_LIST_MODELS: &str = "AmazonCodeWhispererService.ListAvailableModels";
pub(super) const TARGET_USAGE: &str = "AmazonCodeWhispererService.GetUsageLimits";
/// Runtime User-Agent the Kiro CLI sends to the management host (model-list/usage).
pub(super) const UA_MANAGEMENT: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererruntime/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";
/// Client surface reported to the Kiro/CodeWhisperer backend — sent as the
/// `origin` (chat body, usage, model-list). Captured from the real Kiro CLI
/// (`kiro-cli-chat`): it is `KIRO_CLI`, NOT v1's `AI_EDITOR`. SINGLE source of
/// truth; if a capture shows a different value, change it HERE.
pub(super) const ORIGIN: &str = "KIRO_CLI";

/// The Kiro region from settings (default `us-east-1`).
pub(super) fn region(settings: &Value) -> String {
    settings
        .get("region")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_REGION)
        .to_string()
}

/// The management host (model-list / usage): `https://management.{region}.kiro.dev`,
/// or a `settings.management_url` override.
pub(super) fn management_base(settings: &Value) -> String {
    if let Some(u) = settings
        .get("management_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return u.to_string();
    }
    format!("https://management.{}.kiro.dev", region(settings))
}

/// The runtime (chat) host: `https://runtime.{region}.kiro.dev`, or a
/// `settings.runtime_url` / legacy `settings.base_url` override.
fn runtime_base(settings: &Value) -> String {
    for key in ["runtime_url", "base_url"] {
        if let Some(u) = settings
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return u.to_string();
        }
    }
    format!("https://runtime.{}.kiro.dev", region(settings))
}

pub struct KiroChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for KiroChannel {
    fn id(&self) -> &'static str {
        "kiro"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            // Kiro's upstream ONLY speaks the AWS event-stream — there is no
            // non-stream endpoint. Route every `GenerateContent` to a streaming
            // upstream; the pipeline collapses the stream back to one object for
            // non-stream clients (`TransformPlan::AggregateStream`).
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
        ]
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        // Model-list: Kiro exposes no `/v1/models`; the catalogue lives behind the
        // bespoke `GET {base}/ListAvailableModels`. Detect the family model-list
        // request (GET …/models) and build it directly — the content body shaper
        // must NOT run here. All other (content) requests fall through unchanged.
        if model_list::is_model_list(&ctx.method, ctx.path) {
            let req = model_list::request(ctx.secret, ctx.provider_settings)?;
            return Ok(PreparedRequest::new(req));
        }

        let access_token = auth::access_token(ctx.secret)?.to_string();
        let profile_arn = auth::profile_arn(ctx.secret, ctx.provider_settings).map(str::to_owned);
        let base = runtime_base(ctx.provider_settings);

        // Shape the inbound Responses body into Kiro's conversationState graph,
        // then lift profileArn to the top level (where Kiro expects it). The
        // upstream-mapped model id selects the Kiro model.
        let body = request::build_request_body(&ctx.body, ctx.upstream_model_id, &gen_uuid())?;
        let body = with_profile_arn(body, profile_arn.as_deref())?;

        // Captured Kiro CLI chat: AWS-JSON Smithy POST to the runtime host root
        // (the operation is the `x-amz-target`, not a path), event-stream response.
        let uri = join_url(&base, "/", None)?;
        // Smithy/binary channel: it injects its own auth + IDE fingerprint and
        // forwards no inbound headers beyond the base content-type/accept set.
        let headers = allow_headers(ctx.headers, &[]);
        let mut req = build_request(ctx.method, uri, headers, Bytes::from(body))?;
        apply_headers(&mut req, &access_token, TARGET_GENERATE)?;
        Ok(PreparedRequest::new(req))
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(Box::new(KiroStreamDecoder::new()))
    }

    /// Reproject the bespoke `ListAvailableModels` body into the OpenAI family
    /// canonical model-list shape so `parse_models` reads `data[].id`. Content
    /// responses are the AWS event-stream and go through [`KiroStreamDecoder`],
    /// NOT here — so every non-`ListModels` op is returned unchanged.
    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        match ctx.op.operation {
            Operation::ListModels => model_list::to_openai(body),
            _ => body,
        }
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        // `provider_settings` are not threaded into refresh; the social auth base
        // defaults inside `auth::refresh` when absent (the common case).
        auth::refresh(client, &Value::Null, secret).await
    }

    fn prepare_usage_request(
        &self,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        usage::request(secret, settings)
    }

    fn parse_usage(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        usage::parse(status, body)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for KiroChannel {
    /// Interactive authcode+PKCE login for the AWS SSO-OIDC (builderId / idc) and
    /// external-IdP methods, dispatched on `params.auth_method` (default
    /// `builderId`). Social uses the device flow below.
    async fn authcode_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        params: &Value,
        redirect_uri: &str,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let started =
            auth::authcode_start(client, params, redirect_uri, state, pkce_challenge).await?;
        Ok(Some(started))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        extra: Option<&Value>,
    ) -> Result<Value, ChannelError> {
        auth::authcode_exchange(client, code, verifier, redirect_uri, extra).await
    }

    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        params: &Value,
    ) -> Result<DeviceInit, ChannelError> {
        auth::device_start(client, params).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        device_code: &str,
    ) -> Result<DevicePoll, ChannelError> {
        auth::device_poll(client, device_code).await
    }
}

/// Inject `profileArn` at the top level of the serialized request body when the
/// credential carries one and the body does not already set it.
fn with_profile_arn(body: Vec<u8>, profile_arn: Option<&str>) -> Result<Vec<u8>, ChannelError> {
    let Some(arn) = profile_arn else {
        return Ok(body);
    };
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("kiro request body re-parse: {e}")))?;
    if value.get("profileArn").is_none()
        && let Some(obj) = value.as_object_mut()
    {
        obj.insert("profileArn".into(), Value::String(arn.to_string()));
    }
    serde_json::to_vec(&value)
        .map_err(|e| ChannelError::Build(format!("kiro request body re-serialize: {e}")))
}

/// Inject the Kiro Bearer + IDE fingerprint headers (agent-mode, optout, the AWS
/// SDK retry/invocation ids, UA). `accept: */*` matches the Kiro IDE request —
/// the event-stream body is selected by the endpoint, not Accept.
fn apply_headers(
    req: &mut http::Request<Bytes>,
    access_token: &str,
    target: &'static str,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let invocation_id = HeaderValue::from_str(&gen_uuid())
        .map_err(|e| ChannelError::Build(format!("bad invocation id: {e}")))?;

    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(CONTENT_TYPE, HeaderValue::from_static(AMZ_JSON));
    h.insert(ACCEPT, HeaderValue::from_static("*/*"));
    h.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    h.insert(
        HeaderName::from_static("x-amz-user-agent"),
        HeaderValue::from_static(USER_AGENT_VALUE),
    );
    h.insert(
        HeaderName::from_static("x-amz-target"),
        HeaderValue::from_static(target),
    );
    h.insert(
        HeaderName::from_static("x-amzn-codewhisperer-optout"),
        HeaderValue::from_static("false"),
    );
    h.insert(
        HeaderName::from_static("amz-sdk-request"),
        HeaderValue::from_static("attempt=1; max=3"),
    );
    h.insert(
        HeaderName::from_static("amz-sdk-invocation-id"),
        invocation_id,
    );
    Ok(())
}

/// Fresh v4-shaped UUID string from `crate::util::rand` (opaque to Kiro).
fn gen_uuid() -> String {
    crate::util::rand::uuid_v4()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::http::client::ClientError;
    use http::{HeaderMap, Method, Response};
    use serde_json::json;

    /// Mock client returning a fixed JSON body and recording each request's URI
    /// and (JSON-parsed) request body.
    struct MockClient {
        body: Vec<u8>,
        seen: std::sync::Mutex<Vec<String>>,
        bodies: std::sync::Mutex<Vec<Bytes>>,
    }

    #[async_trait::async_trait]
    impl UpstreamClient for MockClient {
        async fn send(&self, req: http::Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
            self.seen.lock().unwrap().push(req.uri().to_string());
            self.bodies.lock().unwrap().push(req.body().clone());
            Ok(Response::new(Bytes::from(self.body.clone())))
        }
    }

    fn mock(body: Value) -> Arc<MockClient> {
        Arc::new(MockClient {
            body: serde_json::to_vec(&body).unwrap(),
            seen: std::sync::Mutex::new(Vec::new()),
            bodies: std::sync::Mutex::new(Vec::new()),
        })
    }

    #[tokio::test]
    async fn refresh_dispatches_by_secret_shape() {
        // SOCIAL: refreshToken POST → rotates tokens + stores profileArn.
        let client = mock(json!({
            "accessToken": "new-access",
            "refreshToken": "new-refresh",
            "profileArn": "arn:aws:kiro:profile/p1",
            "expiresIn": 3600,
        }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        let secret = json!({ "access_token": "old", "refresh_token": "old-rt" });
        let out = auth::refresh(&dyn_client, &Value::Null, &secret)
            .await
            .unwrap();
        assert_eq!(out["access_token"], "new-access");
        assert_eq!(out["refresh_token"], "new-refresh");
        assert_eq!(out["profile_arn"], "arn:aws:kiro:profile/p1");
        assert!(out["expires_at_ms"].as_i64().unwrap() > crate::util::time::unix_now() * 1000);
        assert!(client.seen.lock().unwrap()[0].contains("/refreshToken"));

        // SSO-OIDC (builderId / idc): region-templated oidc CreateToken POST →
        // rotates access token; the response omits refreshToken so the old one is
        // preserved.
        let client = mock(json!({ "accessToken": "idc-access", "expiresIn": 1800 }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        let secret = json!({
            "access_token": "old",
            "refresh_token": "old-rt",
            "client_id": "cid",
            "client_secret": "csecret",
            "region": "eu-west-1",
        });
        let out = auth::refresh(&dyn_client, &Value::Null, &secret)
            .await
            .unwrap();
        assert_eq!(out["access_token"], "idc-access");
        assert_eq!(out["refresh_token"], "old-rt");
        assert!(
            client.seen.lock().unwrap()[0].contains("oidc.eu-west-1.amazonaws.com/token"),
            "SSO-OIDC refresh must hit the region-templated oidc host"
        );
    }

    #[tokio::test]
    async fn sso_authcode_start_registers_and_builds_authorize_url() {
        // RegisterClient returns the dynamic client creds; authcode_start then
        // builds the /authorize URL and stashes the creds in `extra`.
        let client = mock(json!({ "clientId": "reg-cid", "clientSecret": "reg-secret" }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        // No params / no redirect → builderId defaults + the loopback redirect.
        let started = KiroChannel
            .authcode_start(&dyn_client, &json!({}), "", "st-1", "chal-1")
            .await
            .unwrap()
            .expect("kiro has an authcode login");

        // The first (only) call hit RegisterClient on the Builder ID oidc host.
        assert_eq!(
            client.seen.lock().unwrap()[0],
            "https://oidc.us-east-1.amazonaws.com/client/register"
        );
        // RegisterClient body: public client, both PKCE grants, our redirect.
        let reg: Value = serde_json::from_slice(&client.bodies.lock().unwrap()[0]).unwrap();
        assert_eq!(reg["clientName"], "Kiro-CLI");
        assert_eq!(reg["clientType"], "public");
        assert_eq!(
            reg["grantTypes"],
            json!(["authorization_code", "refresh_token"])
        );
        assert_eq!(
            reg["redirectUris"],
            json!(["http://127.0.0.1:1455/oauth/callback"])
        );
        assert_eq!(reg["issuerUrl"], "https://view.awsapps.com/start");

        // Authorize URL on the same host; PKCE S256; plural `scopes`.
        assert!(
            started
                .authorize_url
                .starts_with("https://oidc.us-east-1.amazonaws.com/authorize?")
        );
        assert!(started.authorize_url.contains("code_challenge_method=S256"));
        assert!(started.authorize_url.contains("client_id=reg-cid"));
        assert_eq!(started.redirect_uri, "http://127.0.0.1:1455/oauth/callback");

        // `extra` carries the registered creds + target for the exchange/refresh.
        let extra = started.extra.unwrap();
        assert_eq!(extra["client_id"], "reg-cid");
        assert_eq!(extra["client_secret"], "reg-secret");
        assert_eq!(extra["region"], "us-east-1");
        assert_eq!(extra["start_url"], "https://view.awsapps.com/start");
    }

    #[tokio::test]
    async fn authcode_start_rejects_social_and_unknown_methods() {
        // social uses the device flow; an explicit/typo authcode auth_method must
        // error WITHOUT triggering a live AWS RegisterClient.
        let client = mock(json!({ "clientId": "x", "clientSecret": "y" }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        for method in ["social", "external_idp", "builder-id", "totally-bogus"] {
            let err = KiroChannel
                .authcode_start(&dyn_client, &json!({ "auth_method": method }), "", "s", "c")
                .await;
            assert!(err.is_err(), "auth_method={method} must be rejected");
        }
        assert!(
            client.seen.lock().unwrap().is_empty(),
            "rejection must happen before any network call"
        );
    }

    #[test]
    fn request_build() {
        // Minimal Responses body → Smithy conversationState with profileArn +
        // currentMessage.userInputMessage.content.
        let secret = json!({
            "access_token": "tok",
            "profile_arn": "arn:aws:kiro:profile/abc",
        });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "claude-sonnet-4-5",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(br#"{"input":"hello kiro"}"#),
        };
        let req = KiroChannel.prepare(ctx).unwrap().request;

        // Captured Kiro CLI chat: Smithy POST to the runtime host root with the
        // operation in `x-amz-target` (no path), AWS-JSON 1.0, no agent-mode.
        assert_eq!(req.uri().to_string(), "https://runtime.us-east-1.kiro.dev/");
        assert_eq!(req.headers().get("authorization").unwrap(), "Bearer tok");
        assert_eq!(
            req.headers().get("x-amz-target").unwrap(),
            "AmazonCodeWhispererStreamingService.GenerateAssistantResponse"
        );
        assert_eq!(
            req.headers().get("content-type").unwrap(),
            "application/x-amz-json-1.0"
        );
        assert!(req.headers().get("x-amzn-kiro-agent-mode").is_none());
        assert!(req.headers().get("amz-sdk-invocation-id").is_some());

        let v: Value = serde_json::from_slice(req.body()).unwrap();
        assert_eq!(v["profileArn"], "arn:aws:kiro:profile/abc");
        let user = &v["conversationState"]["currentMessage"]["userInputMessage"];
        assert_eq!(user["content"], "hello kiro");
        // model is dot-versioned via map_model.
        assert_eq!(user["modelId"], "claude-sonnet-4.5");
    }

    #[tokio::test]
    async fn kiro_device_start_posts_authorization() {
        // Captured kiro-cli-chat: device login POSTs to the auth host's
        // /oauth/device/authorization and maps the response to a DeviceInit
        // (verification URL prefers the `complete` form, interval is ms/1000).
        let client = mock(json!({
            "deviceCode": "dev-code-1",
            "userCode": "WXYZ-1234",
            "verificationUriComplete": "https://app.kiro.dev/device?user_code=WXYZ-1234",
            "verificationUri": "https://app.kiro.dev/device",
            "intervalInMilliseconds": 5000,
            "expiresInMilliseconds": 900000,
        }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        let init = KiroChannel
            .device_start(&dyn_client, &json!({}))
            .await
            .expect("device_start ok");
        assert_eq!(init.device_code, "dev-code-1");
        assert_eq!(init.user_code, "WXYZ-1234");
        assert_eq!(
            init.verification_url,
            "https://app.kiro.dev/device?user_code=WXYZ-1234"
        );
        assert_eq!(init.interval_secs, 5);

        // The request hit the device-authorization endpoint on the auth host.
        assert_eq!(
            client.seen.lock().unwrap()[0],
            "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/device/authorization"
        );
    }

    #[tokio::test]
    async fn kiro_device_poll_pending_then_authorized() {
        // status authorization_pending → Pending; the request hits the poll
        // endpoint.
        let client = mock(json!({ "status": "authorization_pending" }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        let poll = KiroChannel
            .device_poll(&dyn_client, "dev-code-1")
            .await
            .expect("device_poll ok");
        assert!(matches!(poll, DevicePoll::Pending));
        assert_eq!(
            client.seen.lock().unwrap()[0],
            "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/device/poll"
        );

        // status authorized → Ready with the mapped secret.
        let client = mock(json!({
            "status": "authorized",
            "accessToken": "at-9",
            "refreshToken": "rt-9",
            "profileArn": "arn:aws:kiro:profile/p9",
            "identityProvider": "Github",
        }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        let poll = KiroChannel
            .device_poll(&dyn_client, "dev-code-1")
            .await
            .expect("device_poll ok");
        let secret = match poll {
            DevicePoll::Ready(v) => v,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert_eq!(secret["access_token"], "at-9");
        assert_eq!(secret["refresh_token"], "rt-9");
        assert_eq!(secret["profile_arn"], "arn:aws:kiro:profile/p9");
        assert_eq!(secret["provider"], "Github");
    }

    #[tokio::test]
    async fn kiro_device_poll_denied() {
        // Any non-pending/non-authorized status (expired/denied) → Denied.
        let client = mock(json!({ "status": "expired" }));
        let dyn_client: Arc<dyn UpstreamClient> = client.clone();
        let poll = KiroChannel
            .device_poll(&dyn_client, "dev-code-1")
            .await
            .expect("device_poll ok");
        assert!(matches!(poll, DevicePoll::Denied));
    }
}
