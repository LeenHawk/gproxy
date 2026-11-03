use super::*;
#[cfg(feature = "openai")]
pub(crate) fn parse_responses(
    value: gproxy_protocol::openai::responses::input::OutputAnnotation,
) -> Result<Citation> {
    use gproxy_protocol::openai::responses::input as i;
    match value {
        i::OutputAnnotation::Url(v) => {
            let mut citation = Citation {
                source: CitationSource::Url { url: v.url },
                title: Some(v.title),
                cited_text: None,
                text_range: Some(TextRange {
                    start: count(v.start_index)?,
                    end: count(v.end_index)?,
                }),
                provider_metadata: ProviderMetadata::new(),
            };
            store(
                &mut citation,
                Vendor::Openai,
                UrlMetadata {
                    rest: v.rest,
                    annotation_rest: Rest::new(),
                },
            )?;
            Ok(citation)
        }
        other => {
            let (id, title, range) = match &other {
                i::OutputAnnotation::File(v) => (v.file_id.clone(), Some(v.filename.clone()), None),
                i::OutputAnnotation::ContainerFile(v) => (
                    v.file_id.clone(),
                    Some(v.filename.clone()),
                    Some(TextRange {
                        start: count(v.start_index)?,
                        end: count(v.end_index)?,
                    }),
                ),
                i::OutputAnnotation::Path(v) => (v.file_id.clone(), None, None),
                i::OutputAnnotation::Url(_) => {
                    return Err(Error::invalid("unexpected URL citation"));
                }
            };
            let mut citation = Citation {
                source: CitationSource::Document {
                    document: DocumentReference::ProviderFile {
                        vendor: Vendor::Openai,
                        id,
                    },
                    range: None,
                },
                title,
                cited_text: None,
                text_range: range,
                provider_metadata: ProviderMetadata::new(),
            };
            store(&mut citation, Vendor::Openai, other)?;
            Ok(citation)
        }
    }
}

#[cfg(feature = "openai")]
pub(crate) fn render_responses(
    citation: &Citation,
) -> Result<Option<gproxy_protocol::openai::responses::input::OutputAnnotation>> {
    use gproxy_protocol::openai::responses::input as i;
    if let Some(url) = uri(citation) {
        let Some(range) = &citation.text_range else {
            return Ok(None);
        };
        let Some(title) = &citation.title else {
            return Ok(None);
        };
        let rest = metadata::<UrlMetadata>(citation, Vendor::Openai)?
            .unwrap_or(UrlMetadata {
                rest: Rest::new(),
                annotation_rest: Rest::new(),
            })
            .rest;
        return Ok(Some(i::OutputAnnotation::Url(
            i::UrlCitation::builder(
                range.end.try_into().map_err(Error::invalid)?,
                range.start.try_into().map_err(Error::invalid)?,
                title.clone(),
                url.into(),
                i::UrlCitationType::UrlCitation,
            )
            .rest(rest)
            .build(),
        )));
    }
    metadata(citation, Vendor::Openai)
}

#[cfg(feature = "chat")]
pub(crate) fn parse_chat(
    value: gproxy_protocol::openai::chat::response::Annotation,
) -> Result<Citation> {
    let v = value.url_citation;
    let mut citation = Citation {
        source: CitationSource::Url { url: v.url },
        title: Some(v.title),
        cited_text: None,
        text_range: Some(TextRange {
            start: count(v.start_index)?,
            end: count(v.end_index)?,
        }),
        provider_metadata: ProviderMetadata::new(),
    };
    store(
        &mut citation,
        Vendor::Openai,
        UrlMetadata {
            rest: v.rest,
            annotation_rest: value.rest,
        },
    )?;
    Ok(citation)
}
