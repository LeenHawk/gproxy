mod json;
mod stream;

use serde::de::DeserializeOwned;
use serde::Serialize;

use gproxy_provider_core::{DownstreamContext, ProxyRequest, ProxyResponse, UpstreamPassthroughError};
use gproxy_transform::count_tokens;
use gproxy_transform::generate_content;
use gproxy_transform::generate_content::claude2openai_chat_completions;
use gproxy_transform::generate_content::claude2openai_response::stream::ClaudeToOpenAIResponseStreamState;
use gproxy_transform::generate_content::gemini2openai_chat_completions;
use gproxy_transform::generate_content::gemini2openai_response::stream::GeminiToOpenAIResponseStreamState;
use gproxy_transform::generate_content::openai_chat_completions2openai_response;
use gproxy_transform::generate_content::openai_chat_completions2openai_response::stream::OpenAIChatCompletionToResponseStreamState;
use gproxy_transform::generate_content::openai_chat_completions2gemini;
use gproxy_transform::generate_content::openai_response2claude::stream::OpenAIResponseToClaudeStreamState;
use gproxy_transform::generate_content::openai_response2openai_chat_completions;
use gproxy_transform::generate_content::openai_response2openai_chat_completions::stream::OpenAIResponseToChatCompletionStreamState;
use gproxy_transform::generate_content::claude2gemini::stream::GeminiToClaudeStreamState;
use gproxy_transform::generate_content::claude2openai_chat_completions::stream::ClaudeToOpenAIChatCompletionStreamState;
use gproxy_transform::generate_content::gemini2claude::stream::ClaudeToGeminiStreamState;
use gproxy_transform::generate_content::gemini2openai_chat_completions::stream::GeminiToOpenAIChatCompletionStreamState;
use gproxy_transform::generate_content::openai_chat_completions2claude::stream::OpenAIToClaudeChatCompletionStreamState;
use gproxy_transform::generate_content::openai_chat_completions2gemini::stream::OpenAIChatCompletionToGeminiStreamState;
use gproxy_transform::generate_content::openai_response2gemini::stream::OpenAIResponseToGeminiStreamState;
use gproxy_transform::get_model;
use gproxy_transform::list_models;

use super::plan::{
    CountTokensPlan, GenerateContentPlan, ModelsGetPlan, ModelsListPlan, StreamContentPlan,
    TransformPlan, UsageKind,
};
use super::record::record_upstream_only;
use super::stream::{
    gemini_generate_to_stream, gemini_stream_to_generate, now_epoch_seconds, sse_claude_bytes,
    sse_json_bytes,
};
use super::{DispatchProvider, UpstreamOk};

use self::json::transform_json_response;
use self::stream::{
    transform_claude_stream, transform_gemini_stream, transform_openai_chat_stream,
    transform_openai_responses_stream,
};

async fn run_json_transform<P, ReqIn, ReqUp, ResUp, ResDown>(
    provider: &P,
    ctx: DownstreamContext,
    usage: UsageKind,
    request: ReqIn,
    to_upstream: fn(ReqIn) -> ReqUp,
    to_proxy: fn(ReqUp) -> ProxyRequest,
    to_downstream: fn(ResUp) -> ResDown,
) -> Result<ProxyResponse, UpstreamPassthroughError>
where
    P: DispatchProvider,
    ResUp: DeserializeOwned,
    ResDown: Serialize,
{
    let ctx_native = ctx.upstream();
    let upstream_req = to_proxy(to_upstream(request));
    let upstream_ctx = ctx_native.clone();
    let UpstreamOk { response, meta } =
        provider.call_native(upstream_req, ctx_native.clone()).await?;
    let upstream_recorded = record_upstream_only(response, meta, usage, upstream_ctx).await?;
    transform_json_response(upstream_recorded, ctx, to_downstream)
}

