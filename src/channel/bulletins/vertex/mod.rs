//! Vertex AI channel (Google Cloud): service-account JWT → bearer token.
//!
//! Auth is a two-step OAuth2 JWT-bearer grant: sign an RS256 assertion from the
//! SA key ([`auth`]), exchange it at the token endpoint for a short-lived bearer
//! (no `refresh_token` — every refresh re-signs from the key). The request is
//! plain Gemini `generateContent` against the regional Vertex host; NO envelope,
//! NO TLS impersonation, NO body mutation.

mod auth;
mod model_list;

use std::sync::Arc;

use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use serde_json::Value;

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::shaping::vertex_normalize;
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::http::client::UpstreamClient;
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

/// Whether this op is a Gemini content-generation call (the only response shape
/// Vertex normalizes; model lists / embeddings / errors pass through untouched).
fn is_gemini_content(ctx: &ShapeCtx) -> bool {
    matches!(
        ctx.op.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
    )
}

use auth::DEFAULT_LOCATION;

/// Refresh slightly before the token actually expires to avoid racing a 401.
const EXPIRY_SKEW_MS: i64 = 60_000;

pub struct VertexChannel;

impl VertexChannel {
    /// Build the regional host: `global` uses the bare host, every other region
    /// is `{location}-aiplatform.googleapis.com`.
    fn host(location: &str) -> String {
        if location == "global" {
            "https://aiplatform.googleapis.com".to_string()
        } else {
            format!("https://{location}-aiplatform.googleapis.com")
        }
    }

    /// Resolve the effective location: provider settings override the SA secret,
    /// which overrides the default region.
    fn location(provider_settings: &Value, secret: &Value) -> String {
        provider_settings
            .get("location")
            .and_then(Value::as_str)
            .or_else(|| secret.get("location").and_then(Value::as_str))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_LOCATION)
            .to_string()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for VertexChannel {
    fn id(&self) -> &'static str {
        "vertex"
    }

    fn provider_family(&self) -> Provider {
        Provider::Gemini
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::Gemini)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::Gemini)),
            xform(ListModels, pv(P::OpenAi), ListModels, pv(P::Gemini)),
            pass(GetModel, pv(P::Gemini)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::Gemini)),
            xform(GetModel, pv(P::OpenAi), GetModel, pv(P::Gemini)),
            pass(CountTokens, pv(P::Gemini)),
            xform(CountTokens, pv(P::Claude), CountTokens, pv(P::Gemini)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(GeminiGenerateContent)),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(StreamGenerateContent, cg(GeminiGenerateContent)),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                CreateImage,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                EditImage,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(CreateEmbedding, pv(P::Gemini)),
            xform(
                CreateEmbedding,
                pv(P::OpenAi),
                CreateEmbedding,
                pv(P::Gemini),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let access_token = ctx
            .secret
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ChannelError::InvalidCredential("missing access_token".into()))?;
        let project_id = ctx
            .secret
            .get("project_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ChannelError::InvalidCredential("missing project_id".into()))?;

        let location = Self::location(ctx.provider_settings, ctx.secret);
        let host = Self::host(&location);
        // The model-pull (and any ListModels client) hits `/v1beta/models` —
        // GET, no model id, no `:verb`. Vertex's `ListPublisherModels` (Model
        // Garden) is NOT project-scoped: it is `GET /v1beta1/publishers/google/models`
        // on the regional host. The `projects/{project}/locations/{location}/`
        // prefix (used by the content path below) 404s on the LIST endpoint.
        let is_list_models = ctx
            .path
            .rsplit('/')
            .next()
            .is_some_and(|seg| seg == "models" && !ctx.path.contains(':'));
        let (path, query) = if is_list_models {
            ("/v1beta1/publishers/google/models".to_string(), None)
        } else {
            // The M2 layer encodes the verb in the path for gemini targets;
            // vertex rebuilds its own URL against the regional host, reusing only
            // the stream flag (`:streamGenerateContent` → SSE, else
            // `:generateContent`).
            let (verb, query) = if ctx.path.contains(":streamGenerateContent") {
                (":streamGenerateContent", Some("alt=sse"))
            } else if ctx.path.contains(":countTokens") {
                (":countTokens", None)
            } else {
                (":generateContent", None)
            };
            (
                format!(
                    "/v1beta1/projects/{project_id}/locations/{location}/publishers/google/models/{}{verb}",
                    ctx.upstream_model_id,
                ),
                query,
            )
        };

        let uri = join_url(&host, &path, query)?;
        // Vertex needs no inbound forwarded headers; it injects its own auth.
        let headers = allow_headers(ctx.headers, &[]);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
            .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
        req.headers_mut().insert(AUTHORIZATION, bearer);
        req.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(PreparedRequest::new(req))
    }

    /// Normalize Gemini content responses to AI-Studio shape (citation rename,
    /// block-reason fix), and reshape Vertex's `publisherModels` list response
    /// into the canonical Gemini `models` shape. Other ops/kinds pass through.
    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if ctx.op.operation == Operation::ListModels {
            model_list::normalize_vertex_model_list(body)
        } else if is_gemini_content(ctx) {
            vertex_normalize::normalize_vertex_response(body)
        } else {
            body
        }
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        let token = secret
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(_) = token else {
            return true;
        };
        let expires_at_ms = secret
            .get("expires_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let now_ms = crate::util::time::unix_now().saturating_mul(1000);
        now_ms > expires_at_ms - EXPIRY_SKEW_MS
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        // jsonwebtoken v10 `rust_crypto` backend signs on all targets (incl
        // edge), so refresh works everywhere — no native gate.
        exchange_token(client, secret).await
    }
}

