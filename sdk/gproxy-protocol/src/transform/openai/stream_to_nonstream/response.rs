use std::collections::BTreeMap;

use http::StatusCode;

use crate::openai::create_response::response::OpenAiCreateResponseResponse;
use crate::openai::create_response::stream::{ResponseStreamErrorPayload, ResponseStreamEvent};
use crate::openai::create_response::types::{
    OpenAiApiError, OpenAiApiErrorResponse, ResponseOutputItem,
};
use crate::openai::types::OpenAiResponseHeaders;
use crate::transform::utils::TransformError;

impl TryFrom<Vec<ResponseStreamEvent>> for OpenAiCreateResponseResponse {
    type Error = TransformError;

    fn try_from(value: Vec<ResponseStreamEvent>) -> Result<Self, TransformError> {
        let mut latest_response = None;
        let mut stream_error = None::<ResponseStreamErrorPayload>;
        // `OutputItemDone` events are the source of truth for the final
        // response body's `output` array — the `response.completed`
        // snapshot returned by codex (and matching OpenAI's Responses
        // API spec) ships with `output: []`, and the per-item content
        // is only visible on the incremental `output_item.done` stream
        // events. Collect those in `output_index` order and inject
        // them into `latest_response.output` below.
        let mut output_items: BTreeMap<u64, ResponseOutputItem> = BTreeMap::new();

        for event in value {
            match event {
                ResponseStreamEvent::Created { response, .. }
                | ResponseStreamEvent::Queued { response, .. }
                | ResponseStreamEvent::InProgress { response, .. }
                | ResponseStreamEvent::Completed { response, .. }
                | ResponseStreamEvent::Incomplete { response, .. }
                | ResponseStreamEvent::Failed { response, .. } => {
                    latest_response = Some(response);
                }
                ResponseStreamEvent::OutputItemDone {
                    item, output_index, ..
                } => {
                    output_items.insert(output_index, item);
                }
                ResponseStreamEvent::Error { error, .. } => stream_error = Some(error),
                _ => {}
            }
        }

        if let Some(mut body) = latest_response {
            if body.output.is_empty() && !output_items.is_empty() {
                body.output = output_items.into_values().collect();
            }
            Ok(OpenAiCreateResponseResponse::Success {
                stats_code: StatusCode::OK,
                headers: OpenAiResponseHeaders::default(),
                body,
            })
        } else if let Some(error) = stream_error {
            Ok(OpenAiCreateResponseResponse::Error {
                stats_code: StatusCode::BAD_REQUEST,
                headers: OpenAiResponseHeaders::default(),
                body: OpenAiApiErrorResponse {
                    error: OpenAiApiError {
                        message: error.message,
                        type_: error.type_,
                        param: error.param,
                        code: error.code,
                    },
                },
            })
        } else {
            Err(TransformError::not_implemented(
                "cannot convert OpenAI response SSE stream body without response snapshots",
            ))
        }
    }
}
