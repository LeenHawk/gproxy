use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};
use http::Method;

pub(super) struct Target {
    pub method: Method,
    pub path: String,
    pub query: Option<&'static str>,
    pub endpoint: &'static str,
}

pub(super) fn target(ctx: &PrepareCtx<'_>) -> Result<Target, ChannelError> {
    let project = super::endpoint::project_id(ctx.secret)?;
    let location = super::endpoint::location(ctx.provider_settings, ctx.secret)?;
    let model = model_id(ctx.upstream_model);
    let key = ctx.key;
    if key == family(Operation::ListModels, WireFamily::Gemini) {
        return Ok(get(
            "/v1beta1/publishers/google/models".into(),
            "gemini_list_models",
        ));
    }
    if key == family(Operation::GetModel, WireFamily::Gemini) {
        return Ok(get(
            format!(
                "/v1beta1/publishers/google/models/{}",
                encoded_model(model)?
            ),
            "gemini_get_model",
        ));
    }
    let google = |verb: &str, endpoint, query| {
        post(
            format!(
                "/v1beta1/projects/{project}/locations/{location}/publishers/google/models/{}{verb}",
                encoded_model(model)?
            ),
            endpoint,
            query,
        )
    };
    if key == family(Operation::CountTokens, WireFamily::Gemini) {
        return google(":countTokens", "gemini_count_tokens", None);
    }
    if key == family(Operation::CreateEmbedding, WireFamily::Gemini)
        && !super::embeddings::uses_predict(ctx)
    {
        return google(":embedContent", "gemini_embeddings", None);
    }
    if super::embeddings::uses_predict(ctx) {
        return post(
            format!(
                "/v1/projects/{project}/locations/{location}/publishers/google/models/{}:predict",
                encoded_model(model)?
            ),
            "gemini_embeddings",
            None,
        );
    }
    if is_content(key, ContentGenerationKind::GeminiGenerateContent) {
        return if key.operation() == Operation::StreamGenerateContent {
            google(
                ":streamGenerateContent",
                "gemini_stream_generate_content",
                Some("alt=sse"),
            )
        } else {
            google(":generateContent", "gemini_generate_content", None)
        };
    }
    if is_content(key, ContentGenerationKind::OpenAiChat) {
        return post(
            format!(
                "/v1beta1/projects/{project}/locations/{location}/endpoints/openapi/chat/completions"
            ),
            "openai_chat_completions",
            None,
        );
    }
    if key == family(Operation::CreateImage, WireFamily::Gemini) {
        return post(
            format!(
                "/v1/projects/{project}/locations/{location}/publishers/google/models/{}:predict",
                encoded_model(model)?
            ),
            "image_generations",
            None,
        );
    }
    if key == family(Operation::CreateVideo, WireFamily::Gemini)
        || key == family(Operation::CreateVideo, WireFamily::OpenAi)
    {
        return post(
            format!(
                "/v1/projects/{project}/locations/{location}/publishers/google/models/{}:predictLongRunning",
                encoded_model(model)?
            ),
            "gemini_video_create",
            None,
        );
    }
    if key == family(Operation::RetrieveVideo, WireFamily::Gemini)
        || key == family(Operation::RetrieveVideo, WireFamily::OpenAi)
    {
        let operation = super::resource::request_operation(ctx.path)?;
        let poll_model = if model.is_empty() {
            super::resource::operation_model(&operation)?
        } else {
            model
        };
        return post(
            format!(
                "/v1/projects/{project}/locations/{location}/publishers/google/models/{}:fetchPredictOperation",
                encoded_model(poll_model)?
            ),
            "gemini_video_retrieve",
            None,
        );
    }
    if key == family(Operation::CountTokens, WireFamily::Claude) {
        return post(
            format!(
                "/v1/projects/{project}/locations/{location}/publishers/anthropic/models/count-tokens:rawPredict"
            ),
            "claude_count_tokens",
            None,
        );
    }
    if is_content(key, ContentGenerationKind::ClaudeMessages) {
        let verb = if key.operation() == Operation::StreamGenerateContent {
            ":streamRawPredict"
        } else {
            ":rawPredict"
        };
        return post(
            format!(
                "/v1/projects/{project}/locations/{location}/publishers/anthropic/models/{}{verb}",
                encoded_model(model)?
            ),
            "claude_messages",
            None,
        );
    }
    Err(ChannelError::Prepare(
        "operation is unsupported by Vertex".into(),
    ))
}

pub(super) fn is_claude(key: OperationKey) -> bool {
    key.kind() == OperationKind::Family(WireFamily::Claude)
        || matches!(
            key.kind(),
            OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        )
}

fn get(path: String, endpoint: &'static str) -> Target {
    Target {
        method: Method::GET,
        path,
        query: None,
        endpoint,
    }
}

fn post(
    path: String,
    endpoint: &'static str,
    query: Option<&'static str>,
) -> Result<Target, ChannelError> {
    Ok(Target {
        method: Method::POST,
        path,
        query,
        endpoint,
    })
}

fn is_content(key: OperationKey, kind: ContentGenerationKind) -> bool {
    key.kind() == OperationKind::ContentGeneration(kind)
        && matches!(
            key.operation(),
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
}

fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

pub(super) fn model_id(model: &str) -> &str {
    model
        .rsplit_once("/models/")
        .map(|(_, id)| id)
        .unwrap_or_else(|| model.strip_prefix("models/").unwrap_or(model))
}

fn encoded_model(model: &str) -> Result<String, ChannelError> {
    if model.is_empty() {
        Err(ChannelError::Prepare("Vertex request has no model".into()))
    } else {
        Ok(crate::shared::http::encode_component(model))
    }
}
