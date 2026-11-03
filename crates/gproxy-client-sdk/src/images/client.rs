use super::*;
use crate::media_support::io;
impl<B: Backend> Client<B> {
    pub async fn generate_images(&self, request: &ImageRequest) -> Result<ImageResult> {
        request.target()?;
        match request.model.dialect {
            ImageDialect::Openai => {
                let rendered = openai::render(request, false)?;
                let (operation, wire) = rendered.request.wire()?;
                let mut warnings = rendered.warnings;
                let format = openai::output_format(request.output_format.as_ref(), &mut Vec::new());
                let response = self
                    .send(
                        OperationKey {
                            operation,
                            dialect: protocol::Dialect::OpenAi,
                        },
                        Some(&request.model.id),
                        wire,
                    )
                    .await?;
                io::read(response, self.limits, |body, head| {
                    let native = io::parse(std::str::from_utf8(body).map_err(Error::invalid)?)?;
                    let mut result = openai::parse(native, format)?;
                    result.response.merge(head.metadata());
                    warnings.extend(result.warnings);
                    result.warnings = warnings;
                    Ok(result)
                })
                .await
            }
            ImageDialect::Gemini => {
                let mut result = self.one_gemini_image(request).await?;
                if request.n.get() == 1 {
                    return Ok(result);
                }
                let mut calls = vec![std::mem::take(&mut result.provider_metadata)];
                let mut responses = vec![result.response.clone()];
                let mut usages = vec![result.usage.clone()];
                for _ in 1..request.n.get() {
                    let next = self.one_gemini_image(request).await?;
                    calls.push(next.provider_metadata);
                    responses.push(next.response.clone());
                    usages.push(next.usage.clone());
                    result.images.extend(next.images);
                    result.text.extend(next.text);
                    result.usage.add(next.usage)?;
                    for warning in next.warnings {
                        if !result.warnings.contains(&warning) {
                            result.warnings.push(warning);
                        }
                    }
                    if next.finish_reason.unified != FinishReasonKind::Stop {
                        result.finish_reason = next.finish_reason;
                    }
                    result.response.merge(next.response);
                }
                if request.n.get() > 1 {
                    #[derive(Serialize)]
                    struct Batch {
                        calls: Vec<ProviderMetadata>,
                        responses: Vec<ResponseMetadata>,
                        usages: Vec<OperationUsage>,
                    }
                    result.provider_metadata = [(
                        Vendor::Gemini,
                        crate::media_support::metadata(Batch {
                            calls,
                            responses,
                            usages,
                        })?,
                    )]
                    .into();
                }
                Ok(result)
            }
            ImageDialect::Unknown(_) => Err(Error::invalid("unsupported image dialect")),
        }
    }
    async fn one_gemini_image(&self, request: &ImageRequest) -> Result<ImageResult> {
        let rendered = gemini::render(request)?;
        let wire = crate::media_support::gemini::wire(&request.model.id, &rendered.request, false)?;
        let response = self
            .send(
                OperationKey {
                    operation: protocol::Operation::GenerateContent,
                    dialect: protocol::Dialect::Gemini,
                },
                Some(&request.model.id),
                wire,
            )
            .await?;
        io::read(response, self.limits, |body, _| {
            let mut result = gemini::parse(io::parse(
                std::str::from_utf8(body).map_err(Error::invalid)?,
            )?)?;
            let mut warnings = rendered.warnings;
            warnings.extend(result.warnings);
            result.warnings = warnings;
            Ok(result)
        })
        .await
    }
    pub async fn generate_images_prepared(
        &self,
        prepared: PreparedImageRequest,
    ) -> Result<ImageResult> {
        let mut result = self.generate_images(&prepared.request).await?;
        let mut warnings = prepared.warnings;
        warnings.extend(result.warnings);
        result.warnings = warnings;
        Ok(result)
    }
}
