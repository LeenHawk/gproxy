use super::*;
#[cfg(feature = "claude")]
pub(crate) fn parse_claude(
    value: gproxy_protocol::claude::generate_content::ResponseTextCitation,
) -> Result<Citation> {
    use gproxy_protocol::claude::generate_content::ResponseTextCitation as C;
    let (source, title, text, anchor, file_id_present, rest) = match value {
        C::Char(v) => (
            CitationSource::Document {
                document: native_reference(
                    Vendor::Claude,
                    v.document_index,
                    v.file_id.clone().flatten(),
                )?,
                range: Some(DocumentRange::Chars {
                    start: count(v.start_char_index)?,
                    end: count(v.end_char_index)?,
                }),
            },
            v.document_title.flatten(),
            v.cited_text,
            ClaudeAnchor::Char,
            v.file_id.is_some(),
            v.rest,
        ),
        C::Page(v) => (
            CitationSource::Document {
                document: native_reference(
                    Vendor::Claude,
                    v.document_index,
                    v.file_id.clone().flatten(),
                )?,
                range: Some(DocumentRange::Pages {
                    start: v.start_page_number.try_into().map_err(Error::invalid)?,
                    end: v.end_page_number.try_into().map_err(Error::invalid)?,
                }),
            },
            v.document_title.flatten(),
            v.cited_text,
            ClaudeAnchor::Page,
            v.file_id.is_some(),
            v.rest,
        ),
        C::ContentBlock(v) => (
            CitationSource::Document {
                document: native_reference(
                    Vendor::Claude,
                    v.document_index,
                    v.file_id.clone().flatten(),
                )?,
                range: Some(DocumentRange::Chunks {
                    start: v.start_block_index.try_into().map_err(Error::invalid)?,
                    end: v.end_block_index.try_into().map_err(Error::invalid)?,
                }),
            },
            v.document_title.flatten(),
            v.cited_text,
            ClaudeAnchor::Chunk,
            v.file_id.is_some(),
            v.rest,
        ),
        C::Web(v) => (
            CitationSource::Url { url: v.url },
            v.title.flatten(),
            v.cited_text,
            ClaudeAnchor::Web {
                encrypted_index: v.encrypted_index,
            },
            false,
            v.rest,
        ),
        C::Search(v) => {
            let title = v.title.flatten();
            let document = Document {
                source: DocumentSource::Asset {
                    source: Asset::Url { url: v.source },
                },
                media_type: None,
                title: None,
                context: None,
                citations: None,
                detail: Detail::Auto,
            };
            (
                CitationSource::Document {
                    document: DocumentReference::Block {
                        document: Box::new(ContentBlock::new(document)),
                    },
                    range: Some(DocumentRange::Chunks {
                        start: v.start_block_index.try_into().map_err(Error::invalid)?,
                        end: v.end_block_index.try_into().map_err(Error::invalid)?,
                    }),
                },
                title,
                v.cited_text,
                ClaudeAnchor::Search {
                    search_result_index: v.search_result_index,
                },
                false,
                v.rest,
            )
        }
    };
    let mut citation = Citation {
        source,
        title,
        cited_text: Some(text),
        text_range: None,
        provider_metadata: ProviderMetadata::new(),
    };
    store(
        &mut citation,
        Vendor::Claude,
        ClaudeCitation {
            anchor,
            file_id_present,
            rest,
        },
    )?;
    Ok(citation)
}

#[cfg(feature = "claude")]
pub(crate) fn render_claude(
    citation: &Citation,
) -> Result<Option<gproxy_protocol::claude::content::TextCitation>> {
    use gproxy_protocol::claude::content as c;
    let Some(meta) = metadata::<ClaudeCitation>(citation, Vendor::Claude)? else {
        return Ok(None);
    };
    let text = citation
        .cited_text
        .clone()
        .ok_or_else(|| Error::invalid("Claude citation requires cited_text"))?;
    let mut rest = meta.rest;
    if meta.file_id_present {
        let id = match &citation.source {
            CitationSource::Document {
                document: DocumentReference::Index { file_id, .. },
                ..
            } => file_id.clone(),
            _ => None,
        };
        // file_id is an extension on request citations, whose native Rest is
        // explicitly the transport for fields outside the parameter schema.
        #[derive(Serialize)]
        struct FileId {
            file_id: Option<String>,
        }
        rest.extend(encode_rest(FileId { file_id: id })?);
    }
    let value =
        match meta.anchor {
            ClaudeAnchor::Char | ClaudeAnchor::Page | ClaudeAnchor::Chunk => {
                let CitationSource::Document {
                    document,
                    range: Some(range),
                } = &citation.source
                else {
                    return Ok(None);
                };
                let Some(index) = native_index(document, Vendor::Claude) else {
                    return Ok(None);
                };
                match (meta.anchor, range) {
                    (ClaudeAnchor::Char, DocumentRange::Chars { start, end }) => {
                        let mut v = c::CharCitation::builder(
                            text,
                            index.into(),
                            (*end).try_into().map_err(Error::invalid)?,
                            (*start).try_into().map_err(Error::invalid)?,
                        )
                        .rest(rest)
                        .build();
                        v.document_title = citation.title.clone();
                        c::TextCitation::Char(v)
                    }
                    (ClaudeAnchor::Page, DocumentRange::Pages { start, end }) => {
                        let mut v = c::PageCitation::builder(
                            text,
                            index.into(),
                            (*end).into(),
                            (*start).into(),
                        )
                        .rest(rest)
                        .build();
                        v.document_title = citation.title.clone();
                        c::TextCitation::Page(v)
                    }
                    (ClaudeAnchor::Chunk, DocumentRange::Chunks { start, end }) => {
                        let mut v = c::ContentBlockCitation::builder(
                            text,
                            index.into(),
                            (*end).into(),
                            (*start).into(),
                        )
                        .rest(rest)
                        .build();
                        v.document_title = citation.title.clone();
                        c::TextCitation::ContentBlock(v)
                    }
                    _ => {
                        return Err(Error::invalid(
                            "citation range does not match its Claude anchor",
                        ));
                    }
                }
            }
            ClaudeAnchor::Web { encrypted_index } => {
                let Some(url) = uri(citation) else {
                    return Ok(None);
                };
                let title = citation.title.clone().ok_or_else(|| {
                    Error::invalid("Claude web citation request requires a title")
                })?;
                c::TextCitation::Web(
                    c::WebCitation::builder(text, encrypted_index, title, url.into())
                        .rest(rest)
                        .build(),
                )
            }
            ClaudeAnchor::Search {
                search_result_index,
            } => {
                let Some(source) = uri(citation) else {
                    return Ok(None);
                };
                let CitationSource::Document {
                    range: Some(DocumentRange::Chunks { start, end }),
                    ..
                } = &citation.source
                else {
                    return Ok(None);
                };
                let title = citation.title.clone().ok_or_else(|| {
                    Error::invalid("Claude search citation request requires a title")
                })?;
                c::TextCitation::Search(
                    c::SearchCitation::builder(
                        text,
                        (*end).into(),
                        search_result_index,
                        source.into(),
                        (*start).into(),
                        title,
                    )
                    .rest(rest)
                    .build(),
                )
            }
        };
    Ok(Some(value))
}
