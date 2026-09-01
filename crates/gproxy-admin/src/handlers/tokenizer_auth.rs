use bytes::Bytes;
use http::{Response, StatusCode};

use crate::dto::{TokenizerAuthDto, TokenizerAuthRevealResponse, TokenizerAuthUpdate};
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn get(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    response::json(
        StatusCode::OK,
        &TokenizerAuthDto {
            configured: state.tokenizer_auth().await?,
        },
    )
}

pub(super) async fn update(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: TokenizerAuthUpdate = util::parse(body)?;
    let token = request.token.as_deref().map(str::trim);
    if token == Some("") {
        return Err(AdminError::BadRequest(
            "Hugging Face token must not be blank".into(),
        ));
    }
    response::json(
        StatusCode::OK,
        &TokenizerAuthDto {
            configured: state.update_tokenizer_auth(token).await?,
        },
    )
}

pub(super) async fn reveal(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    response::json(
        StatusCode::OK,
        &TokenizerAuthRevealResponse {
            token: state.reveal_tokenizer_auth().await?,
        },
    )
}
