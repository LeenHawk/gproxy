use serde::{Deserialize, Serialize};

use super::super::{JsonObject, TypedObject};
use super::{ImageBlock, TextBlock};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageSource {
    Base64(Base64ImageSource),
    Url(UrlImageSource),
    File(FileImageSource),
    Raw(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocumentSource {
    Base64(Base64PdfSource),
    Text(PlainTextSource),
    Content(ContentSource),
    Url(UrlDocumentSource),
    File(FileDocumentSource),
    Raw(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Base64ImageSource {
    pub data: String,
    pub media_type: ImageMediaType,
    #[serde(rename = "type")]
    pub type_: Base64SourceType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Base64PdfSource {
    pub data: String,
    pub media_type: PdfMediaType,
    #[serde(rename = "type")]
    pub type_: Base64SourceType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Base64SourceType {
    #[serde(rename = "base64")]
    Base64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMediaType {
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfMediaType {
    #[serde(rename = "application/pdf")]
    ApplicationPdf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlImageSource {
    #[serde(rename = "type")]
    pub type_: UrlSourceType,
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlDocumentSource {
    #[serde(rename = "type")]
    pub type_: UrlSourceType,
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrlSourceType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileImageSource {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: FileSourceType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDocumentSource {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: FileSourceType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileSourceType {
    #[serde(rename = "file")]
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlainTextSource {
    pub data: String,
    pub media_type: PlainTextMediaType,
    #[serde(rename = "type")]
    pub type_: TextSourceType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlainTextMediaType {
    #[serde(rename = "text/plain")]
    TextPlain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextSourceType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentSource {
    pub content: ContentSourceContent,
    #[serde(rename = "type")]
    pub type_: ContentSourceType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentSourceType {
    #[serde(rename = "content")]
    Content,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentSourceContent {
    Text(String),
    Blocks(Vec<ContentSourceBlock>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentSourceBlock {
    Text(TextBlock),
    Image(ImageBlock),
    Raw(TypedObject),
}
