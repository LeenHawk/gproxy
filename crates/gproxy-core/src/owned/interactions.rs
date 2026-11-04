//! Interactions are account resources, including when creation returns SSE.
use super::*;
use futures_util::StreamExt;
use gproxy_protocol::codec::{SseDecoder, SseFrame};

/// A create can continue an existing interaction; CRUD addresses it by path.
pub(super) fn reference(operation: OperationKey, body: &[u8]) -> Option<String> {
    if operation.dialect != Dialect::GeminiInteractions
        || !matches!(
            operation.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
    {
        return None;
    }
    serde_json::from_slice::<Value>(body)
        .ok()?
        .get("previous_interaction_id")?
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

fn event_id(frames: Vec<SseFrame>) -> Option<String> {
    frames.into_iter().find_map(|frame| {
        let SseFrame::Event(event) = frame else {
            return None;
        };
        let value: Value = serde_json::from_str(&event.data).ok()?;
        // Both created and terminal events carry the resource envelope.
        value
            .get("interaction")?
            .get("id")?
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    })
}

/// Read only as far as the resource id, then replay the original bytes. This
/// registers the resource before exposing its id without buffering generation.
pub(super) async fn created_stream_id(
    response: &mut WireResponse<HttpBody>,
    limits: CodecLimits,
) -> CoreResult<Option<String>> {
    let mut decoder = SseDecoder::new(limits);
    let decode = |error: gproxy_protocol::codec::CodecError| {
        TransformError::invalid_result("interaction.stream", error.to_string())
    };
    if let HttpBody::Bytes(bytes) = &response.body {
        return Ok(event_id(decoder.push(bytes).map_err(decode)?));
    }
    let HttpBody::Stream(mut stream) =
        std::mem::replace(&mut response.body, HttpBody::Bytes(Bytes::new()))
    else {
        unreachable!()
    };
    let mut prefix = Vec::new();
    let mut total = 0u64;
    let mut id = None;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|error| {
            TransformError::invalid_result("interaction.stream", error.to_string())
        })?;
        total = total.saturating_add(bytes.len() as u64);
        if total > limits.max_buffer_bytes {
            return Err(TransformError::invalid_result(
                "interaction.stream",
                "no interaction id within the stream prefix limit",
            )
            .into());
        }
        let frames = decoder.push(&bytes).map_err(decode)?;
        let failed = frames.iter().any(|frame| matches!(frame, SseFrame::Event(event) if event.event.as_deref() == Some("error") || serde_json::from_str::<Value>(&event.data).is_ok_and(|value| value["event_type"] == "error")));
        id = event_id(frames);
        prefix.push(Ok(bytes));
        if id.is_some() || failed {
            break;
        }
    }
    response.body = HttpBody::Stream(Box::pin(futures_util::stream::iter(prefix).chain(stream)));
    Ok(id)
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
    /// Recover a read/cancel/delete's original target before admission so model
    /// permissions apply to the same catalog name used for creation.
    pub async fn interaction_request_model(
        &self,
        scope: &str,
        operation: Operation,
        path: &str,
        provider: Option<&str>,
    ) -> CoreResult<Option<(String, Option<String>)>> {
        if !matches!(
            operation,
            Operation::RetrieveInteraction
                | Operation::CancelInteraction
                | Operation::DeleteInteraction
        ) {
            return Ok(None);
        }
        let id = path_id(OwnedKind::Interaction, path)?;
        let snapshot = self.snapshot();
        let columns = snapshot
            .providers
            .keys()
            .filter(|id| provider.is_none_or(|provider| provider == id.as_str()))
            .map(|provider| scope_column(scope, provider))
            .collect();
        let rows = self
            .owned_rows(columns, OwnedKind::Interaction, Some(&id), None)
            .await?;
        if rows.len() != 1 {
            return Err(CoreError::ResourceNotFound {
                kind: "interaction",
                id,
            });
        }
        let row = &rows[0];
        Ok(Some((
            row.provider_id.clone(),
            row.summary
                .get("requested_model")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )))
    }
}
