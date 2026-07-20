//! Gemini CLI request shaping and Code Assist envelope construction.

use bytes::Bytes;

use super::{auth, models};
use crate::channel::envelope;
use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::shaping::{self, gemini_genconfig};
use crate::channel::{ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};

pub(super) fn prepare(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let access_token = auth::access_token(ctx.secret)?.to_string();
    let project_id = auth::project_id(ctx.secret)?;

    if models::is_list_models(&ctx.method, ctx.path) {
        let user_agent = auth::user_agent(ctx.upstream_model_id);
        let request =
            envelope::user_quota_request(auth::BASE_URL, &access_token, project_id, &user_agent)?
                .ok_or_else(|| ChannelError::Build("failed to build retrieveUserQuota".into()))?;
        return Ok(PreparedRequest::new(request));
    }

    let is_count = ctx.path.contains(":countTokens");
    let wrapped = if is_count {
        envelope::wrap_code_assist_count(&ctx.body)?
    } else {
        envelope::wrap_code_assist(
            &ctx.body,
            ctx.upstream_model_id,
            project_id,
            &envelope::random_user_prompt_id(),
        )?
    };

    let (verb, query) = if ctx.path.contains(":streamGenerateContent") {
        (":streamGenerateContent", Some("alt=sse"))
    } else if is_count {
        (":countTokens", None)
    } else {
        (":generateContent", None)
    };
    let path = format!("/v1internal{verb}");
    let uri = join_url(auth::BASE_URL, &path, query)?;
    let headers = allow_headers(ctx.headers, &[]);
    let mut request = build_request(ctx.method, uri, headers, Bytes::from(wrapped))?;
    auth::apply(&mut request, &access_token, ctx.upstream_model_id)?;
    Ok(PreparedRequest::new(request))
}

pub(super) fn shape(body: Bytes, _headers: &mut http::HeaderMap, _ctx: &ShapeCtx) -> Bytes {
    shaping::with_json_body(body, gemini_genconfig::strip)
}
