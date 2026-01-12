use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;

use crate::context::AppContext;
use crate::formats::claude::count_tokens::CountTokensResponse;
use crate::formats::claude::file::{
    FileDeleteResponse, FileGetResponse, FileUploadResponse, FilesListResponse,
};
use crate::formats::claude::messages::MessageCreateResponse;
use crate::formats::claude::model_get::ModelGetResponse;
use crate::formats::claude::models_list::ModelsListResponse;
use crate::formats::claude::skill::{
    SkillCreateResponse, SkillDeleteResponse, SkillGetResponse, SkillsListResponse,
};
use crate::formats::claude::skill_version::{
    SkillVersionCreateResponse, SkillVersionDeleteResponse, SkillVersionGetResponse,
    SkillVersionsListResponse,
};
use crate::formats::claude::stream::ClaudeStreamEvent;
use crate::providers::common::usage::{
    build_claude_stream_usage_record, build_claude_usage_record,
};
use crate::providers::claude::ClaudeProvider;
use crate::providers::claude::transform;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    ClaudeFiles, ClaudeMessages, ClaudeMessagesCountTokens, ClaudeModelGet, ClaudeModelsList,
    ClaudeSkillDelete, ClaudeSkillGet, ClaudeSkillVersionDelete, ClaudeSkillVersionGet,
    ClaudeSkillVersionsCreate, ClaudeSkillVersionsList, ClaudeSkillsCreate, ClaudeSkillsList,
    DownstreamRequest, UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, apply_query, build_url, parse_json_response, parse_sse_response,
    render_bytes_response, render_json_response, render_sse_response, send_bytes_request_with_status,
    send_delete_request_with_status, send_get_request_with_status, send_json_request_with_status,
};

#[derive(Debug, Clone, Copy, Serialize)]
struct EmptyJsonBody;

#[async_trait::async_trait]
impl ClaudeMessages for ClaudeProvider {
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
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/messages")?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
            &body,
        )
        .await?;
        if body.stream == Some(true) {
            let parsed = parse_sse_response::<ClaudeStreamEvent>(res).await?;
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.key.clone();
            let mut message_id: Option<String> = None;
            let mut message_model: Option<String> = None;
            let mut recorded = false;
            let mapped = transform::map_sse_response(parsed, move |event| {
                match &event {
                    ClaudeStreamEvent::MessageStart { message } => {
                        message_id = Some(message.id.clone());
                        message_model = Some(message.model.as_str().to_string());
                        if !recorded && let Some(usage) = message.usage.clone() {
                            let record = build_claude_stream_usage_record(
                                ProviderKind::Claude,
                                message_id.as_deref(),
                                message_model.as_deref().unwrap_or_default(),
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
                    }
                    ClaudeStreamEvent::MessageDelta { usage, .. } => {
                        if !recorded && let Some(usage) = usage.clone() {
                            let record = build_claude_stream_usage_record(
                                ProviderKind::Claude,
                                message_id.as_deref(),
                                message_model.as_deref().unwrap_or_default(),
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
                    }
                    _ => {}
                }
                Ok(event)
            });

            render_sse_response(mapped)
        } else {
            let parsed = parse_json_response::<MessageCreateResponse>(res).await?;

            let mapped = transform::map_json_response(parsed, Ok)?;
            if let ParsedBody::Ok(ref response) = mapped.body {
                let record = build_claude_usage_record(
                    ProviderKind::Claude,
                    Some(response.id.as_str()),
                    response.model.as_str(),
                    caller_api_key.clone(),
                    credential.key.clone(),
                    response.usage.clone(),
                );
                let _ = ctx.usage_store().record(record).await;
            }

            render_json_response(mapped)
        }
    }
}

#[async_trait::async_trait]
impl ClaudeMessagesCountTokens for ClaudeProvider {
    async fn claude_messages_count_tokens(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::claude::count_tokens::CountTokensRequest>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/messages/count_tokens")?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
            &body,
        )
        .await?;
        let parsed = parse_json_response::<CountTokensResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeModelsList for ClaudeProvider {
    async fn claude_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/models")?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<ModelsListResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeModelGet for ClaudeProvider {
    async fn claude_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let path = format!("/v1/models/{model_id}");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<ModelGetResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillsCreate for ClaudeProvider {
    async fn claude_skills_create(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/skills")?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
            &EmptyJsonBody,
        )
        .await?;
        let parsed = parse_json_response::<SkillCreateResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillsList for ClaudeProvider {
    async fn claude_skills_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/skills")?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<SkillsListResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillGet for ClaudeProvider {
    async fn claude_skill_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let path = format!("/v1/skills/{skill_id}");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<SkillGetResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillDelete for ClaudeProvider {
    async fn claude_skill_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let path = format!("/v1/skills/{skill_id}");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_delete_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<SkillDeleteResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionsCreate for ClaudeProvider {
    async fn claude_skill_versions_create(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let path = format!("/v1/skills/{skill_id}/versions");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
            &EmptyJsonBody,
        )
        .await?;
        let parsed = parse_json_response::<SkillVersionCreateResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionsList for ClaudeProvider {
    async fn claude_skill_versions_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let path = format!("/v1/skills/{skill_id}/versions");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<SkillVersionsListResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionGet for ClaudeProvider {
    async fn claude_skill_version_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
        version: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let path = format!("/v1/skills/{skill_id}/versions/{version}");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version_header = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version_header);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<SkillVersionGetResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeSkillVersionDelete for ClaudeProvider {
    async fn claude_skill_version_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
        version: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let path = format!("/v1/skills/{skill_id}/versions/{version}");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version_header = super::anthropic_version(&headers)?;
        super::ensure_skills_beta(&mut headers);
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_delete_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version_header);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<SkillVersionDeleteResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl ClaudeFiles for ClaudeProvider {
    async fn claude_files_upload(
        ctx: &AppContext,
        req: DownstreamRequest<Vec<u8>>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_files_beta(&mut headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/files")?;
        apply_query(&mut url, &query);
        let res = send_bytes_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
            body,
        )
        .await?;
        let parsed = parse_json_response::<FileUploadResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn claude_files_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_files_beta(&mut headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/files")?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<FilesListResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn claude_files_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        file_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_files_beta(&mut headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let path = format!("/v1/files/{file_id}");
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<FileGetResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn claude_files_download(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        file_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_files_beta(&mut headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let path = format!("/v1/files/{file_id}/content");
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        render_bytes_response(res).await
    }

    async fn claude_files_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        file_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            mut headers,
            query,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        super::ensure_files_beta(&mut headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let path = format!("/v1/files/{file_id}");
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_delete_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<FileDeleteResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }
}