/// Sign the SA assertion, exchange it at the token endpoint, and return the
/// secret with `access_token` + `expires_at_ms` rotated (all other fields kept).
async fn exchange_token(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    use crate::channel::oauth::token_post;
    use auth::ServiceAccount;

    let sa = ServiceAccount::parse(secret)?;
    let jwt = auth::sign_jwt(&sa)?;
    let resp = token_post(
        client,
        &sa.token_uri,
        &[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ],
        &[],
    )
    .await?;
    let access_token = resp
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| ChannelError::Build("token response missing access_token".into()))?;
    let expires_in = resp.expires_in.unwrap_or(3600);
    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    let expires_at_ms = now_ms.saturating_add(expires_in.saturating_mul(1000) as i64);

    // Preserve every other secret field; only the token + expiry rotate.
    let mut out = secret.clone();
    let obj = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(access_token));
    obj.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    Ok(out)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, Method};
    use serde_json::json;

    // Throwaway 2048-bit RSA key (PKCS#1) generated solely for these tests — it
    // signs nothing real; it only needs to be a valid PEM `from_rsa_pem` accepts.
    const TEST_PEM: &str = "-----BEGIN RSA PRIVATE KEY-----\n\
MIIEowIBAAKCAQEAq62FE0f0CqPnZCu8hZ4G+MyQnJhkg7MWwl+JqH9wjoVWBX2A\n\
f7IobJNMJYNAQs2jeby6qz7CorDTOKO0Pi8RYahcIM7TNlmJr3X1rRzgiyW69/M+\n\
cGoJ6EGUrGHop9WTwKHkC9sWj5WsWrT2wk20w/1jpDK7sSw4wbN0ilGFjc67C4Bu\n\
PMl69BLDUHh+8uxjoeYK3PHM/xYe/e4BOZ2/xI9CiilWfO3FGpEU8MRYvkaXaeRY\n\
02ghFRA14KxqSSKEEthxBYy2hWM+FiNvLJwQ+3Ybs34MPNu1CtzCoLnaNcCnKNtF\n\
VUeRauUTJ4A2I3Kb7/H0WX5ZfYCJSZzGmCSccwIDAQABAoIBAClqSMon94WBmNaf\n\
fnE1eDUZFGHSmZzz3S+y4ICXjc2z+NaGOjOUBRB8UEhUa3IyLZe2ocmh8E5THgFx\n\
7I97x1Opy9/WRTm9S+vaJxRF/R1UUtByC8QOsKko+PbE/91NNsGnzF3X0o986gFP\n\
2p9xI4SMYjdATesl4eNIqXqcw/07Vrqr/ryeDGs50Y2agTVFNr2D38ZQPt6E9Qws\n\
sx4T+yA9M3ZtsvW0OvXX08BMLEJyU26wSU4cyoFYmtBU0TE81rst60MVjW1RMLHM\n\
Rcj4E8EcQlcIRZE8ELXCX9l35olZgnzVrH1I6LgycIrzwYEfYy3svzh8p/Be+kRM\n\
2ZEDVyECgYEA3DzbuCDVgTFTtKegd/dGR9PDgSNJEK/qElVufx4nVgYTNPFnLo5s\n\
joTCEWvFd3L01DBfm7kMOu8YtwIQxw5YCM/pD0WJihYs9f+wyldEtYJbcoh5bmKX\n\
ZaVbokpy/+RxcQkaYmwIRRBHgQ7ZrapxA4vkcfsxmY79P6v2pPbblmkCgYEAx44O\n\
jb+Dx1mGZ29cPYWyi0shH+Oxp5IdII5uvpbMLiclbeHMl/3u0wBfng6AcRJYb48V\n\
RWc0vG9MwvWKziuzANCr+37n/qDpj/cmD6ZixwDDS9CXJaYqgCNbiL9WGxlQ1VIs\n\
ckth2j7HRSDGMqOstd82ZHUZvLWATEArRDgNmHsCgYEAnabO3ZpbWzS1J6+KlfWj\n\
EI2M+GcKyXAzjVYsV8B9Bf4pR4+6fcAkA00TIqdT3jKjATVzayRmldVLis1mtycU\n\
a5Jw0abEUt2W561VnzIjFA8xaOY6joLyvydEVgMXGQgtEG4kvel5bf6+QKshtUg5\n\
yAEe0Vyv361UqXxufR3ciGkCgYBd7f/ruLnOm9Un2sMQMl5YMoTk/cghmCUdre1y\n\
yIhTMRntHtuur1g6+XIIc8sBbiEyYachg/LOv5TiL7GmWetn9tD9ED8jG5rUqQDB\n\
XRAhm7pRdV2v2wcmSX5MX8On/cKOpp9FLTZiBCrH3yVrsJ8a/HYd0wDKUqSRP6Md\n\
+URtAQKBgAzMJVmDox7PUbdeVC3c631DTNb49BiyPpRtmnDhwm1VlAEGiVygM58E\n\
nUFcQyULVDXehYeiq7hDInaK5gUtnhaN8Y7zzBDf3gEUmqM5GwBeFe7mIn9gfqsb\n\
yR/PS6gbNUvYTwD+RYNaQFOsbyQkoNy1azBQm6X1m3J2+c+wnrYp\n\
-----END RSA PRIVATE KEY-----";

    fn sa_secret() -> Value {
        json!({
            "client_email": "[email protected]",
            "private_key": TEST_PEM,
            "project_id": "proj-123",
        })
    }

    #[test]
    fn signs_and_builds_url() {
        // Sign produces a compact JWS: header.payload.signature, header alg RS256.
        let sa = auth::ServiceAccount::parse(&sa_secret()).unwrap();
        let jwt = auth::sign_jwt(&sa).unwrap();
        let segments: Vec<&str> = jwt.split('.').collect();
        assert_eq!(segments.len(), 3, "JWT must have 3 segments");
        let header: Value = serde_json::from_slice(
            &base64::Engine::decode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                segments[0],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(header["alg"], "RS256");
        assert_eq!(header["typ"], "JWT");

        // prepare with a fixed access_token builds the regional Vertex URL + bearer.
        let mut secret = sa_secret();
        secret["access_token"] = json!("tok-abc");
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gemini-2.5-pro",
            method: Method::POST,
            path: "/v1beta/models/gemini-2.5-pro:generateContent",
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{}"),
        };
        let req = VertexChannel.prepare(ctx).unwrap().into_http();
        assert_eq!(
            req.uri().to_string(),
            "https://us-central1-aiplatform.googleapis.com/v1beta1/projects/proj-123/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer tok-abc"
        );
        assert_eq!(
            req.headers().get("content-type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn list_models_builds_vertex_list_endpoint() {
        // The model-pull sends GET /v1beta/models (no model, no `:verb`); vertex
        // must build the Model Garden `ListPublisherModels` endpoint — NOT
        // project-scoped (the project/location prefix 404s on LIST).
        let mut secret = sa_secret();
        secret["access_token"] = json!("tok-abc");
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "",
            method: Method::GET,
            path: "/v1beta/models",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        };
        let req = VertexChannel.prepare(ctx).unwrap().into_http();
        assert_eq!(
            req.uri().to_string(),
            "https://us-central1-aiplatform.googleapis.com/v1beta1/publishers/google/models"
        );
    }

    #[test]
    fn needs_refresh_expiry() {
        let now_ms = crate::util::time::unix_now().saturating_mul(1000);
        // No access_token → must refresh.
        assert!(VertexChannel.needs_refresh(&json!({})));
        // Fresh token, expiry well in the future → no refresh.
        assert!(!VertexChannel.needs_refresh(&json!({
            "access_token": "t",
            "expires_at_ms": now_ms + 600_000,
        })));
        // Near expiry (inside the skew window) → refresh.
        assert!(VertexChannel.needs_refresh(&json!({
            "access_token": "t",
            "expires_at_ms": now_ms + 10_000,
        })));
    }

    #[test]
    fn shape_response_normalizes_gemini_content_only() {
        use crate::protocol::{ContentGenerationKind, Operation, OperationKey, Provider as P};

        let body = Bytes::from(
            json!({"candidates": [{"citationMetadata": {"citations": [{"uri": "x"}]}}]})
                .to_string(),
        );

        // Gemini content op → citations renamed to citationSources.
        let content_ctx = ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
        };
        let out = VertexChannel.shape_response(body.clone(), &content_ctx);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["candidates"][0]["citationMetadata"]["citationSources"][0]["uri"],
            "x"
        );

        // Non-content op (e.g. ListModels) → body untouched.
        let list_ctx = ShapeCtx {
            op: OperationKey::provider(Operation::ListModels, P::Gemini),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
        };
        let out2 = VertexChannel.shape_response(body.clone(), &list_ctx);
        assert_eq!(out2, body);
    }
}