pub(super) async fn dispatch_transform<P: DispatchProvider>(
    provider: &P,
    plan: TransformPlan,
    usage: UsageKind,
    ctx: DownstreamContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    macro_rules! run_json {
        ($request:expr, $to_up:expr, $to_proxy:expr, $to_down:expr) => {
            run_json_transform(
                provider,
                ctx,
                usage.clone(),
                $request,
                $to_up,
                $to_proxy,
                $to_down,
            )
            .await
        };
    }
    macro_rules! run_claude_stream {
        ($request:expr, $factory:expr) => {
            transform_claude_stream(provider, $request, ctx, usage.clone(), $factory).await
        };
    }
    macro_rules! run_gemini_stream {
        ($request:expr, $factory:expr) => {
            transform_gemini_stream(provider, $request, ctx, usage.clone(), $factory).await
        };
    }
    macro_rules! run_openai_chat_stream {
        ($request:expr, $factory:expr) => {
            transform_openai_chat_stream(provider, $request, ctx, usage.clone(), $factory).await
        };
    }
    macro_rules! run_openai_responses_stream {
        ($request:expr, $factory:expr) => {
            transform_openai_responses_stream(provider, $request, ctx, usage.clone(), $factory).await
        };
    }
    macro_rules! map_stream {
        ($state:expr, $method:ident) => {{
            let mut state = $state;
            move |event| {
                state
                    .$method(event)
                    .into_iter()
                    .filter_map(|response| sse_json_bytes(&response))
                    .collect()
            }
        }};
    }
    macro_rules! map_claude_stream {
        ($state:expr, $method:ident) => {{
            let mut state = $state;
            move |event| {
                state
                    .$method(event)
                    .into_iter()
                    .filter_map(|response| sse_claude_bytes(&response))
                    .collect()
            }
        }};
    }

    match plan {
        TransformPlan::GenerateContent(plan) => match plan {
            GenerateContentPlan::Claude2Gemini(request) => run_json!(
                request,
                generate_content::claude2gemini::request::transform_request,
                ProxyRequest::GeminiGenerate,
                generate_content::claude2gemini::response::transform_response
            ),
            GenerateContentPlan::Claude2OpenAIResponses(request) => run_json!(
                request,
                generate_content::claude2openai_response::request::transform_request,
                ProxyRequest::OpenAIResponses,
                generate_content::openai_response2claude::response::transform_response
            ),
            GenerateContentPlan::Claude2OpenAIChat(request) => run_json!(
                request,
                claude2openai_chat_completions::request::transform_request,
                ProxyRequest::OpenAIChat,
                claude2openai_chat_completions::response::transform_response
            ),
            GenerateContentPlan::Gemini2Claude(request) => run_json!(
                request,
                generate_content::gemini2claude::request::transform_request,
                ProxyRequest::ClaudeMessages,
                generate_content::gemini2claude::response::transform_response
            ),
            GenerateContentPlan::Gemini2OpenAIResponses(request) => run_json!(
                request,
                generate_content::gemini2openai_response::request::transform_request,
                ProxyRequest::OpenAIResponses,
                generate_content::gemini2openai_response::response::transform_response
            ),
            GenerateContentPlan::Gemini2OpenAIChat(request) => run_json!(
                request,
                gemini2openai_chat_completions::request::transform_request,
                ProxyRequest::OpenAIChat,
                gemini2openai_chat_completions::response::transform_response
            ),
            GenerateContentPlan::OpenAIResponses2Claude(request) => run_json!(
                request,
                generate_content::openai_response2claude::request::transform_request,
                ProxyRequest::ClaudeMessages,
                generate_content::claude2openai_response::response::transform_response
            ),
            GenerateContentPlan::OpenAIResponses2Gemini(request) => run_json!(
                request,
                generate_content::openai_response2gemini::request::transform_request,
                ProxyRequest::GeminiGenerate,
                generate_content::openai_response2gemini::response::transform_response
            ),
            GenerateContentPlan::OpenAIResponses2OpenAIChat(request) => run_json!(
                request,
                openai_response2openai_chat_completions::request::transform_request,
                ProxyRequest::OpenAIChat,
                openai_response2openai_chat_completions::response::transform_response
            ),
            GenerateContentPlan::OpenAIChat2Claude(request) => run_json!(
                request,
                gproxy_transform::generate_content::openai_chat_completions2claude::request::transform_request,
                ProxyRequest::ClaudeMessages,
                gproxy_transform::generate_content::openai_chat_completions2claude::response::transform_response
            ),
            GenerateContentPlan::OpenAIChat2Gemini(request) => run_json!(
                request,
                openai_chat_completions2gemini::request::transform_request,
                ProxyRequest::GeminiGenerate,
                openai_chat_completions2gemini::response::transform_response
            ),
            GenerateContentPlan::OpenAIChat2OpenAIResponses(request) => run_json!(
                request,
                openai_chat_completions2openai_response::request::transform_request,
                ProxyRequest::OpenAIResponses,
                openai_chat_completions2openai_response::response::transform_response
            ),
        },
        TransformPlan::StreamContent(plan) => match plan {
            StreamContentPlan::Claude2Gemini(request) => {
                let gemini_request =
                    generate_content::claude2gemini::request::transform_request(request);
                let stream_request = gemini_generate_to_stream(gemini_request);
                run_gemini_stream!(
                    ProxyRequest::GeminiGenerateStream(stream_request),
                    || map_claude_stream!(GeminiToClaudeStreamState::new(), transform_response)
                )
            }
            StreamContentPlan::Claude2OpenAIResponses(request) => {
                let openai_request =
                    generate_content::claude2openai_response::request::transform_request(request);
                run_openai_responses_stream!(
                    ProxyRequest::OpenAIResponsesStream(openai_request),
                    || map_claude_stream!(OpenAIResponseToClaudeStreamState::new(), transform_event)
                )
            }
            StreamContentPlan::Claude2OpenAIChat(request) => {
                let openai_request =
                    claude2openai_chat_completions::request::transform_request(request);
                run_openai_chat_stream!(
                    ProxyRequest::OpenAIChatStream(openai_request),
                    || {
                        map_claude_stream!(
                            OpenAIToClaudeChatCompletionStreamState::new(),
                            transform_chunk
                        )
                    }
                )
            }
            StreamContentPlan::Gemini2Claude(request) => {
                let request = gemini_stream_to_generate(request);
                let claude_request =
                    generate_content::gemini2claude::request::transform_request(request);
                run_claude_stream!(
                    ProxyRequest::ClaudeMessagesStream(claude_request),
                    || map_stream!(ClaudeToGeminiStreamState::new(), transform_event)
                )
            }
            StreamContentPlan::Gemini2OpenAIResponses(request) => {
                let request = gemini_stream_to_generate(request);
                let openai_request =
                    generate_content::gemini2openai_response::request::transform_request(request);
                run_openai_responses_stream!(
                    ProxyRequest::OpenAIResponsesStream(openai_request),
                    || map_stream!(OpenAIResponseToGeminiStreamState::new(), transform_event)
                )
            }
            StreamContentPlan::Gemini2OpenAIChat(request) => {
                let request = gemini_stream_to_generate(request);
                let openai_request =
                    gemini2openai_chat_completions::request::transform_request(request);
                run_openai_chat_stream!(
                    ProxyRequest::OpenAIChatStream(openai_request),
                    || map_stream!(OpenAIChatCompletionToGeminiStreamState::new(), transform_event)
                )
            }
            StreamContentPlan::OpenAIResponses2Claude(request) => {
                let claude_request =
                    generate_content::openai_response2claude::request::transform_request(request);
                run_claude_stream!(
                    ProxyRequest::ClaudeMessagesStream(claude_request),
                    || {
                        let created = now_epoch_seconds();
                        map_stream!(ClaudeToOpenAIResponseStreamState::new(created), transform_event)
                    }
                )
            }
            StreamContentPlan::OpenAIResponses2Gemini(request) => {
                let gemini_request =
                    generate_content::openai_response2gemini::request::transform_request(request);
                let stream_request = gemini_generate_to_stream(gemini_request);
                run_gemini_stream!(
                    ProxyRequest::GeminiGenerateStream(stream_request),
                    || map_stream!(GeminiToOpenAIResponseStreamState::new(), transform_response)
                )
            }
            StreamContentPlan::OpenAIResponses2OpenAIChat(request) => {
                let openai_request =
                    openai_response2openai_chat_completions::request::transform_request(request);
                run_openai_chat_stream!(
                    ProxyRequest::OpenAIChatStream(openai_request),
                    || map_stream!(OpenAIChatCompletionToResponseStreamState::new(), transform_event)
                )
            }
            StreamContentPlan::OpenAIChat2Claude(request) => {
                let claude_request =
                    gproxy_transform::generate_content::openai_chat_completions2claude::request::transform_request(request);
                run_claude_stream!(
                    ProxyRequest::ClaudeMessagesStream(claude_request),
                    || {
                        let created = now_epoch_seconds();
                        map_stream!(ClaudeToOpenAIChatCompletionStreamState::new(created), transform_event)
                    }
                )
            }
            StreamContentPlan::OpenAIChat2Gemini(request) => {
                let gemini_request =
                    openai_chat_completions2gemini::request::transform_request(request);
                let stream_request = gemini_generate_to_stream(gemini_request);
                run_gemini_stream!(
                    ProxyRequest::GeminiGenerateStream(stream_request),
                    || map_stream!(GeminiToOpenAIChatCompletionStreamState::new(), transform_response)
                )
            }
            StreamContentPlan::OpenAIChat2OpenAIResponses(request) => {
                let openai_request =
                    openai_chat_completions2openai_response::request::transform_request(request);
                run_openai_responses_stream!(
                    ProxyRequest::OpenAIResponsesStream(openai_request),
                    || map_stream!(OpenAIResponseToChatCompletionStreamState::new(), transform_event)
                )
            }
        },
        TransformPlan::CountTokens(plan) => match plan {
            CountTokensPlan::Claude2Gemini(request) => run_json!(
                request,
                count_tokens::claude2gemini::request::transform_request,
                ProxyRequest::GeminiCountTokens,
                count_tokens::claude2gemini::response::transform_response
            ),
            CountTokensPlan::Claude2OpenAIInputTokens(request) => run_json!(
                request,
                count_tokens::claude2openai::request::transform_request,
                ProxyRequest::OpenAIInputTokens,
                count_tokens::claude2openai::response::transform_response
            ),
            CountTokensPlan::Gemini2Claude(request) => run_json!(
                request,
                count_tokens::gemini2claude::request::transform_request,
                ProxyRequest::ClaudeCountTokens,
                count_tokens::gemini2claude::response::transform_response
            ),
            CountTokensPlan::Gemini2OpenAIInputTokens(request) => run_json!(
                request,
                count_tokens::gemini2openai::request::transform_request,
                ProxyRequest::OpenAIInputTokens,
                count_tokens::gemini2openai::response::transform_response
            ),
            CountTokensPlan::OpenAIInputTokens2Claude(request) => run_json!(
                request,
                count_tokens::openai2claude::request::transform_request,
                ProxyRequest::ClaudeCountTokens,
                count_tokens::openai2claude::response::transform_response
            ),
            CountTokensPlan::OpenAIInputTokens2Gemini(request) => run_json!(
                request,
                count_tokens::openai2gemini::request::transform_request,
                ProxyRequest::GeminiCountTokens,
                count_tokens::openai2gemini::response::transform_response
            ),
        },
        TransformPlan::ModelsList(plan) => match plan {
            ModelsListPlan::Claude2Gemini(request) => run_json!(
                request,
                list_models::claude2gemini::request::transform_request,
                ProxyRequest::GeminiModelsList,
                list_models::claude2gemini::response::transform_response
            ),
            ModelsListPlan::Claude2OpenAI(request) => run_json!(
                request,
                list_models::claude2openai::request::transform_request,
                ProxyRequest::OpenAIModelsList,
                list_models::claude2openai::response::transform_response
            ),
            ModelsListPlan::Gemini2Claude(request) => run_json!(
                request,
                list_models::gemini2claude::request::transform_request,
                ProxyRequest::ClaudeModelsList,
                list_models::gemini2claude::response::transform_response
            ),
            ModelsListPlan::Gemini2OpenAI(request) => run_json!(
                request,
                list_models::gemini2openai::request::transform_request,
                ProxyRequest::OpenAIModelsList,
                list_models::gemini2openai::response::transform_response
            ),
            ModelsListPlan::OpenAI2Claude(request) => run_json!(
                request,
                list_models::openai2claude::request::transform_request,
                ProxyRequest::ClaudeModelsList,
                list_models::openai2claude::response::transform_response
            ),
            ModelsListPlan::OpenAI2Gemini(request) => run_json!(
                request,
                list_models::openai2gemini::request::transform_request,
                ProxyRequest::GeminiModelsList,
                list_models::openai2gemini::response::transform_response
            ),
        },
        TransformPlan::ModelsGet(plan) => match plan {
            ModelsGetPlan::Claude2Gemini(request) => run_json!(
                request,
                get_model::claude2gemini::request::transform_request,
                ProxyRequest::GeminiModelsGet,
                get_model::claude2gemini::response::transform_response
            ),
            ModelsGetPlan::Claude2OpenAI(request) => run_json!(
                request,
                get_model::claude2openai::request::transform_request,
                ProxyRequest::OpenAIModelsGet,
                get_model::claude2openai::response::transform_response
            ),
            ModelsGetPlan::Gemini2Claude(request) => run_json!(
                request,
                get_model::gemini2claude::request::transform_request,
                ProxyRequest::ClaudeModelsGet,
                get_model::gemini2claude::response::transform_response
            ),
            ModelsGetPlan::Gemini2OpenAI(request) => run_json!(
                request,
                get_model::gemini2openai::request::transform_request,
                ProxyRequest::OpenAIModelsGet,
                get_model::gemini2openai::response::transform_response
            ),
            ModelsGetPlan::OpenAI2Claude(request) => run_json!(
                request,
                get_model::openai2claude::request::transform_request,
                ProxyRequest::ClaudeModelsGet,
                get_model::openai2claude::response::transform_response
            ),
            ModelsGetPlan::OpenAI2Gemini(request) => run_json!(
                request,
                get_model::openai2gemini::request::transform_request,
                ProxyRequest::GeminiModelsGet,
                get_model::openai2gemini::response::transform_response
            ),
        },
    }
}
