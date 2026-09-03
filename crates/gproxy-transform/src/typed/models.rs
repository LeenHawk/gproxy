//! Typed List Models and Get Model pairs.

use gproxy_protocol::{claude, gemini, openai};

use crate::TransformError;

fn openai_list(models: Vec<openai::Model>) -> openai::ModelListResponse {
    crate::wire!(openai::ModelListResponse {
        data: models,
        object: openai::ListObjectType::List,
        rest: Default::default(),
    })
}

fn claude_list(
    models: Vec<claude::ModelInfo>,
) -> Result<claude::ListModelsResponse, TransformError> {
    Ok(crate::wire!(claude::ListModelsResponse {
        first_id: models
            .first()
            .map(|model| crate::models::common::wire_string(&model.id))
            .transpose()?,
        last_id: models
            .last()
            .map(|model| crate::models::common::wire_string(&model.id))
            .transpose()?,
        data: models,
        has_more: None,
        rest: Default::default(),
    }))
}

pub mod openai_to_claude {
    use super::*;

    pub fn list_request(_: openai::ListModelsRequest) -> claude::ListModelsQuery {
        crate::wire!(claude::ListModelsQuery {
            after_id: None,
            before_id: None,
            limit: None,
            rest: Default::default(),
        })
    }

    pub fn list_response(
        input: claude::ListModelsResponse,
    ) -> Result<openai::ModelListResponse, TransformError> {
        input
            .data
            .into_iter()
            .map(crate::models::common::claude_to_openai)
            .collect::<Result<Vec<_>, _>>()
            .map(openai_list)
    }

    pub fn get_request(
        input: openai::RetrieveModelRequest,
    ) -> Result<claude::RetrieveModelPath, TransformError> {
        Ok(crate::wire!(claude::RetrieveModelPath {
            model_id: crate::models::common::wire_string(&input.model)?.into(),
            rest: Default::default(),
        }))
    }

    pub fn get_response(input: claude::ModelInfo) -> Result<openai::Model, TransformError> {
        crate::models::common::claude_to_openai(input)
    }
}

pub mod claude_to_openai {
    use super::*;

    pub fn list_request(_: claude::ListModelsQuery) -> openai::ListModelsRequest {
        openai::ListModelsRequest::default()
    }

    pub fn list_response(
        input: openai::ModelListResponse,
    ) -> Result<claude::ListModelsResponse, TransformError> {
        claude_list(
            input
                .data
                .into_iter()
                .map(crate::models::common::openai_to_claude)
                .collect::<Result<_, _>>()?,
        )
    }

    pub fn get_request(
        input: claude::RetrieveModelPath,
    ) -> Result<openai::RetrieveModelRequest, TransformError> {
        Ok(crate::wire!(openai::RetrieveModelRequest {
            model: crate::models::common::wire_string(&input.model_id)?.into(),
            rest: Default::default(),
        }))
    }

    pub fn get_response(input: openai::Model) -> Result<claude::ModelInfo, TransformError> {
        crate::models::common::openai_to_claude(input)
    }
}

pub mod openai_to_gemini {
    use super::*;

    pub fn list_request(_: openai::ListModelsRequest) -> gemini::ListModelsRequest {
        gemini::ListModelsRequest::default()
    }

    pub fn list_response(input: gemini::ListModelsResponse) -> openai::ModelListResponse {
        openai_list(
            input
                .models
                .into_iter()
                .map(crate::models::gemini::gemini_to_openai)
                .collect(),
        )
    }

    pub fn get_request(
        input: openai::RetrieveModelRequest,
    ) -> Result<gemini::GetModelRequest, TransformError> {
        Ok(crate::wire!(gemini::GetModelRequest {
            name: Some(crate::models::common::wire_string(&input.model)?),
            rest: Default::default(),
        }))
    }

    pub fn get_response(input: gemini::Model) -> openai::Model {
        crate::models::gemini::gemini_to_openai(input)
    }
}

pub mod gemini_to_openai {
    use super::*;

    pub fn list_request(_: gemini::ListModelsRequest) -> openai::ListModelsRequest {
        openai::ListModelsRequest::default()
    }

    pub fn list_response(
        input: openai::ModelListResponse,
    ) -> Result<gemini::ListModelsResponse, TransformError> {
        Ok(crate::wire!(gemini::ListModelsResponse {
            models: input
                .data
                .into_iter()
                .map(crate::models::gemini::openai_to_gemini)
                .collect::<Result<_, _>>()?,
            next_page_token: None,
            rest: Default::default(),
        }))
    }

    pub fn get_request(input: gemini::GetModelRequest) -> openai::RetrieveModelRequest {
        crate::wire!(openai::RetrieveModelRequest {
            model: input.name.unwrap_or_default().into(),
            rest: Default::default(),
        })
    }

    pub fn get_response(input: openai::Model) -> Result<gemini::Model, TransformError> {
        crate::models::gemini::openai_to_gemini(input)
    }
}

pub mod claude_to_gemini {
    use super::*;

    pub fn list_request(input: claude::ListModelsQuery) -> gemini::ListModelsRequest {
        crate::wire!(gemini::ListModelsRequest {
            page_size: input
                .limit
                .map(|value| i32::try_from(value).unwrap_or(i32::MAX)),
            page_token: input.after_id,
            rest: Default::default(),
        })
    }

    pub fn list_response(
        input: gemini::ListModelsResponse,
    ) -> Result<claude::ListModelsResponse, TransformError> {
        let has_more = input.next_page_token.is_some();
        let last_id = input.next_page_token.clone();
        let models = input
            .models
            .into_iter()
            .map(crate::models::gemini::gemini_to_claude)
            .collect();
        let mut output = claude_list(models)?;
        output.has_more = Some(has_more);
        output.last_id = last_id.or(output.last_id);
        Ok(output)
    }

    pub fn get_request(
        input: claude::RetrieveModelPath,
    ) -> Result<gemini::GetModelRequest, TransformError> {
        Ok(crate::wire!(gemini::GetModelRequest {
            name: Some(crate::models::common::wire_string(&input.model_id)?),
            rest: Default::default(),
        }))
    }

    pub fn get_response(input: gemini::Model) -> claude::ModelInfo {
        crate::models::gemini::gemini_to_claude(input)
    }
}

pub mod gemini_to_claude {
    use super::*;

    pub fn list_request(input: gemini::ListModelsRequest) -> claude::ListModelsQuery {
        crate::wire!(claude::ListModelsQuery {
            after_id: input.page_token,
            before_id: None,
            limit: input.page_size.and_then(|value| u64::try_from(value).ok()),
            rest: Default::default(),
        })
    }

    pub fn list_response(
        input: claude::ListModelsResponse,
    ) -> Result<gemini::ListModelsResponse, TransformError> {
        Ok(crate::wire!(gemini::ListModelsResponse {
            models: input
                .data
                .into_iter()
                .map(crate::models::gemini::claude_to_gemini)
                .collect::<Result<_, _>>()?,
            next_page_token: input
                .has_more
                .unwrap_or(false)
                .then_some(input.last_id)
                .flatten(),
            rest: Default::default(),
        }))
    }

    pub fn get_request(input: gemini::GetModelRequest) -> claude::RetrieveModelPath {
        crate::wire!(claude::RetrieveModelPath {
            model_id: input.name.unwrap_or_default().into(),
            rest: Default::default(),
        })
    }

    pub fn get_response(input: claude::ModelInfo) -> Result<gemini::Model, TransformError> {
        crate::models::gemini::claude_to_gemini(input)
    }
}
