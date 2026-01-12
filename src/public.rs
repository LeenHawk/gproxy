use std::sync::Arc;

use axum::Router;
use axum::extract::Extension;
use axum::routing::get;

use crate::context::AppContext;
#[cfg(feature = "provider-aistudio")]
use crate::providers::aistudio;
#[cfg(feature = "provider-antigravity")]
use crate::providers::antigravity;
#[cfg(feature = "provider-claude")]
use crate::providers::claude;
#[cfg(feature = "provider-claudecode")]
use crate::providers::claudecode;
#[cfg(feature = "provider-codex")]
use crate::providers::codex;
#[cfg(feature = "provider-deepseek")]
use crate::providers::deepseek;
#[cfg(feature = "provider-geminicli")]
use crate::providers::geminicli;
#[cfg(feature = "provider-nvidia")]
use crate::providers::nvidia;
#[cfg(feature = "provider-openai")]
use crate::providers::openai;
use crate::providers::router as provider_router;
#[cfg(feature = "provider-vertex")]
use crate::providers::vertex;
#[cfg(feature = "provider-vertexexpress")]
use crate::providers::vertexexpress;

pub fn router(ctx: Arc<AppContext>) -> Router {
    let mut router = Router::new();
    #[cfg(feature = "provider-openai")]
    {
        router = router.nest(
            "/openai",
            provider_router::provider_router_with_responses::<openai::OpenAIProvider>(),
        );
    }
    #[cfg(feature = "provider-claude")]
    {
        router = router.nest(
            "/claude",
            provider_router::claude_router::<claude::ClaudeProvider>(),
        );
    }
    #[cfg(feature = "provider-aistudio")]
    {
        router = router.nest(
            "/aistudio",
            provider_router::provider_router::<aistudio::AIStudioProvider>(),
        );
    }
    #[cfg(feature = "provider-claudecode")]
    {
        router = router.nest(
            "/claudecode",
            provider_router::provider_router::<claudecode::ClaudeCodeProvider>()
                .route("/usage", get(claudecode::claudecode_usage))
                .route("/oauth", get(claudecode::claudecode_oauth_start))
                .route("/oauth/callback", get(claudecode::claudecode_oauth_callback)),
        );
    }
    #[cfg(feature = "provider-codex")]
    {
        router = router.nest(
            "/codex",
            provider_router::provider_router::<codex::CodexProvider>()
                .route("/usage", get(codex::codex_usage))
                .route("/oauth", get(codex::codex_oauth_start))
                .route("/oauth/callback", get(codex::codex_oauth_callback)),
        );
    }
    #[cfg(feature = "provider-deepseek")]
    {
        router = router.nest(
            "/deepseek",
            provider_router::provider_router::<deepseek::DeepSeekProvider>(),
        );
    }
    #[cfg(feature = "provider-geminicli")]
    {
        router = router.nest(
            "/geminicli",
            provider_router::provider_router::<geminicli::GeminiCliProvider>()
                .route("/oauth", get(geminicli::geminicli_oauth_start))
                .route("/oauth/callback", get(geminicli::geminicli_oauth_callback)),
        );
    }
    #[cfg(feature = "provider-antigravity")]
    {
        router = router.nest(
            "/antigravity",
            provider_router::provider_router::<antigravity::AntigravityProvider>()
                .route("/usage", get(antigravity::antigravity_usage))
                .route("/oauth", get(antigravity::antigravity_oauth_start))
                .route("/oauth/callback", get(antigravity::antigravity_oauth_callback)),
        );
    }
    #[cfg(feature = "provider-nvidia")]
    {
        router = router.nest(
            "/nvidia",
            provider_router::provider_router::<nvidia::NvidiaProvider>(),
        );
    }
    #[cfg(feature = "provider-vertex")]
    {
        router = router.nest(
            "/vertex",
            provider_router::provider_router::<vertex::VertexProvider>(),
        );
    }
    #[cfg(feature = "provider-vertexexpress")]
    {
        router = router.nest(
            "/vertexexpress",
            provider_router::provider_router::<vertexexpress::VertexExpressProvider>(),
        );
    }
    router.layer(Extension(ctx))
}
