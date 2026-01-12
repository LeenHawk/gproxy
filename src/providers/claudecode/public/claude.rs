use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;

use crate::context::AppContext;
use crate::formats::claude::count_tokens::CountTokensResponse;
use crate::formats::claude::messages::MessageCreateResponse;
use crate::formats::claude::stream::ClaudeStreamEvent;
use crate::formats::claude::types::{
    BetaTextBlockParam, SystemPrompt, TextBlockType,
};
use crate::providers::common::usage::{
    build_claude_stream_usage_record, build_claude_usage_record,
};
use crate::providers::claudecode::transform;
use crate::providers::claudecode::{
    CLAUDE_API_VERSION, CLAUDE_BETA_BASE, CLAUDE_CODE_SYSTEM_PROMPT, CLAUDE_CODE_USER_AGENT,
};
use crate::providers::claudecode::ClaudeCodeProvider;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    ClaudeMessages, ClaudeMessagesCountTokens, ClaudeModelGet, ClaudeModelsList, DownstreamRequest,
    UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, apply_query, build_url, not_implemented_response, parse_json_response,
    parse_sse_response, render_json_response, render_sse_response, send_json_request_with_status,
};

fn text_block(text: String) -> BetaTextBlockParam {
    BetaTextBlockParam {
        text,
        block_type: TextBlockType::Text,
        cache_control: None,
        citations: None,
    }
}

fn claude_code_prompt_block() -> BetaTextBlockParam {
    BetaTextBlockParam {
        text: CLAUDE_CODE_SYSTEM_PROMPT.to_string(),
        block_type: TextBlockType::Text,
        cache_control: None,
        citations: None,
    }
}

pub(super) fn normalize_system_prompt(system: Option<SystemPrompt>) -> Option<SystemPrompt> {
    let mut blocks = match system {
        None => Vec::new(),
        Some(SystemPrompt::Text(text)) => vec![text_block(text)],
        Some(SystemPrompt::Blocks(blocks)) => blocks,
    };
    let has_prompt = blocks
        .iter()
        .any(|block| block.text.contains(CLAUDE_CODE_SYSTEM_PROMPT));
    if !has_prompt {
        blocks.insert(0, claude_code_prompt_block());
    }
    if blocks.is_empty() {
        None
    } else {
        Some(SystemPrompt::Blocks(blocks))
    }
}

#[async_trait::async_trait]
impl ClaudeMessages for ClaudeCodeProvider {
    async fn claude_messages(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::claude::messages::MessageCreateRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            mut body,
            ..
        } = transform::to_upstream_request(req);
        body.system = normalize_system_prompt(body.system.take());
        let provider = super::get_settings_and_credentials(ctx).await?;
        let model = body.model.as_str();
        let credential = provider
            .pick_credential_for_model(model)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/messages")?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::ClaudeCode,
            credential.refresh_token.as_str(),
            model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |out_headers| {
                out_headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static(CLAUDE_API_VERSION),
                );
                out_headers.insert(
                    "anthropic-beta",
                    HeaderValue::from_static(CLAUDE_BETA_BASE),
                );
                out_headers.insert(
                    header::USER_AGENT,
                    HeaderValue::from_static(CLAUDE_CODE_USER_AGENT),
                );
                Ok(())
            },
            &body,
        )
        .await?;
        if body.stream == Some(true) {
            let parsed = parse_sse_response::<ClaudeStreamEvent>(res).await?;
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.refresh_token.clone();
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
                                ProviderKind::ClaudeCode,
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
                                ProviderKind::ClaudeCode,
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
                    ProviderKind::ClaudeCode,
                    Some(response.id.as_str()),
                    response.model.as_str(),
                    caller_api_key.clone(),
                    credential.refresh_token.clone(),
                    response.usage.clone(),
                );
                let _ = ctx.usage_store().record(record).await;
            }

            render_json_response(mapped)
        }
    }
}

#[async_trait::async_trait]
impl ClaudeMessagesCountTokens for ClaudeCodeProvider {
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
        let model = body.model.as_str();
        let credential = provider
            .pick_credential_for_model(model)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/messages/count_tokens")?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::ClaudeCode,
            credential.refresh_token.as_str(),
            model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |out_headers| {
                out_headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static(CLAUDE_API_VERSION),
                );
                out_headers.insert(
                    "anthropic-beta",
                    HeaderValue::from_static(CLAUDE_BETA_BASE),
                );
                out_headers.insert(
                    header::USER_AGENT,
                    HeaderValue::from_static(CLAUDE_CODE_USER_AGENT),
                );
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
impl ClaudeModelsList for ClaudeCodeProvider {
    async fn claude_models_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl ClaudeModelGet for ClaudeCodeProvider {
    async fn claude_model_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _model_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}
