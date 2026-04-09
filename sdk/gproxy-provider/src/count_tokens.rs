use crate::engine::Usage;
use crate::response::UpstreamError;
use gproxy_protocol::kinds::ProtocolKind;

use std::sync::{Arc, OnceLock};
use tiktoken_rs::{CoreBPE, get_bpe_from_model, o200k_base};
use tokenizers::Tokenizer;

/// Token counting strategy, tried in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountStrategy {
    /// Call upstream count_tokens API.
    UpstreamApi,
    /// Use local tokenizer (tiktoken for GPT models, DeepSeek/HF for others).
    Local,
}

/// Result of a token count operation.
#[derive(Debug, Clone)]
pub struct TokenCount {
    pub count: i64,
    pub method: CountMethod,
}

/// How the token count was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountMethod {
    UpstreamApi,
    LocalTiktoken,
    LocalDeepSeek,
}

const DEEPSEEK_TOKENIZER_BYTES: &[u8] = include_bytes!("tokenizers/deepseek_tokenizer.json");

/// Count tokens locally. Tries tiktoken first (GPT models), falls back to
/// bundled DeepSeek tokenizer (covers all other models).
/// Always succeeds — DeepSeek tokenizer is the universal fallback.
pub fn count_tokens_local(model: &str, text: &str) -> TokenCount {
    if is_gpt_model(model)
        && let Ok(count) = count_tiktoken(model, text)
    {
        return TokenCount {
            count,
            method: CountMethod::LocalTiktoken,
        };
    }

    let count = count_deepseek(text);
    TokenCount {
        count,
        method: CountMethod::LocalDeepSeek,
    }
}

/// Estimate output tokens from partially received streaming body.
/// Used when stream is interrupted and no usage was reported.
pub fn estimate_partial_usage(
    input_tokens: Option<i64>,
    partial_output: &str,
    model: &str,
) -> Usage {
    let tc = count_tokens_local(model, partial_output);
    Usage {
        input_tokens,
        output_tokens: Some(tc.count),
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    }
}

pub fn local_count_response_for_protocol(
    protocol: ProtocolKind,
    body: &[u8],
) -> Result<Vec<u8>, UpstreamError> {
    let request = openai_count_request_from_protocol(protocol, body)?;
    let text = openai_count_request_text(&request.body);
    let model = request.body.model.as_deref().unwrap_or_default();
    let input_tokens = u64::try_from(count_tokens_local(model, &text).count).unwrap_or(0);

    match protocol {
        ProtocolKind::OpenAi => serde_json::to_vec(
            &gproxy_protocol::openai::count_tokens::response::ResponseBody {
                input_tokens,
                object: gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensObject::ResponseInputTokens,
            },
        )
        .map_err(|e| UpstreamError::Channel(format!("serialize local openai count response: {e}"))),
        ProtocolKind::Claude => serde_json::to_vec(
            &gproxy_protocol::claude::count_tokens::types::BetaMessageTokensCount {
                context_management: None,
                input_tokens,
            },
        )
        .map_err(|e| UpstreamError::Channel(format!("serialize local claude count response: {e}"))),
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => serde_json::to_vec(
            &gproxy_protocol::gemini::count_tokens::response::ResponseBody {
                total_tokens: input_tokens,
                cached_content_token_count: None,
                prompt_tokens_details: None,
                cache_tokens_details: None,
            },
        )
        .map_err(|e| UpstreamError::Channel(format!("serialize local gemini count response: {e}"))),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported local count protocol: {protocol}"
        ))),
    }
}

