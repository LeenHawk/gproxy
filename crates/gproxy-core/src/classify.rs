use bytes::Bytes;
use gproxy_provider_core::ProxyRequest;
use gproxy_protocol::claude;
use gproxy_protocol::gemini;
use gproxy_protocol::openai;
use http::{HeaderMap, Method};
use serde::de::DeserializeOwned;

use crate::error::ProxyError;

#[derive(Debug)]
pub struct ProxyClassified {
    pub request: ProxyRequest,
    pub is_stream: bool,
}

pub fn classify_request(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProxyClassified, ProxyError> {
    let path = path.trim_start_matches('/');
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return Err(ProxyError::not_found("missing path"));
    }

    match segments.as_slice() {
        ["oauth"] => {
            ensure_method(method, Method::GET, "oauth")?;
            Ok(ProxyClassified {
                request: ProxyRequest::OAuthStart {
                    query: query.map(|q| q.to_string()),
                    headers: headers.clone(),
                },
                is_stream: false,
            })
        }
        ["oauth", "callback"] => {
            ensure_method(method, Method::GET, "oauth callback")?;
            Ok(ProxyClassified {
                request: ProxyRequest::OAuthCallback {
                    query: query.map(|q| q.to_string()),
                    headers: headers.clone(),
                },
                is_stream: false,
            })
        }
        ["usage"] => {
            ensure_method(method, Method::GET, "usage")?;
            Ok(ProxyClassified {
                request: ProxyRequest::Usage,
                is_stream: false,
            })
        }
        ["v1", "messages", ..] => classify_claude(method, &segments, query, headers, body),
        ["v1", "chat", "completions"] | ["v1", "responses", ..] => {
            classify_openai(method, &segments, body)
        }
        ["v1beta", ..] => classify_gemini(method, &segments, query, body),
        ["v1", "models", ..] => classify_models(method, &segments, query, headers, body),
        ["v1", ..] => classify_openai(method, &segments, body),
        _ => Err(ProxyError::not_found("unknown path")),
    }
}

