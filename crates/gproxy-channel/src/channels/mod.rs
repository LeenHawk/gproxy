//! Concrete channel implementations. None is enabled by default; the host
//! registers the ones it compiled in.
//!
//! One list below declares both the modules and [`compiled_in`], the vector a
//! host registers. They were two hand-written lists for a while and drifted
//! immediately: a channel landed as a module, nothing added it to the host's
//! registry, and it was unreachable from every binary while its tests passed.
//! Adding a channel is now one line — its feature, its module and the value to
//! register — and forgetting the registry is not expressible.

use std::sync::Arc;

use crate::BaseChannel;

/// `feature => module, the values to register`.
///
/// A value is an expression so a channel that needs constructing says so;
/// most are unit structs and name themselves. A module may register more
/// than one channel when the products share a wire (`opencode`).
macro_rules! channels {
    ($($feature:literal => $module:ident, $($instance:expr),+);+ $(;)?) => {
        $(
            #[cfg(feature = $feature)]
            pub mod $module;
        )+

        /// Every channel this build compiled in, ready to register.
        ///
        /// A build with no channel feature returns an empty vector, which is
        /// legitimate: a host may register only channels of its own.
        pub fn compiled_in() -> Vec<Arc<dyn BaseChannel>> {
            vec![
                $($(
                    #[cfg(feature = $feature)]
                    (Arc::new($instance) as Arc<dyn BaseChannel>),
                )+)+
            ]
        }
    };
}

channels! {
    "aistudio" => aistudio, aistudio::Aistudio;
    "antigravity" => antigravity, antigravity::Antigravity;
    "aws_bedrock" => aws_bedrock, aws_bedrock::AwsBedrock;
    "azure" => azure, azure::Azure;
    "claudeapi" => claudeapi, claudeapi::Claudeapi;
    "claudecode" => claudecode, claudecode::Claudecode;
    "claudeweb" => claudeweb, claudeweb::ClaudeWeb::new();
    "cline" => cline, cline::Cline;
    "cloudflare_ai_gateway" => cloudflare_ai_gateway, cloudflare_ai_gateway::CloudflareAiGateway;
    "codex" => codex, codex::Codex;
    "copilotcli" => copilotcli, copilotcli::CopilotCli;
    "custom" => custom, custom::Custom;
    "dashscope" => dashscope, dashscope::DashScope;
    "deepseek" => deepseek, deepseek::DeepSeek;
    "devin" => devin, devin::Devin;
    "geminicli" => geminicli, geminicli::GeminiCli;
    "grokbuild" => grokbuild, grokbuild::GrokBuild;
    "kimi" => kimi, kimi::Kimi;
    "kiro" => kiro, kiro::Kiro;
    "nvidia" => nvidia, nvidia::Nvidia;
    "openai" => openai, openai::OpenAi;
    "opencode" => opencode, opencode::OpenCode::ZEN, opencode::OpenCode::GO;
    "openrouter" => openrouter, openrouter::OpenRouter;
    "vercel" => vercel, vercel::Vercel;
    "vertex" => vertex, vertex::Vertex;
    "vertexexpress" => vertexexpress, vertexexpress::VertexExpress;
    "workbuddy" => workbuddy, workbuddy::WorkBuddy;
    "xai" => xai, xai::Xai;
}

/// Code several channels share. Its own list, because it is not a channel and
/// the one channel missing from it — `devin` — speaks a wire format nothing
/// else does and borrows none of it.
#[cfg(any(
    feature = "aistudio",
    feature = "antigravity",
    feature = "aws_bedrock",
    feature = "azure",
    feature = "claudeapi",
    feature = "codex",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "cline",
    feature = "cloudflare_ai_gateway",
    feature = "copilotcli",
    feature = "custom",
    feature = "geminicli",
    feature = "grokbuild",
    feature = "dashscope",
    feature = "deepseek",
    feature = "kimi",
    feature = "kiro",
    feature = "nvidia",
    feature = "openai",
    feature = "opencode",
    feature = "openrouter",
    feature = "vercel",
    feature = "vertex",
    feature = "vertexexpress",
    feature = "workbuddy",
    feature = "xai"
))]
mod shared;
