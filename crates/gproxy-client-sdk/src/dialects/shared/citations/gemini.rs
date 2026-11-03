use super::*;
#[cfg(feature = "gemini")]
pub(crate) fn gemini_citations(
    candidate: &gproxy_protocol::gemini::Candidate,
) -> Result<Vec<(usize, Citation)>> {
    use gproxy_protocol::gemini as g;
    let mut out = Vec::new();
    if let Some(grounding) = &candidate.grounding_metadata {
        let chunks = grounding.grounding_chunks.as_deref().unwrap_or_default();
        for support in grounding.grounding_supports.iter().flatten() {
            let part = support
                .segment
                .as_ref()
                .and_then(|v| v.part_index)
                .unwrap_or(0)
                .try_into()
                .map_err(Error::invalid)?;
            for index in support.grounding_chunk_indices.iter().flatten() {
                let index: usize = (*index).try_into().map_err(Error::invalid)?;
                let Some(chunk) = chunks.get(index) else {
                    return Err(Error::invalid(
                        "grounding support references a missing chunk",
                    ));
                };
                let source = if let Some(web) = &chunk.web {
                    web.uri
                        .as_ref()
                        .map(|url| (CitationSource::Url { url: url.clone() }, web.title.clone()))
                } else if let Some(doc) = &chunk.retrieved_context {
                    doc.uri.as_ref().map(|url| {
                        (
                            CitationSource::Document {
                                document: DocumentReference::Block {
                                    document: Box::new(ContentBlock::new(Document {
                                        source: DocumentSource::Asset {
                                            source: Asset::Url { url: url.clone() },
                                        },
                                        media_type: None,
                                        title: doc.title.clone(),
                                        context: None,
                                        citations: None,
                                        detail: Detail::Auto,
                                    })),
                                },
                                range: doc
                                    .page_number
                                    .and_then(|p| u32::try_from(p).ok())
                                    .and_then(|p| {
                                        p.checked_add(1)
                                            .map(|end| DocumentRange::Pages { start: p, end })
                                    }),
                            },
                            doc.title.clone(),
                        )
                    })
                } else {
                    None
                };
                if let Some((source, title)) = source {
                    let segment = support.segment.as_ref();
                    let range = match segment
                        .and_then(|s| s.start_index)
                        .zip(segment.and_then(|s| s.end_index))
                    {
                        Some((start, end)) => Some(TextRange {
                            start: count(start)?,
                            end: count(end)?,
                        }),
                        None => None,
                    };
                    let mut citation = Citation {
                        source,
                        title,
                        cited_text: segment.and_then(|s| s.text.clone()),
                        text_range: range,
                        provider_metadata: ProviderMetadata::new(),
                    };
                    #[derive(Serialize)]
                    struct Grounding<'a> {
                        chunk: &'a g::GroundingChunk,
                        support: &'a g::GroundingSupport,
                    }
                    store(&mut citation, Vendor::Gemini, Grounding { chunk, support })?;
                    out.push((part, citation));
                }
            }
        }
    }
    if let Some(metadata) = &candidate.citation_metadata {
        for source in metadata.citation_sources.iter().flatten() {
            if let Some(url) = &source.uri {
                let range = match source.start_index.zip(source.end_index) {
                    Some((start, end)) => Some(TextRange {
                        start: count(start)?,
                        end: count(end)?,
                    }),
                    None => None,
                };
                let mut citation = Citation {
                    source: CitationSource::Url { url: url.clone() },
                    title: None,
                    cited_text: None,
                    text_range: range,
                    provider_metadata: ProviderMetadata::new(),
                };
                store(&mut citation, Vendor::Gemini, source)?;
                out.push((0, citation));
            }
        }
    }
    Ok(out)
}
