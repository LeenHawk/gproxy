use super::*;
impl Walk {
    pub(super) fn cache<T>(&mut self, block: &ContentBlock<T>, supported: bool, path: &str) {
        let Some(cache) = &block.cache else { return };
        if !supported {
            self.warn(
                WarningKind::Unsupported,
                &format!("{path}.cache"),
                "This native content kind has no cache breakpoint",
            );
            return;
        }
        let Some(ttl) = &cache.ttl else { return };
        let supported = match self.vendor {
            Vendor::Claude => matches!(ttl, CacheTtl::FiveMinutes | CacheTtl::OneHour),
            Vendor::Openai => matches!(ttl, CacheTtl::ThirtyMinutes | CacheTtl::TwelveHours),
            _ => false,
        };
        if !supported {
            self.warn(
                WarningKind::Unsupported,
                &format!("{path}.cache.ttl"),
                "The requested TTL is omitted; the native cache breakpoint uses its default TTL",
            );
        }
    }
    pub(super) fn detail(&mut self, detail: &Detail, document: bool, path: &str) {
        let (kind, message) = if matches!(detail, Detail::Unknown(_)) {
            (
                WarningKind::Unsupported,
                "Unknown detail is omitted or uses native Auto where required",
            )
        } else if *detail == Detail::Auto {
            return;
        } else if self.vendor == Vendor::Claude {
            (
                WarningKind::Unsupported,
                "Claude has no per-block detail setting",
            )
        } else if self.is_responses()
            && document
            && matches!(detail, Detail::Medium | Detail::Original)
        {
            (
                WarningKind::Unsupported,
                "This document detail is not supported by Responses and is omitted",
            )
        } else if self.is_responses() && !document && *detail == Detail::Medium {
            (
                WarningKind::Compatibility,
                "Responses maps medium image detail to native Auto",
            )
        } else if self.is_chat()
            && (document || matches!(detail, Detail::Medium | Detail::Original))
        {
            (
                WarningKind::Unsupported,
                "This detail is not supported by Chat and is omitted",
            )
        } else {
            return;
        };
        self.warn(kind, &format!("{path}.detail"), message);
    }
    pub(super) fn citations(
        &mut self,
        text: &Text,
        path: &str,
        responses_output: bool,
    ) -> Result<()> {
        for (index, citation) in text.citations.iter().enumerate() {
            let path = format!("{path}.citations[{index}]");
            self.metadata(&citation.provider_metadata, &path)?;
            let native = presence(&citation.provider_metadata, &self.vendor)?.native;
            let supported = if self.vendor == Vendor::Claude && native {
                match &citation.source {
                    CitationSource::Url { .. } => true,
                    CitationSource::Document {
                        document: DocumentReference::Index { vendor, .. },
                        range: Some(_),
                    } => *vendor == Vendor::Claude,
                    CitationSource::Document {
                        document: DocumentReference::Block { document },
                        range: Some(DocumentRange::Chunks { .. }),
                    } => matches!(
                        &document.content.source,
                        DocumentSource::Asset {
                            source: Asset::Url { .. }
                        }
                    ),
                    _ => false,
                }
            } else if self.is_responses() && responses_output {
                match &citation.source {
                    CitationSource::Url { .. } => {
                        citation.text_range.is_some() && citation.title.is_some()
                    }
                    CitationSource::Document { .. } => native,
                    CitationSource::Unknown(_) => false,
                }
            } else {
                false
            };
            if !supported {
                self.warn(WarningKind::Unsupported,&path,"Citation has no supported request representation for this dialect and is omitted");
            } else if self.vendor == Vendor::Claude && citation.text_range.is_some() {
                self.warn(
                    WarningKind::Unsupported,
                    &format!("{path}.text_range"),
                    "Claude citation parameters do not carry output-text offsets",
                );
            }
        }
        Ok(())
    }
    pub(super) fn document(&mut self, document: &Document, path: &str) -> Result<()> {
        self.detail(&document.detail, true, path);
        if self.vendor != Vendor::Claude {
            if document.context.is_some() {
                self.warn(
                    WarningKind::Unsupported,
                    &format!("{path}.context"),
                    "Document context is omitted by this dialect",
                );
            }
            if document.citations.is_some() {
                self.warn(
                    WarningKind::Unsupported,
                    &format!("{path}.citations"),
                    "This dialect has no document citation-enable switch",
                );
            }
        }
        if matches!(&document.source, DocumentSource::Text { .. })
            && document
                .media_type
                .as_deref()
                .is_some_and(|mime| mime != "text/plain")
        {
            self.warn(
                WarningKind::Compatibility,
                &format!("{path}.media_type"),
                "Plain document text uses text/plain",
            );
        }
        if let DocumentSource::Chunks { chunks } = &document.source {
            for (index, chunk) in chunks.iter().enumerate() {
                let path = format!("{path}.source.chunks[{index}]");
                self.metadata(&chunk.provider_metadata, &path)?;
                self.cache(chunk, self.vendor == Vendor::Claude, &path);
                match &chunk.content {
                    ChunkContent::Text(text) => self.citations(text, &path, false)?,
                    ChunkContent::Image(image) => self.detail(&image.detail, false, &path),
                    ChunkContent::Unknown(_) => self.warn(
                        WarningKind::Unsupported,
                        &path,
                        "Unknown document chunk is not sent",
                    ),
                }
            }
        }
        Ok(())
    }
}
