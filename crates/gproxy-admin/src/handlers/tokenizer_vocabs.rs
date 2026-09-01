use bytes::Bytes;
use http::{Response, StatusCode};

use crate::dto::{TokenizerDeleteRequest, TokenizerFetchRequest, TokenizerVocabDto};
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn list(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    let values = state
        .store()
        .tokenizer_vocabs()
        .await?
        .into_iter()
        .map(dto)
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &values)
}

pub(super) async fn fetch(state: &impl State, body: &Bytes) -> Result<Response<Bytes>, AdminError> {
    let request: TokenizerFetchRequest = util::parse(body)?;
    let value = state
        .fetch_tokenizer_vocab(request.name.trim(), request.repository.trim())
        .await?;
    response::json(StatusCode::CREATED, &value)
}

pub(super) fn progress(
    state: &impl State,
    parts: &http::request::Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query = util::query(parts);
    let name = util::value(&query, "name")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AdminError::BadRequest("tokenizer vocabulary name is required".into()))?;
    response::json(StatusCode::OK, &state.tokenizer_vocab_progress(name))
}

pub(super) async fn delete(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: TokenizerDeleteRequest = util::parse(body)?;
    state.delete_tokenizer_vocab(request.name.trim()).await?;
    Ok(response::empty(StatusCode::NO_CONTENT))
}

fn dto(value: gproxy_store::records::TokenizerVocabRecord) -> TokenizerVocabDto {
    TokenizerVocabDto {
        name: value.name,
        repository: value.repository,
        size_bytes: value.size_bytes,
        updated_at: value.updated_at,
    }
}