fn classify_claude(
    method: &Method,
    segments: &[&str],
    query: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProxyClassified, ProxyError> {
    if segments.first().copied() != Some("v1") {
        return Err(ProxyError::not_found("unknown claude path"));
    }

    match segments {
        ["v1", "messages"] => {
            ensure_method(method, Method::POST, "claude messages")?;
            let body = parse_json::<claude::create_message::request::CreateMessageRequestBody>(
                &body,
                "claude messages",
            )?;
            let is_stream = body.stream.unwrap_or(false);
            let headers = parse_anthropic_headers::<claude::create_message::request::CreateMessageHeaders>(headers)?;
            let request = claude::create_message::request::CreateMessageRequest { headers, body };
            let request = if is_stream {
                ProxyRequest::ClaudeMessagesStream(request)
            } else {
                ProxyRequest::ClaudeMessages(request)
            };
            Ok(ProxyClassified { request, is_stream })
        }
        ["v1", "messages", "count_tokens"] => {
            ensure_method(method, Method::POST, "claude count tokens")?;
            let body = parse_json::<claude::count_tokens::request::CountTokensRequestBody>(
                &body,
                "claude count tokens",
            )?;
            let headers =
                parse_anthropic_headers::<claude::count_tokens::request::CountTokensHeaders>(
                    headers,
                )?;
            let request = claude::count_tokens::request::CountTokensRequest { headers, body };
            Ok(ProxyClassified {
                request: ProxyRequest::ClaudeCountTokens(request),
                is_stream: false,
            })
        }
        ["v1", "models"] => {
            ensure_method(method, Method::GET, "claude models list")?;
            let query =
                parse_query_or_default::<claude::list_models::request::ListModelsQuery>(query)?;
            let headers =
                parse_anthropic_headers::<claude::list_models::request::ListModelsHeaders>(headers)?;
            let request = claude::list_models::request::ListModelsRequest { query, headers };
            Ok(ProxyClassified {
                request: ProxyRequest::ClaudeModelsList(request),
                is_stream: false,
            })
        }
        ["v1", "models", model_id] => {
            ensure_method(method, Method::GET, "claude model get")?;
            let headers =
                parse_anthropic_headers::<claude::get_model::request::GetModelHeaders>(headers)?;
            let path = claude::get_model::request::GetModelPath {
                model_id: (*model_id).to_string(),
            };
            let request = claude::get_model::request::GetModelRequest { path, headers };
            Ok(ProxyClassified {
                request: ProxyRequest::ClaudeModelsGet(request),
                is_stream: false,
            })
        }
        _ => Err(ProxyError::not_found("unknown claude path")),
    }
}

fn classify_gemini(
    method: &Method,
    segments: &[&str],
    query: Option<&str>,
    body: Bytes,
) -> Result<ProxyClassified, ProxyError> {
    match segments.first().copied() {
        Some("v1") | Some("v1beta") => {}
        _ => return Err(ProxyError::not_found("unknown gemini path")),
    }

    match segments {
        [_version_segment, "models"] => {
            ensure_method(method, Method::GET, "gemini models list")?;
            let query =
                parse_query_or_default::<gemini::list_models::request::ListModelsQuery>(query)?;
            let request = gemini::list_models::request::ListModelsRequest { query };
            Ok(ProxyClassified {
                request: ProxyRequest::GeminiModelsList(request),
                is_stream: false,
            })
        }
        [_version_segment, "models", rest @ ..] => {
            let joined = rest.join("/");
            let (model, action) = split_model_action(&joined);
            if let Some(action) = action {
                classify_gemini_action(method, model, action, body)
            } else {
                ensure_method(method, Method::GET, "gemini model get")?;
                let path = gemini::get_model::request::GetModelPath {
                    name: model.to_string(),
                };
                let request = gemini::get_model::request::GetModelRequest { path };
                Ok(ProxyClassified {
                    request: ProxyRequest::GeminiModelsGet(request),
                    is_stream: false,
                })
            }
        }
        _ => Err(ProxyError::not_found("unknown gemini path")),
    }
}

fn classify_models(
    method: &Method,
    segments: &[&str],
    query: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProxyClassified, ProxyError> {
    if let ["v1", "models", rest @ ..] = segments {
        let joined = rest.join("/");
        let (model, action) = split_model_action(&joined);
        if let Some(action) = action {
            return classify_gemini_action(method, model, action, body);
        }
    }
    match detect_models_protocol(headers, query) {
        ModelsProtocol::Claude => classify_claude(method, segments, query, headers, body),
        ModelsProtocol::Gemini => classify_gemini(method, segments, query, body),
        ModelsProtocol::OpenAI => classify_openai(method, segments, body),
    }
}

fn classify_gemini_action(
    method: &Method,
    model: &str,
    action: &str,
    body: Bytes,
) -> Result<ProxyClassified, ProxyError> {
    ensure_method(method, Method::POST, "gemini action")?;
    match action {
        "generateContent" => {
            let body = parse_json::<gemini::generate_content::request::GenerateContentRequestBody>(
                &body,
                "gemini generate",
            )?;
            let path = gemini::generate_content::request::GenerateContentPath {
                model: model.to_string(),
            };
            let request = gemini::generate_content::request::GenerateContentRequest { path, body };
            Ok(ProxyClassified {
                request: ProxyRequest::GeminiGenerate(request),
                is_stream: false,
            })
        }
        "streamGenerateContent" => {
            let body = parse_json::<gemini::generate_content::request::GenerateContentRequestBody>(
                &body,
                "gemini stream generate",
            )?;
            let path = gemini::generate_content::request::GenerateContentPath {
                model: model.to_string(),
            };
            let request = gemini::stream_content::request::StreamGenerateContentRequest {
                path,
                body,
            };
            Ok(ProxyClassified {
                request: ProxyRequest::GeminiGenerateStream(request),
                is_stream: true,
            })
        }
        "countTokens" => {
            let body = parse_json::<gemini::count_tokens::request::CountTokensRequestBody>(
                &body,
                "gemini count tokens",
            )?;
            let path = gemini::count_tokens::request::CountTokensPath {
                model: model.to_string(),
            };
            let request = gemini::count_tokens::request::CountTokensRequest { path, body };
            Ok(ProxyClassified {
                request: ProxyRequest::GeminiCountTokens(request),
                is_stream: false,
            })
        }
        _ => Err(ProxyError::not_found("unknown gemini action")),
    }
}

fn classify_openai(
    method: &Method,
    segments: &[&str],
    body: Bytes,
) -> Result<ProxyClassified, ProxyError> {
    if segments.first().copied() != Some("v1") {
        return Err(ProxyError::not_found("unknown openai path"));
    }

    match segments {
        ["v1", "chat", "completions"] => {
            ensure_method(method, Method::POST, "openai chat completions")?;
            let body = parse_json::<openai::create_chat_completions::request::CreateChatCompletionRequestBody>(
                &body,
                "openai chat",
            )?;
            let is_stream = body.stream.unwrap_or(false);
            let request = openai::create_chat_completions::request::CreateChatCompletionRequest {
                body,
            };
            let request = if is_stream {
                ProxyRequest::OpenAIChatStream(request)
            } else {
                ProxyRequest::OpenAIChat(request)
            };
            Ok(ProxyClassified { request, is_stream })
        }
        ["v1", "responses"] => {
            ensure_method(method, Method::POST, "openai responses")?;
            let body = parse_json::<openai::create_response::request::CreateResponseRequestBody>(
                &body,
                "openai responses",
            )?;
            let is_stream = body.stream.unwrap_or(false);
            let request = openai::create_response::request::CreateResponseRequest { body };
            let request = if is_stream {
                ProxyRequest::OpenAIResponsesStream(request)
            } else {
                ProxyRequest::OpenAIResponses(request)
            };
            Ok(ProxyClassified { request, is_stream })
        }
        ["v1", "responses", "input_tokens"] => {
            ensure_method(method, Method::POST, "openai input tokens")?;
            let body = parse_json::<openai::count_tokens::request::InputTokenCountRequestBody>(
                &body,
                "openai input tokens",
            )?;
            let request = openai::count_tokens::request::InputTokenCountRequest { body };
            Ok(ProxyClassified {
                request: ProxyRequest::OpenAIInputTokens(request),
                is_stream: false,
            })
        }
        ["v1", "models"] => {
            ensure_method(method, Method::GET, "openai models list")?;
            let request = openai::list_models::request::ListModelsRequest;
            Ok(ProxyClassified {
                request: ProxyRequest::OpenAIModelsList(request),
                is_stream: false,
            })
        }
        ["v1", "models", model] => {
            ensure_method(method, Method::GET, "openai model get")?;
            let path = openai::get_model::request::GetModelPath {
                model: (*model).to_string(),
            };
            let request = openai::get_model::request::GetModelRequest { path };
            Ok(ProxyClassified {
                request: ProxyRequest::OpenAIModelsGet(request),
                is_stream: false,
            })
        }
        _ => Err(ProxyError::not_found("unknown openai path")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelsProtocol {
    Claude,
    Gemini,
    OpenAI,
}

fn detect_models_protocol(headers: &HeaderMap, query: Option<&str>) -> ModelsProtocol {
    if header_present(headers, "anthropic-version") {
        return ModelsProtocol::Claude;
    }

    let query = query.unwrap_or("");
    if header_present(headers, "x-goog-api-key") || query.contains("key=") {
        return ModelsProtocol::Gemini;
    }

    ModelsProtocol::OpenAI
}

fn header_present(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some()
}

fn parse_query_or_default<T>(query: Option<&str>) -> Result<T, ProxyError>
where
    T: DeserializeOwned + Default,
{
    match query {
        Some(value) if !value.is_empty() => serde_qs::from_str(value)
            .map_err(|err| ProxyError::bad_request(format!("invalid query: {err}"))),
        _ => Ok(T::default()),
    }
}

fn parse_json<T>(body: &[u8], label: &str) -> Result<T, ProxyError>
where
    T: DeserializeOwned,
{
    if body.is_empty() {
        return Err(ProxyError::bad_request(format!(
            "missing body for {label}",
        )));
    }
    serde_json::from_slice(body)
        .map_err(|err| ProxyError::bad_request(format!("invalid json: {err}")))
}

fn ensure_method(method: &Method, expected: Method, label: &str) -> Result<(), ProxyError> {
    if *method == expected {
        Ok(())
    } else {
        Err(ProxyError::method_not_allowed(format!(
            "invalid method for {label}",
        )))
    }
}

fn parse_anthropic_headers<T>(headers: &HeaderMap) -> Result<T, ProxyError>
where
    T: Default + AnthropicHeaderSet,
{
    let mut output = T::default();
    if let Some(value) = header_value(headers, "anthropic-version") {
        output.set_version(parse_anthropic_version(value.as_str())?);
    }
    if let Some(value) = header_value(headers, "anthropic-beta") {
        output.set_beta(Some(parse_anthropic_beta(value.as_str())?));
    }
    Ok(output)
}

fn parse_anthropic_version(value: &str) -> Result<claude::types::AnthropicVersion, ProxyError> {
    let json = serde_json::to_string(value)
        .map_err(|err| ProxyError::bad_request(format!("invalid anthropic version: {err}")))?;
    serde_json::from_str(&json)
        .map_err(|err| ProxyError::bad_request(format!("invalid anthropic version: {err}")))
}

fn parse_anthropic_beta(value: &str) -> Result<claude::types::AnthropicBetaHeader, ProxyError> {
    let parts: Vec<&str> = value
        .split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(ProxyError::bad_request("invalid anthropic beta header"));
    }

    let json = if parts.len() == 1 {
        serde_json::to_string(parts[0])
    } else {
        serde_json::to_string(&parts)
    }
    .map_err(|err| ProxyError::bad_request(format!("invalid anthropic beta: {err}")))?;

    serde_json::from_str(&json)
        .map_err(|err| ProxyError::bad_request(format!("invalid anthropic beta: {err}")))
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn split_model_action(segment: &str) -> (&str, Option<&str>) {
    match segment.split_once(':') {
        Some((model, action)) => (model, Some(action)),
        None => (segment, None),
    }
}

trait AnthropicHeaderSet {
    fn set_version(&mut self, version: claude::types::AnthropicVersion);
    fn set_beta(&mut self, beta: Option<claude::types::AnthropicBetaHeader>);
}

impl AnthropicHeaderSet for claude::types::AnthropicHeaders {
    fn set_version(&mut self, version: claude::types::AnthropicVersion) {
        self.anthropic_version = version;
    }

    fn set_beta(&mut self, beta: Option<claude::types::AnthropicBetaHeader>) {
        self.anthropic_beta = beta;
    }
}
