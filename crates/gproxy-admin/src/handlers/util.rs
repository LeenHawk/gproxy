use bytes::Bytes;
use http::{Response, StatusCode};
use serde::de::DeserializeOwned;

use crate::dto::{AppliedResponse, IdResponse};
use crate::{AdminError, State, response};

pub(super) fn parse<T: DeserializeOwned>(body: &Bytes) -> Result<T, AdminError> {
    serde_json::from_slice(body).map_err(|error| AdminError::BadRequest(error.to_string()))
}

pub(super) async fn created(state: &impl State, id: i64) -> Result<Response<Bytes>, AdminError> {
    state.reload().await?;
    response::json(StatusCode::CREATED, &IdResponse { id })
}

pub(super) async fn updated(
    state: &impl State,
    applied: bool,
) -> Result<Response<Bytes>, AdminError> {
    if !applied {
        return Err(AdminError::NotFound);
    }
    state.reload().await?;
    response::json(StatusCode::OK, &AppliedResponse { applied })
}

pub(super) fn query(parts: &http::request::Parts) -> Vec<(String, String)> {
    parts
        .uri
        .query()
        .map(|query| {
            form_urlencoded::parse(query.as_bytes())
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn value<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query
        .iter()
        .find_map(|(name, value)| (name == key).then_some(value.as_str()))
}

pub(super) fn parse_i64(value: Option<&str>, field: &str) -> Result<Option<i64>, AdminError> {
    value
        .map(|value| {
            value
                .parse()
                .map_err(|_| AdminError::BadRequest(format!("{field} must be an integer")))
        })
        .transpose()
}
