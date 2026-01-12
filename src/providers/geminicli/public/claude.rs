use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde_json::Value;
use std::time::Duration;

use async_stream::stream;
use futures_util::StreamExt;

use crate::context::AppContext;
use crate::formats::gemini::query::apply_gemini_query;
use crate::providers::common::usage::build_gemini_usage_record;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    ClaudeMessages, ClaudeMessagesCountTokens, ClaudeModelGet, ClaudeModelsList, ClaudeSkillDelete,
    ClaudeSkillGet, ClaudeSkillVersionDelete, ClaudeSkillVersionGet, ClaudeSkillVersionsCreate,
    ClaudeSkillVersionsList, ClaudeSkillsCreate, ClaudeSkillsList, DownstreamRequest, UpstreamRequest,
};
use crate::providers::geminicli::constants::GEMINICLI_USER_AGENT;
use crate::providers::geminicli::GeminiCliProvider;
use crate::providers::geminicli::transform;
use crate::providers::router::{
    AuthMode, ParsedBody, ParsedSseBody, ParsedSseResponse, SseMessage, build_url,
    not_implemented_response, parse_json_response, parse_sse_response, render_json_response,
    render_sse_response, send_json_request_with_status, send_json_request_with_status_timeout,
};

#[async_trait::async_trait]
impl ClaudeMessages for GeminiCliProvider {
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
        let model = super::gemini::normalize_generate_model_name(&request_model);

        let gemini_request = crate::formats::transform::gen_claude_messages_to_gemini_generate::request(body)
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential_for_model(&model)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = super::gemini::ensure_access_token(ctx, credential).await?;

        if stream {
            let mut url = build_url(&provider.setting.base_url, "v1internal:streamGenerateContent")?;
            apply_gemini_query(&mut url, &query, true);
            let payload = super::gemini::GeminiCliGenerateRequest {
                model: &model,
                project: credential.project_id.as_str(),
                request: &gemini_request,
            };
            let res = send_json_request_with_status(
                ctx,
                ProviderKind::GeminiCli,
                credential.project_id.as_str(),
                &model,
                ctx.http_client(),
                url.as_str(),
                &headers,
                AuthMode::AuthorizationBearer,
                access_token.as_str(),
                |headers| {
                    headers.insert(header::USER_AGENT, HeaderValue::from_static(GEMINICLI_USER_AGENT));
                    headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
                    Ok(())
                },
                &payload,
            )
            .await?;

            let parsed = parse_sse_response::<Value>(res).await?;
            let ParsedSseResponse { status, headers, body } = parsed;
            let mapped_body = match body {
                ParsedSseBody::Error(value) => ParsedSseBody::Error(value),
                ParsedSseBody::Stream(mut stream) => {
                    let usage_store = ctx.usage_store();
                    let provider_credential_id = credential.project_id.clone();
                    let response_model = request_model.clone();
                    let model_name = model.clone();
                    let mut state =
                        crate::formats::transform::gen_gemini_generate_to_claude_messages_stream::ClaudeStreamState::new();
                    let mut recorded = false;

                    let output_stream = stream! {
                        while let Some(item) = stream.next().await {
                            match item {
                                Ok(SseMessage::Data(event)) => {
                                    let event = match super::gemini::map_wrapped_sse_event(event) {
                                        Ok(value) => value,
                                        Err(status) => {
                                            yield Err(status);
                                            return;
                                        }
                                    };
                                    if !recorded && let Some(usage) = event.usage_metadata.clone() {
                                        let record = build_gemini_usage_record(
                                            ProviderKind::GeminiCli,
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
            let mut url = build_url(&provider.setting.base_url, "v1internal:generateContent")?;
            apply_gemini_query(&mut url, &query, false);
            let payload = super::gemini::GeminiCliGenerateRequest {
                model: &model,
                project: credential.project_id.as_str(),
                request: &gemini_request,
            };
            let res = send_json_request_with_status_timeout(
                ctx,
                ProviderKind::GeminiCli,
                credential.project_id.as_str(),
                &model,
                ctx.http_client(),
                url.as_str(),
                &headers,
                AuthMode::AuthorizationBearer,
                access_token.as_str(),
                |headers| {
                    headers.insert(header::USER_AGENT, HeaderValue::from_static(GEMINICLI_USER_AGENT));
                    headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
                    Ok(())
                },
                &payload,
                Duration::from_secs(300),
            )
            .await?;

            let parsed = parse_json_response::<Value>(res).await?;
            let mapped = transform::map_json_response(parsed, super::gemini::map_wrapped_json_response)?;
            if let ParsedBody::Ok(ref response) = mapped.body
                && let Some(usage) = response.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::GeminiCli,
                    response.response_id.as_deref(),
                    &model,
                    caller_api_key.clone(),
                    credential.project_id.clone(),
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
impl ClaudeMessagesCountTokens for GeminiCliProvider {
    async fn claude_messages_count_tokens(
        _ctx: &AppContext,
        _req: DownstreamRequest<crate::formats::claude::count_tokens::CountTokensRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeModelsList for GeminiCliProvider {
    async fn claude_models_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeModelGet for GeminiCliProvider {
    async fn claude_model_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _model: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillsList for GeminiCliProvider {
    async fn claude_skills_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillsCreate for GeminiCliProvider {
    async fn claude_skills_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillGet for GeminiCliProvider {
    async fn claude_skill_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillDelete for GeminiCliProvider {
    async fn claude_skill_delete(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionsList for GeminiCliProvider {
    async fn claude_skill_versions_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionsCreate for GeminiCliProvider {
    async fn claude_skill_versions_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionGet for GeminiCliProvider {
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
impl ClaudeSkillVersionDelete for GeminiCliProvider {
    async fn claude_skill_version_delete(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _skill_id: String,
        _version: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}
