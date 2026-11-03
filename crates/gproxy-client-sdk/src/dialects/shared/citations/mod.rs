//! Citations retain native-only anchors in their own vendor metadata.
use super::*;

#[cfg(any(feature = "claude", feature = "openai"))]
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

#[cfg(feature = "claude")]
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

#[cfg(any(feature = "claude", feature = "openai"))]
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

#[cfg(feature = "claude")]
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
#[serde(tag = "kind", rename_all = "snake_case")]
enum ClaudeAnchor {
    Char,
    Page,
    Chunk,
    Web { encrypted_index: String },
    Search { search_result_index: i64 },
}

#[cfg(feature = "claude")]
#[derive(Serialize, Deserialize)]
struct ClaudeCitation {
    anchor: ClaudeAnchor,
    #[serde(default)]
    file_id_present: bool,
    #[serde(default, flatten)]
    rest: Rest,
}

#[cfg(any(feature = "openai", feature = "chat"))]
#[derive(Serialize, Deserialize)]
struct UrlMetadata {
    #[serde(default)]
    rest: Rest,
    #[serde(default)]
    annotation_rest: Rest,
}
#[cfg(feature = "claude")]
mod claude;
#[cfg(feature = "claude")]
pub(crate) use claude::*;

#[cfg(any(feature = "openai", feature = "chat"))]
mod openai;
#[cfg(any(feature = "openai", feature = "chat"))]
pub(crate) use openai::*;

#[cfg(feature = "gemini")]
mod gemini;
#[cfg(feature = "gemini")]
pub(crate) use gemini::*;
