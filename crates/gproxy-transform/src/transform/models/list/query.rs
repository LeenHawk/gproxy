//! List-models QUERY-string conversion (inbound wire → target wire).
//!
//! Models requests are GET/no-body — their parameters travel in the query, so
//! the bytes dispatch never sees them. The pipeline calls [`request_query`]
//! with the inbound query in [`TransformContext::query`] instead. Values are
//! kept percent-encoded verbatim (parse → typed request transform → re-emit).

use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformPair};

/// Convert the inbound ListModels query to the target wire's query string.
/// `None` = nothing to send (OpenAI's list endpoint takes no parameters, so
/// every pair into/out of OpenAI drops them). Non-models pairs and GetModel
/// are a no-op.
pub fn request_query(pair: TransformPair, ctx: &TransformContext) -> Option<String> {
    use TransformPair as P;
    if ctx.source.operation != crate::protocol::Operation::ListModels {
        return None;
    }
    match pair {
        P::ClaudeToGeminiModels => {
            write_gemini(super::claude_to_gemini::request(parse_claude(ctx), ctx))
        }
        P::GeminiToClaudeModels => {
            write_claude(super::gemini_to_claude::request(parse_gemini(ctx), ctx))
        }
        _ => None,
    }
}

fn pairs(ctx: &TransformContext) -> impl Iterator<Item = (&str, &str)> {
    ctx.query
        .as_deref()
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (!k.is_empty() && !v.is_empty()).then_some((k, v))
        })
}

fn parse_claude(ctx: &TransformContext) -> claude::ListModelsQuery {
    let mut q = claude::ListModelsQuery {
        after_id: None,
        before_id: None,
        limit: None,
        extra: Default::default(),
    };
    for (k, v) in pairs(ctx) {
        match k {
            "after_id" => q.after_id = Some(v.to_owned()),
            "before_id" => q.before_id = Some(v.to_owned()),
            "limit" => q.limit = v.parse().ok(),
            _ => {}
        }
    }
    q
}

fn parse_gemini(ctx: &TransformContext) -> gemini::ListModelsRequest {
    let mut q = gemini::ListModelsRequest::default();
    for (k, v) in pairs(ctx) {
        match k {
            "pageSize" => q.page_size = v.parse().ok(),
            "pageToken" => q.page_token = Some(v.to_owned()),
            _ => {}
        }
    }
    q
}

fn write_claude(q: claude::ListModelsQuery) -> Option<String> {
    let mut out = Vec::new();
    if let Some(limit) = q.limit {
        out.push(format!("limit={limit}"));
    }
    if let Some(after_id) = q.after_id {
        out.push(format!("after_id={after_id}"));
    }
    if let Some(before_id) = q.before_id {
        out.push(format!("before_id={before_id}"));
    }
    (!out.is_empty()).then(|| out.join("&"))
}

fn write_gemini(q: gemini::ListModelsRequest) -> Option<String> {
    let mut out = Vec::new();
    if let Some(page_size) = q.page_size {
        out.push(format!("pageSize={page_size}"));
    }
    if let Some(page_token) = q.page_token {
        out.push(format!("pageToken={page_token}"));
    }
    (!out.is_empty()).then(|| out.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Operation, OperationKey, Provider};

    fn ctx(source: Provider, target: Provider, query: &str) -> TransformContext {
        TransformContext::new(
            OperationKey::provider(Operation::ListModels, source),
            OperationKey::provider(Operation::ListModels, target),
        )
        .with_request("/v1/models", Some(query))
    }

    #[test]
    fn claude_and_gemini_pagination_round_trips() {
        let out = request_query(
            TransformPair::ClaudeToGeminiModels,
            &ctx(
                Provider::Claude,
                Provider::Gemini,
                "limit=25&after_id=m1&x=1",
            ),
        );
        assert_eq!(out.as_deref(), Some("pageSize=25&pageToken=m1"));

        let out = request_query(
            TransformPair::GeminiToClaudeModels,
            &ctx(
                Provider::Gemini,
                Provider::Claude,
                "pageSize=25&pageToken=tok",
            ),
        );
        assert_eq!(out.as_deref(), Some("limit=25&after_id=tok"));

        // openai targets take no list parameters
        let out = request_query(
            TransformPair::ClaudeToOpenAiModels,
            &ctx(Provider::Claude, Provider::OpenAi, "limit=25"),
        );
        assert_eq!(out, None);
    }
}
