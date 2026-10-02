use super::{DEFAULT_BASE_URL, MiniMax, MiniMaxConfig, video};
use crate::channel::{BaseChannel, ChannelError, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation};
use http::{HeaderValue, header};

pub(super) fn prepare(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
    if !MiniMax
        .native_dialects(ctx.provider, ctx.operation.operation)
        .contains(&ctx.operation.dialect)
    {
        return Err(ChannelError::UnsupportedOperation(ctx.operation));
    }
    let config: MiniMaxConfig = serde_json::from_value(ctx.provider.config.clone())
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    let key = ctx
        .credential
        .secret
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let mut request = ctx.request;
    let path = match ctx.operation.operation {
        Operation::ListModels | Operation::GetModel => request.path.clone(),
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            match ctx.operation.dialect {
                Dialect::Claude => "/anthropic/v1/messages".into(),
                _ => "/v1/chat/completions".into(),
            }
        }
        Operation::CreateVideo => "/v2/video_generation".into(),
        Operation::ListVideos => "/v2/query/video_generation".into(),
        Operation::RetrieveVideo | Operation::DownloadVideoContent => format!(
            "/v2/query/video_generation/{}",
            video::task_id(&request.path)?
        ),
        Operation::DeleteVideo => {
            format!("/v2/video_generation/{}", video::task_id(&request.path)?)
        }
        _ => return Err(ChannelError::UnsupportedOperation(ctx.operation)),
    };
    let base = ctx
        .provider
        .base_url
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/');
    let mut url = ctx
        .endpoint_override
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{base}{path}"));
    if ctx.operation.operation == Operation::ListVideos {
        request.query = request.query.map(|query| {
            query
                .split('&')
                .map(|part| {
                    part.strip_prefix("limit=")
                        .map(|value| format!("page_size={value}"))
                        .unwrap_or_else(|| part.to_owned())
                })
                .collect::<Vec<_>>()
                .join("&")
        });
        if request.query.as_deref().is_some_and(|query| {
            query
                .split('&')
                .any(|part| matches!(part.split('=').next(), Some("after" | "order")))
        }) {
            return Err(ChannelError::InvalidConfig(
                "MiniMax lists videos with page_num/page_size; after/order are unsupported".into(),
            ));
        }
    }
    if let Some(query) = request
        .query
        .as_deref()
        .map(strip_query_auth)
        .filter(|q| !q.is_empty())
    {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&query);
    }
    let allowlist = HeaderAllowlist::from_view_for(
        ctx.provider,
        crate::channel::ChannelHeaders::native(ctx.operation.dialect),
    )?;
    let mut headers = forwardable(&request.headers, allowlist.as_ref(), &[]);
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    if ctx.operation.operation == Operation::CreateVideo {
        let HttpBody::Bytes(body) = request.body else {
            return Err(ChannelError::InvalidConfig(
                "MiniMax video requests require buffered JSON".into(),
            ));
        };
        request.body = HttpBody::Bytes(video::request(&body)?.into());
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.remove(header::CONTENT_LENGTH);
    }
    let mut builder = http::Request::builder().method(request.method).uri(url);
    *builder.headers_mut().expect("fresh builder") = headers;
    builder
        .body(request.body)
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
}
