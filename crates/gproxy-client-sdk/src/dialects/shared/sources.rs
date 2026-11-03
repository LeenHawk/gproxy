//! Retrieved sources are independent of output-text citation anchors.
use crate::*;
fn source(
    source: CitationSource,
    title: Option<String>,
    text: Option<String>,
    vendor: Vendor,
    rest: &protocol::Rest,
) -> Source {
    Source {
        source: Some(source),
        title,
        text,
        provider_metadata: if rest.is_empty() {
            ProviderMetadata::new()
        } else {
            [(vendor, rest.clone())].into()
        },
    }
}
fn url(
    url: String,
    title: Option<String>,
    text: Option<String>,
    vendor: Vendor,
    rest: &protocol::Rest,
) -> Source {
    source(CitationSource::Url { url }, title, text, vendor, rest)
}
#[cfg(feature = "claude")]
pub(crate) fn claude_block(
    block: &protocol::claude::generate_content::ResponseContentBlock,
) -> Result<Vec<Source>> {
    use protocol::claude::generate_content as c;
    Ok(match block {
        c::ResponseContentBlock::WebSearchToolResult(result) => match &result.content {
            c::ResponseWebSearchResultContent::Results(results) => results
                .iter()
                .map(|value| {
                    url(
                        value.url.clone(),
                        Some(value.title.clone()),
                        None,
                        Vendor::Claude,
                        &value.rest,
                    )
                })
                .collect(),
            c::ResponseWebSearchResultContent::Error(_) => Vec::new(),
        },
        c::ResponseContentBlock::WebFetchToolResult(result) => match &result.content {
            c::ResponseWebFetchResultContent::Result(value) => vec![url(
                value.url.clone(),
                value.content.title.clone().flatten(),
                None,
                Vendor::Claude,
                &value.rest,
            )],
            c::ResponseWebFetchResultContent::Error(_) => Vec::new(),
        },
        _ => Vec::new(),
    })
}
#[cfg(feature = "openai")]
pub(crate) fn openai_item(
    item: &protocol::openai::responses::response::ResponseOutputItem,
) -> Result<Vec<Source>> {
    use protocol::openai::responses::{input as i, response as r};
    Ok(match item {
        r::ResponseOutputItem::WebSearchCall(value) => match &value.action {
            Some(i::WebSearchAction::Search(query)) => query
                .sources
                .iter()
                .flatten()
                .map(|value| url(value.url.clone(), None, None, Vendor::Openai, &value.rest))
                .collect(),
            Some(i::WebSearchAction::OpenPage(value)) => value
                .url
                .iter()
                .flatten()
                .map(|uri| url(uri.clone(), None, None, Vendor::Openai, &value.rest))
                .collect(),
            Some(i::WebSearchAction::FindInPage(value)) => value
                .url
                .iter()
                .flatten()
                .map(|uri| url(uri.clone(), None, None, Vendor::Openai, &value.rest))
                .collect(),
            None => Vec::new(),
        },
        r::ResponseOutputItem::FileSearchCall(value) => value
            .results
            .iter()
            .flatten()
            .flatten()
            .map(|value| Source {
                source: value.file_id.as_ref().map(|id| CitationSource::Document {
                    document: DocumentReference::ProviderFile {
                        vendor: Vendor::Openai,
                        id: id.clone(),
                    },
                    range: None,
                }),
                title: value.filename.clone(),
                text: value.text.clone(),
                provider_metadata: if value.rest.is_empty() {
                    ProviderMetadata::new()
                } else {
                    [(Vendor::Openai, value.rest.clone())].into()
                },
            })
            .collect(),
        _ => Vec::new(),
    })
}
#[cfg(feature = "gemini")]
pub(crate) fn gemini_candidate(candidate: &protocol::gemini::Candidate) -> Result<Vec<Source>> {
    let mut sources = Vec::new();
    for chunk in candidate
        .grounding_metadata
        .iter()
        .flat_map(|m| m.grounding_chunks.iter().flatten())
    {
        if let Some(web) = &chunk.web
            && let Some(uri) = &web.uri
        {
            sources.push(url(
                uri.clone(),
                web.title.clone(),
                None,
                Vendor::Gemini,
                &web.rest,
            ));
        }
        if let Some(document) = &chunk.retrieved_context {
            let reference = if let Some(uri) = &document.uri {
                let range = document
                    .page_number
                    .map(|page| -> Result<DocumentRange> {
                        let start = u32::try_from(page).map_err(Error::invalid)?;
                        let end = start
                            .checked_add(1)
                            .ok_or_else(|| Error::invalid("source page range overflow"))?;
                        Ok(DocumentRange::Pages { start, end })
                    })
                    .transpose()?;
                Some(CitationSource::Document {
                    document: DocumentReference::Block {
                        document: Box::new(ContentBlock::new(Document {
                            source: DocumentSource::Asset {
                                source: Asset::Url { url: uri.clone() },
                            },
                            media_type: None,
                            title: None,
                            context: None,
                            citations: None,
                            detail: Detail::Auto,
                        })),
                    },
                    range,
                })
            } else {
                None
            };
            sources.push(Source {
                source: reference,
                title: document.title.clone(),
                text: document.text.clone(),
                provider_metadata: if document.rest.is_empty() {
                    ProviderMetadata::new()
                } else {
                    [(Vendor::Gemini, document.rest.clone())].into()
                },
            });
        }
        if let Some(image) = &chunk.image
            && let Some(uri) = image.source_uri.as_ref().or(image.image_uri.as_ref())
        {
            sources.push(url(
                uri.clone(),
                image.title.clone(),
                None,
                Vendor::Gemini,
                &image.rest,
            ));
        }
        if let Some(maps) = &chunk.maps
            && let Some(uri) = &maps.uri
        {
            sources.push(url(
                uri.clone(),
                maps.title.clone(),
                maps.text.clone(),
                Vendor::Gemini,
                &maps.rest,
            ));
        }
    }
    for citation in candidate
        .citation_metadata
        .iter()
        .flat_map(|m| m.citation_sources.iter().flatten())
    {
        if let Some(uri) = &citation.uri {
            let value = url(uri.clone(), None, None, Vendor::Gemini, &citation.rest);
            if !sources.contains(&value) {
                sources.push(value);
            }
        }
    }
    Ok(sources)
}

pub(crate) fn unique(values: Vec<Source>) -> Vec<Source> {
    let mut out = Vec::new();
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}
