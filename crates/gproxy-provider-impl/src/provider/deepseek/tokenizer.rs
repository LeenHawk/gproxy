use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use tokenizers::Tokenizer;

use gproxy_provider_core::{AttemptFailure, UpstreamPassthroughError};

pub async fn count_input_tokens(
    body: &gproxy_protocol::openai::count_tokens::request::InputTokenCountRequestBody,
    data_dir: Option<&str>,
) -> Result<i64, AttemptFailure> {
    let tokenizer = load_tokenizer(data_dir).await?;
    let mut value = serde_json::to_value(body).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    if let Some(map) = value.as_object_mut() {
        map.remove("model");
    }
    let text = serde_json::to_string(&value).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let encoding = tokenizer.encode(text, false).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    Ok(encoding.get_ids().len() as i64)
}

async fn load_tokenizer(data_dir: Option<&str>) -> Result<Arc<Tokenizer>, AttemptFailure> {
    let cache = tokenizer_cache();
    let key = tokenizer_key(data_dir);
    {
        let guard = cache.lock().map_err(|_| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable("tokenizer lock failed".to_string()),
            mark: None,
        })?;
        if let Some(tokenizer) = guard.get(&key) {
            return Ok(tokenizer.clone());
        }
    }

    let path = tokenizer_path(data_dir);
    let bytes = tokio::fs::read(&path).await.map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let tokenizer = Tokenizer::from_bytes(bytes.as_slice()).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let tokenizer = Arc::new(tokenizer);
    let mut guard = cache.lock().map_err(|_| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable("tokenizer lock failed".to_string()),
        mark: None,
    })?;
    guard.insert(key, tokenizer.clone());
    Ok(tokenizer)
}

fn tokenizer_cache() -> &'static Mutex<HashMap<String, Arc<Tokenizer>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Tokenizer>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn tokenizer_key(data_dir: Option<&str>) -> String {
    let base = data_dir
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("./data");
    base.to_string()
}

fn tokenizer_path(data_dir: Option<&str>) -> PathBuf {
    let base = data_dir
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "./data".to_string());
    Path::new(&base)
        .join("cache")
        .join("tokenizers")
        .join("deepseek")
        .join("tokenizer.json")
}