fn openai_count_request_from_protocol(
    protocol: ProtocolKind,
    body: &[u8],
) -> Result<gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest, UpstreamError>
{
    match protocol {
        ProtocolKind::OpenAi => {
            let body = serde_json::from_slice::<
                gproxy_protocol::openai::count_tokens::request::RequestBody,
            >(body)
            .map_err(|e| UpstreamError::Channel(format!("deserialize openai count body: {e}")))?;
            Ok(
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest {
                    body,
                    ..Default::default()
                },
            )
        }
        ProtocolKind::Claude => {
            let body = serde_json::from_slice::<
                gproxy_protocol::claude::count_tokens::request::RequestBody,
            >(body)
            .map_err(|e| UpstreamError::Channel(format!("deserialize claude count body: {e}")))?;
            let request =
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest {
                    body,
                    ..Default::default()
                };
            gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest::try_from(
                request,
            )
            .map_err(|e| UpstreamError::Channel(format!("transform claude count request: {e}")))
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            let body = serde_json::from_slice::<
                gproxy_protocol::gemini::count_tokens::request::RequestBody,
            >(body)
            .map_err(|e| UpstreamError::Channel(format!("deserialize gemini count body: {e}")))?;
            let request =
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest {
                    body,
                    ..Default::default()
                };
            gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest::try_from(
                request,
            )
            .map_err(|e| UpstreamError::Channel(format!("transform gemini count request: {e}")))
        }
        _ => Err(UpstreamError::Channel(format!(
            "unsupported local count protocol: {protocol}"
        ))),
    }
}

fn openai_count_request_text(
    body: &gproxy_protocol::openai::count_tokens::request::RequestBody,
) -> String {
    use gproxy_protocol::openai::count_tokens::types as ot;
    use gproxy_protocol::transform::openai::count_tokens::utils::{
        openai_function_call_output_content_to_text, openai_input_to_items,
        openai_message_content_to_text, openai_reasoning_summary_to_text,
    };

    let mut chunks = Vec::new();

    if let Some(instructions) = body.instructions.as_ref().filter(|text| !text.is_empty()) {
        chunks.push(instructions.clone());
    }

    for item in openai_input_to_items(body.input.clone()) {
        match item {
            ot::ResponseInputItem::Message(message) => {
                let text = openai_message_content_to_text(&message.content);
                if !text.is_empty() {
                    chunks.push(text);
                }
            }
            ot::ResponseInputItem::OutputMessage(message) => {
                let text = message
                    .content
                    .into_iter()
                    .map(|part| match part {
                        ot::ResponseOutputContent::Text(text) => text.text,
                        ot::ResponseOutputContent::Refusal(refusal) => refusal.refusal,
                    })
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    chunks.push(text);
                }
            }
            ot::ResponseInputItem::FunctionToolCall(call) => {
                chunks.push(format!("{} {}", call.name, call.arguments));
            }
            ot::ResponseInputItem::FunctionCallOutput(output) => {
                let text = openai_function_call_output_content_to_text(&output.output);
                if !text.is_empty() {
                    chunks.push(text);
                }
            }
            ot::ResponseInputItem::ReasoningItem(reasoning) => {
                let text = openai_reasoning_summary_to_text(&reasoning.summary);
                if !text.is_empty() {
                    chunks.push(text);
                }
            }
            ot::ResponseInputItem::CustomToolCall(call) => {
                chunks.push(format!("{} {}", call.name, call.input));
            }
            other => chunks.push(format!("{other:?}")),
        }
    }

    chunks.join("\n")
}

// === Tiktoken (GPT models) ===

fn is_gpt_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.starts_with("gpt")
        || m.starts_with("chatgpt")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        || m.starts_with("ft:gpt")
        || m.contains("gpt-")
}

fn count_tiktoken(model: &str, text: &str) -> Result<i64, String> {
    let bpe = build_bpe(model)?;
    Ok(bpe.encode_ordinary(text).len() as i64)
}

fn build_bpe(model: &str) -> Result<CoreBPE, String> {
    if let Ok(bpe) = get_bpe_from_model(model) {
        return Ok(bpe);
    }
    o200k_base().map_err(|e| e.to_string())
}

// === DeepSeek tokenizer (universal fallback) ===

fn deepseek_tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| {
        let tokenizer = Tokenizer::from_bytes(DEEPSEEK_TOKENIZER_BYTES)
            .expect("bundled DeepSeek tokenizer must be valid");
        Arc::new(tokenizer)
    })
}

fn count_deepseek(text: &str) -> i64 {
    let tokenizer = deepseek_tokenizer();
    match tokenizer.encode(text, false) {
        Ok(encoding) => encoding.len() as i64,
        Err(_) => {
            // Should never fail with valid UTF-8, but just in case
            (text.len() as i64 + 2) / 3
        }
    }
}
