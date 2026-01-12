use axum::http::StatusCode;
use axum::response::Response;

use async_stream::stream;
use futures_util::StreamExt;

use crate::context::AppContext;
use crate::formats::gemini::generate_content::GenerateContentResponse;
use crate::formats::gemini::query::apply_gemini_query;
use crate::formats::gemini::stream_generate_content::StreamGenerateContentResponse;
use crate::providers::common::usage::{
    build_gemini_usage_record,
};
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    ClaudeMessages, ClaudeMessagesCountTokens, ClaudeModelGet, ClaudeModelsList, ClaudeSkillDelete,
    ClaudeSkillGet, ClaudeSkillVersionDelete, ClaudeSkillVersionGet, ClaudeSkillVersionsCreate,
    ClaudeSkillVersionsList, ClaudeSkillsCreate, ClaudeSkillsList, DownstreamRequest, GeminiVersion,
    UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, ParsedSseBody, ParsedSseResponse, SseMessage, build_url,
    not_implemented_response, parse_json_response, parse_sse_response, render_json_response,
    render_sse_response, send_json_request_with_status,
};
use crate::providers::vertexexpress::VertexExpressProvider;
use crate::providers::vertexexpress::transform;

#[async_trait::async_trait]
impl ClaudeMessages for VertexExpressProvider {
    async fn claude_messages(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::claude::messages::MessageCreateRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);

        let stream = body.stream == Some(true);
        let request_model = body.model.as_str().to_string();

        let gemini_request = crate::formats::transform::gen_claude_messages_to_gemini_generate::request(body)
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        let model_path = super::gemini::vertex_express_model_path(&request_model);
        let version = GeminiVersion::V1Beta;

        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        if stream {
            let path = format!(
                "{}/{}:streamGenerateContent",
                super::gemini::vertex_express_version_path(version),
                model_path
            );
            let mut url = build_url(&provider.setting.base_url, &path)?;
            apply_gemini_query(&mut url, &query, true);
            let res = send_json_request_with_status(
                ctx,
                ProviderKind::VertexExpress,
                credential.key.as_str(),
                &request_model,
                ctx.http_client(),
                url.as_str(),
                &headers,
                AuthMode::XGoogApiKey,
                credential.key.as_str(),
                |_| Ok(()),
                &gemini_request,
            )
            .await?;

            let parsed = parse_sse_response::<StreamGenerateContentResponse>(res).await?;
            let ParsedSseResponse { status, headers, body } = parsed;
            let mapped_body = match body {
                ParsedSseBody::Error(value) => ParsedSseBody::Error(value),
                ParsedSseBody::Stream(mut stream) => {
                    let usage_store = ctx.usage_store();
                    let provider_credential_id = credential.key.clone();
                    let response_model = request_model.clone();
                    let model_name = request_model.clone();
                    let mut state =
                        crate::formats::transform::gen_gemini_generate_to_claude_messages_stream::ClaudeStreamState::new();
                    let mut recorded = false;

                    let output_stream = stream! {
                        while let Some(item) = stream.next().await {
                            match item {
                                Ok(SseMessage::Data(event)) => {
                                    if !recorded && let Some(usage) = event.usage_metadata.clone() {
                                        let record = build_gemini_usage_record(
                                            ProviderKind::VertexExpress,
                                            event.response_id.as_deref(),
                                            &model_name,
                                            caller_api_key.clone(),
                                            provider_credential_id.clone(),
                                            usage,
                                        );
                                        recorded = true;
                                        let usage_store = usage_store.clone();
                                        tokio::spawn(async move {
                                            let _ = usage_store.record(record).await;
                                        });
                                    }
                                    let converted = match state.handle_event(&event, &response_model) {
                                        Ok(events) => events,
                                        Err(_) => {
                                            yield Err(StatusCode::BAD_GATEWAY);
                                            return;
                                        }
                                    };
                                    for output in converted {
                                        yield Ok(SseMessage::Data(output));
                                    }
                                }
                                Ok(SseMessage::Done) => {
                                    let converted = match state.finish() {
                                        Ok(events) => events,
                                        Err(_) => {
                                            yield Err(StatusCode::BAD_GATEWAY);
                                            return;
                                        }
                                    };
                                    for output in converted {
                                        yield Ok(SseMessage::Data(output));
                                    }
                                    return;
                                }
                                Err(status) => {
                                    yield Err(status);
                                    return;
                                }
                            }
                        }

                        let converted = match state.finish() {
                            Ok(events) => events,
                            Err(_) => {
                                yield Err(StatusCode::BAD_GATEWAY);
                                return;
                            }
                        };
                        for output in converted {
                            yield Ok(SseMessage::Data(output));
                        }
                    };

                    ParsedSseBody::Stream(Box::pin(output_stream))
                }
            };

            render_sse_response(ParsedSseResponse {
                status,
                headers,
                body: mapped_body,
            })
        } else {
            let path = format!(
                "{}/{}:generateContent",
                super::gemini::vertex_express_version_path(version),
                model_path
            );
            let mut url = build_url(&provider.setting.base_url, &path)?;
            apply_gemini_query(&mut url, &query, false);
            let res = send_json_request_with_status(
                ctx,
                ProviderKind::VertexExpress,
                credential.key.as_str(),
                &request_model,
                ctx.http_client(),
                url.as_str(),
                &headers,
                AuthMode::XGoogApiKey,
                credential.key.as_str(),
                |_| Ok(()),
                &gemini_request,
            )
            .await?;

            let parsed = parse_json_response::<GenerateContentResponse>(res).await?;
            let mapped = transform::map_json_response(parsed, Ok)?;
            if let ParsedBody::Ok(ref response) = mapped.body
                && let Some(usage) = response.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::VertexExpress,
                    response.response_id.as_deref(),
                    &request_model,
                    caller_api_key.clone(),
                    credential.key.clone(),
                    usage,
                );
                let _ = ctx.usage_store().record(record).await;
            }

            let mapped = transform::map_json_response(mapped, |response| {
                crate::formats::transform::gen_gemini_generate_to_claude_messages::response(
                    response,
                    &request_model,
                )
                .map_err(|_| StatusCode::BAD_GATEWAY)
            })?;

            render_json_response(mapped)
        }
    }
}

#[async_trait::async_trait]
impl ClaudeMessagesCountTokens for VertexExpressProvider {
    async fn claude_messages_count_tokens(
        _ctx: &AppContext,
        _req: DownstreamRequest<crate::formats::claude::count_tokens::CountTokensRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeModelsList for VertexExpressProvider {
    async fn claude_models_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeModelGet for VertexExpressProvider {
    async fn claude_model_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _model: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillsList for VertexExpressProvider {
    async fn claude_skills_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillsCreate for VertexExpressProvider {
    async fn claude_skills_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillGet for VertexExpressProvider {
    async fn claude_skill_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillDelete for VertexExpressProvider {
    async fn claude_skill_delete(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionsList for VertexExpressProvider {
    async fn claude_skill_versions_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionsCreate for VertexExpressProvider {
    async fn claude_skill_versions_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionGet for VertexExpressProvider {
    async fn claude_skill_version_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
        _version: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionDelete for VertexExpressProvider {
    async fn claude_skill_version_delete(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
        _version: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}
