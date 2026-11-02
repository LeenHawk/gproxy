//! Citations retain native-only anchors in their own vendor metadata.
use super::*;

fn metadata<T: DeserializeOwned>(citation: &Citation, vendor: Vendor) -> Result<Option<T>> {
    citation
        .provider_metadata
        .get(&vendor)
        .map(|rest| {
            serde_json::from_value::<Native<T>>(Value::Object(rest.clone()))
                .map(|v| v.native)
                .map_err(Error::invalid)
        })
        .transpose()
        .map(Option::flatten)
}
fn store<T: Serialize>(citation: &mut Citation, vendor: Vendor, value: T) -> Result<()> {
    citation.provider_metadata.insert(
        vendor,
        encode_rest(Native {
            native: Some(value),
        })?,
    );
    Ok(())
}
fn native_reference(
    vendor: Vendor,
    index: i64,
    file_id: Option<String>,
) -> Result<DocumentReference> {
    Ok(DocumentReference::Index {
        vendor,
        index: index.try_into().map_err(Error::invalid)?,
        file_id,
    })
}
fn uri(citation: &Citation) -> Option<&str> {
    match &citation.source {
        CitationSource::Url { url } => Some(url),
        CitationSource::Document {
            document: DocumentReference::Block { document },
            ..
        } => match &document.content.source {
            DocumentSource::Asset {
                source: Asset::Url { url },
            } => Some(url),
            _ => None,
        },
        _ => None,
    }
}
fn native_index(reference: &DocumentReference, vendor: Vendor) -> Option<u32> {
    match reference {
        DocumentReference::Index {
            vendor: owner,
            index,
            ..
        } if *owner == vendor => Some(*index),
        _ => None,
    }
}

