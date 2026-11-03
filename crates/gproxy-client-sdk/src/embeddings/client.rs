use super::*;
use protocol::{HttpBody, WireResponse};
impl<B: Backend> Client<B> {
    pub async fn embed_prepared(&self, prepared: PreparedEmbedRequest) -> Result<EmbedResult> {
        let mut result = self.embed(&prepared.request).await?;
        let mut warnings = prepared.warnings;
        warnings.extend(result.warnings);
        result.warnings = warnings;
        Ok(result)
    }
    pub async fn embed(&self, request: &EmbedRequest) -> Result<EmbedResult> {
        let mut warnings = Vec::new();
        let mut vectors = Vec::with_capacity(request.inputs.len());
        let mut tokens = Some(0u64);
        match &request.model.dialect {
            EmbeddingDialect::Openai => {
                openai::embed(self, request, &mut warnings, &mut vectors, &mut tokens).await?
            }
            EmbeddingDialect::Gemini => {
                gemini::embed(self, request, &mut warnings, &mut vectors, &mut tokens).await?
            }
            EmbeddingDialect::Unknown(_) => {
                return Err(Error::invalid("unknown embedding dialect"));
            }
        }
        Ok(EmbedResult {
            vectors,
            usage: Usage {
                input_tokens: tokens,
                ..Usage::default()
            },
            warnings,
        })
    }
}
pub(super) struct Batch {
    pub(super) vectors: Vec<Vec<f64>>,
    pub(super) input_tokens: Option<u64>,
    pub(super) warnings: Vec<Warning>,
}

pub(super) async fn read_batch<T: serde::de::DeserializeOwned>(
    response: WireResponse<HttpBody>,
    limits: protocol::codec::CodecLimits,
    parse: impl FnOnce(T) -> Result<Batch>,
) -> Result<Batch> {
    let status = response.status.as_u16();
    let headers = response.headers;
    let bytes = protocol::codec::read_http_body(response.body, limits)
        .await
        .map_err(|error| Error::from(error).with_head(status, &headers))?;
    serde_json::from_slice::<T>(&bytes)
        .map_err(Error::invalid)
        .and_then(parse)
        .map_err(|error| error.with_head(status, &headers).with_body(&bytes))
}

pub(super) fn numbers(values: Vec<serde_json::Number>) -> Result<Vec<f64>> {
    values
        .into_iter()
        .map(|n| {
            n.as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| Error::invalid("embedding value is not a finite number"))
        })
        .collect()
}

pub(super) fn add_tokens(left: Option<u64>, right: Option<u64>) -> Result<Option<u64>> {
    left.zip(right)
        .map(|(a, b)| {
            a.checked_add(b)
                .ok_or_else(|| Error::invalid("embedding token usage overflow"))
        })
        .transpose()
}