#[cfg(feature = "claude")]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
enum ClaudeCitation {
    Char(gproxy_protocol::claude::generate_content::ResponseCharCitationOptions),
    Page(gproxy_protocol::claude::generate_content::ResponsePageCitationOptions),
    Chunk(gproxy_protocol::claude::generate_content::ResponseContentBlockCitationOptions),
    Web(gproxy_protocol::claude::generate_content::ResponseWebCitationOptions),
    Search(gproxy_protocol::claude::generate_content::ResponseSearchCitationOptions),
}
#[cfg(feature = "claude")]
pub(crate) fn parse_claude(
    value: gproxy_protocol::claude::generate_content::ResponseTextCitation,
) -> Result<Citation> {
    use gproxy_protocol::claude::generate_content as r;
    let (source, title, text, native) = match value {
        r::ResponseTextCitation::Char(mut v) => {
            let source = CitationSource::Document {
                document: native_reference(
                    Vendor::Claude,
                    v.document_index,
                    v.file_id.clone().flatten(),
                )?,
                range: Some(DocumentRange::Chars {
                    start: count(v.start_char_index)?,
                    end: count(v.end_char_index)?,
                }),
            };
            let title = v.document_title.clone().flatten();
            let text = std::mem::take(&mut v.cited_text);
            let mut m = r::ResponseCharCitationOptions::from(v);
            m.cited_text = None;
            m.document_index = None;
            m.start_char_index = None;
            m.end_char_index = None;
            m.document_title = m.document_title.map(|_| None);
            (source, title, text, ClaudeCitation::Char(m))
        }
        r::ResponseTextCitation::Page(mut v) => {
            let source = CitationSource::Document {
                document: native_reference(
                    Vendor::Claude,
                    v.document_index,
                    v.file_id.clone().flatten(),
                )?,
                range: Some(DocumentRange::Pages {
                    start: v.start_page_number.try_into().map_err(Error::invalid)?,
                    end: v.end_page_number.try_into().map_err(Error::invalid)?,
                }),
            };
            let title = v.document_title.clone().flatten();
            let text = std::mem::take(&mut v.cited_text);
            let mut m = r::ResponsePageCitationOptions::from(v);
            m.cited_text = None;
            m.document_index = None;
            m.start_page_number = None;
            m.end_page_number = None;
            m.document_title = m.document_title.map(|_| None);
            (source, title, text, ClaudeCitation::Page(m))
        }
        r::ResponseTextCitation::ContentBlock(mut v) => {
            let source = CitationSource::Document {
                document: native_reference(
                    Vendor::Claude,
                    v.document_index,
                    v.file_id.clone().flatten(),
                )?,
                range: Some(DocumentRange::Chunks {
                    start: v.start_block_index.try_into().map_err(Error::invalid)?,
                    end: v.end_block_index.try_into().map_err(Error::invalid)?,
                }),
            };
            let title = v.document_title.clone().flatten();
            let text = std::mem::take(&mut v.cited_text);
            let mut m = r::ResponseContentBlockCitationOptions::from(v);
            m.cited_text = None;
            m.document_index = None;
            m.start_block_index = None;
            m.end_block_index = None;
            m.document_title = m.document_title.map(|_| None);
            (source, title, text, ClaudeCitation::Chunk(m))
        }
        r::ResponseTextCitation::Web(mut v) => {
            let source = CitationSource::Url {
                url: std::mem::take(&mut v.url),
            };
            let title = v.title.clone().flatten();
            let text = std::mem::take(&mut v.cited_text);
            let mut m = r::ResponseWebCitationOptions::from(v);
            m.url = None;
            m.title = m.title.map(|_| None);
            m.cited_text = None;
            (source, title, text, ClaudeCitation::Web(m))
        }
        r::ResponseTextCitation::Search(mut v) => {
            let doc = Document {
                source: DocumentSource::Asset {
                    source: Asset::Url {
                        url: std::mem::take(&mut v.source),
                    },
                },
                media_type: None,
                title: v.title.clone().flatten(),
                context: None,
                citations: None,
                detail: Detail::Auto,
            };
            let source = CitationSource::Document {
                document: DocumentReference::Block {
                    document: Box::new(ContentBlock::new(doc)),
                },
                range: Some(DocumentRange::Chunks {
                    start: v.start_block_index.try_into().map_err(Error::invalid)?,
                    end: v.end_block_index.try_into().map_err(Error::invalid)?,
                }),
            };
            let title = v.title.clone().flatten();
            let text = std::mem::take(&mut v.cited_text);
            let mut m = r::ResponseSearchCitationOptions::from(v);
            m.source = None;
            m.title = m.title.map(|_| None);
            m.cited_text = None;
            m.start_block_index = None;
            m.end_block_index = None;
            (source, title, text, ClaudeCitation::Search(m))
        }
    };
    let mut citation = Citation {
        source,
        title,
        cited_text: Some(text),
        text_range: None,
        provider_metadata: ProviderMetadata::new(),
    };
    store(&mut citation, Vendor::Claude, native)?;
    Ok(citation)
}
#[cfg(feature = "claude")]
pub(crate) fn render_claude(
    citation: &Citation,
) -> Result<Option<gproxy_protocol::claude::generate_content::ResponseTextCitation>> {
    use gproxy_protocol::claude::generate_content as r;
    let Some(native) = metadata::<ClaudeCitation>(citation, Vendor::Claude)? else {
        return Ok(None);
    };
    let text = citation.cited_text.clone().unwrap_or_default();
    Ok(Some(match native {
        ClaudeCitation::Web(m) => {
            let Some(url) = uri(citation) else {
                return Ok(None);
            };
            let encrypted = m
                .encrypted_index
                .clone()
                .ok_or_else(|| Error::invalid("Claude web citation requires encrypted index"))?;
            let mut v = r::ResponseWebCitation::builder(
                r::ResponseWebCitationType::Tag,
                text,
                encrypted,
                url.into(),
            )
            .build();
            m.apply_to(&mut v);
            v.url = url.into();
            if let Some(title) = &citation.title {
                v.title = Some(Some(title.clone()));
            }
            r::ResponseTextCitation::Web(v)
        }
        ClaudeCitation::Char(m) => {
            let CitationSource::Document {
                document,
                range: Some(DocumentRange::Chars { start, end }),
            } = &citation.source
            else {
                return Ok(None);
            };
            let Some(index) = native_index(document, Vendor::Claude) else {
                return Ok(None);
            };
            let mut v = r::ResponseCharCitation::builder(
                r::ResponseCharCitationType::Tag,
                text,
                index.into(),
                (*end).try_into().map_err(Error::invalid)?,
                (*start).try_into().map_err(Error::invalid)?,
            )
            .build();
            m.apply_to(&mut v);
            if let Some(title) = &citation.title {
                v.document_title = Some(Some(title.clone()));
            }
            r::ResponseTextCitation::Char(v)
        }
        ClaudeCitation::Page(m) => {
            let CitationSource::Document {
                document,
                range: Some(DocumentRange::Pages { start, end }),
            } = &citation.source
            else {
                return Ok(None);
            };
            let Some(index) = native_index(document, Vendor::Claude) else {
                return Ok(None);
            };
            let mut v = r::ResponsePageCitation::builder(
                r::ResponsePageCitationType::Tag,
                text,
                index.into(),
                (*end).into(),
                (*start).into(),
            )
            .build();
            m.apply_to(&mut v);
            if let Some(title) = &citation.title {
                v.document_title = Some(Some(title.clone()));
            }
            r::ResponseTextCitation::Page(v)
        }
        ClaudeCitation::Chunk(m) => {
            let CitationSource::Document {
                document,
                range: Some(DocumentRange::Chunks { start, end }),
            } = &citation.source
            else {
                return Ok(None);
            };
            let Some(index) = native_index(document, Vendor::Claude) else {
                return Ok(None);
            };
            let mut v = r::ResponseContentBlockCitation::builder(
                r::ResponseContentBlockCitationType::Tag,
                text,
                index.into(),
                (*end).into(),
                (*start).into(),
            )
            .build();
            m.apply_to(&mut v);
            if let Some(title) = &citation.title {
                v.document_title = Some(Some(title.clone()));
            }
            r::ResponseTextCitation::ContentBlock(v)
        }
        ClaudeCitation::Search(m) => {
            let Some(url) = uri(citation) else {
                return Ok(None);
            };
            let CitationSource::Document {
                range: Some(DocumentRange::Chunks { start, end }),
                ..
            } = &citation.source
            else {
                return Ok(None);
            };
            let index = m
                .search_result_index
                .ok_or_else(|| Error::invalid("Claude search citation requires result index"))?;
            let mut v = r::ResponseSearchCitation::builder(
                r::ResponseSearchCitationType::Tag,
                text,
                (*end).into(),
                index,
                url.into(),
                (*start).into(),
            )
            .build();
            m.apply_to(&mut v);
            v.source = url.into();
            if let Some(title) = &citation.title {
                v.title = Some(Some(title.clone()));
            }
            r::ResponseTextCitation::Search(v)
        }
    }))
}

#[cfg(any(feature = "openai", feature = "chat"))]
#[derive(Serialize, Deserialize)]
struct UrlMetadata {
    #[serde(default)]
    rest: Rest,
    #[serde(default)]
    annotation_rest: Rest,
}
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
                citation.title.clone().unwrap_or_default(),
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
