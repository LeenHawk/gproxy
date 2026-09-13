use crate::{
    Rest,
    transform::{Converted, Report, TransformError},
    wire::{claude, gemini, openai},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilePurpose {
    Assistants,
    Batch,
    FineTune,
    UserData,
    Vision,
    AssistantsOutput,
    BatchOutput,
    FineTuneResults,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatusFacts {
    Uploaded,
    Processing,
    Active,
    Failed,
    Unknown,
}

/// Facts supplied by the host or a provider response. `None` means unknown and
/// remains unknown through conversion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileFacts {
    pub id: Option<String>,
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub bytes: Option<u64>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
    pub purpose: Option<FilePurpose>,
    pub status: Option<FileStatusFacts>,
    pub provider_scope: Option<String>,
}

fn require_id(facts: &FileFacts) -> Result<String, TransformError> {
    facts
        .id
        .clone()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| TransformError::missing_metadata("target file.id"))
}

pub fn openai_to_claude(
    source: &openai::files::FileObject,
    facts: &FileFacts,
) -> Result<Converted<claude::files::FileMetadata>, TransformError> {
    if source.bytes < 0 {
        return Err(TransformError::invalid_result(
            "file.bytes",
            "negative source size",
        ));
    }
    let id = require_id(facts)?;
    let created = iso(facts
        .created_at
        .as_deref()
        .unwrap_or(&source.created_at.to_string()))?;
    let filename = facts
        .filename
        .clone()
        .unwrap_or_else(|| source.filename.clone());
    let mime = facts
        .mime
        .clone()
        .ok_or_else(|| TransformError::missing_metadata("file.mime"))?;
    let bytes = facts
        .bytes
        .or_else(|| u64::try_from(source.bytes).ok())
        .ok_or_else(|| TransformError::missing_metadata("file.bytes"))?;
    Ok(Converted {
        value: claude::files::FileMetadata {
            id,
            created_at: created,
            filename,
            mime_type: mime,
            size_bytes: i64::try_from(bytes)
                .map_err(|_| TransformError::shape("file.bytes", "size overflows target"))?,
            type_: claude::files::FileType::File,
            downloadable: None,
            scope: None,
            rest: Rest::new(),
        },
        report: mapping_report(),
    })
}

pub fn openai_to_gemini(
    source: &openai::files::FileObject,
    facts: &FileFacts,
) -> Result<Converted<gemini::files::File>, TransformError> {
    let id = require_id(facts)?;
    gemini_id(&id)?;
    let source_size = u64::try_from(source.bytes)
        .map_err(|_| TransformError::invalid_result("file.bytes", "negative size"))?;
    Ok(Converted {
        value: gemini::files::File {
            name: Some(Some(id)),
            display_name: Some(Some(
                facts
                    .filename
                    .clone()
                    .unwrap_or_else(|| source.filename.clone()),
            )),
            mime_type: facts.mime.clone().map(Some),
            size_bytes: Some(Some(gemini::files::FileSize::String(
                facts.bytes.unwrap_or(source_size).to_string(),
            ))),
            create_time: Some(Some(iso(facts
                .created_at
                .as_deref()
                .unwrap_or(&source.created_at.to_string()))?)),
            update_time: None,
            expiration_time: facts
                .expires_at
                .as_ref()
                .map(|v| iso(v).map(Some))
                .transpose()?
                .or(source
                    .expires_at
                    .flatten()
                    .map(|v| iso(&v.to_string()).map(Some))
                    .transpose()?),
            sha256_hash: None,
            uri: None,
            download_uri: None,
            state: gemini_state(facts.status.as_ref()),
            source: None,
            video_metadata: None,
            error: None,
            rest: Rest::new(),
        },
        report: mapping_report(),
    })
}

pub fn claude_to_openai(
    source: &claude::files::FileMetadata,
    facts: &FileFacts,
) -> Result<Converted<openai::files::FileObject>, TransformError> {
    openai_metadata(
        source.size_bytes,
        &source.filename,
        &source.created_at,
        facts,
    )
}
fn openai_metadata(
    source_size: i64,
    filename: &str,
    created: &str,
    facts: &FileFacts,
) -> Result<Converted<openai::files::FileObject>, TransformError> {
    if source_size < 0 {
        return Err(TransformError::invalid_result(
            "file.bytes",
            "negative size",
        ));
    }
    let id = require_id(facts)?;
    let created_at = epoch(facts.created_at.as_deref().unwrap_or(created))?;
    let bytes = facts
        .bytes
        .or_else(|| u64::try_from(source_size).ok())
        .ok_or_else(|| TransformError::missing_metadata("file.bytes"))?;
    let purpose = match facts
        .purpose
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("file.purpose"))?
    {
        FilePurpose::Assistants => openai::files::FilePurpose::Assistants,
        FilePurpose::Batch => openai::files::FilePurpose::Batch,
        FilePurpose::FineTune => openai::files::FilePurpose::FineTune,
        FilePurpose::UserData => openai::files::FilePurpose::UserData,
        FilePurpose::Vision => openai::files::FilePurpose::Vision,
        FilePurpose::AssistantsOutput => openai::files::FilePurpose::AssistantsOutput,
        FilePurpose::BatchOutput => openai::files::FilePurpose::BatchOutput,
        FilePurpose::FineTuneResults => openai::files::FilePurpose::FineTuneResults,
        FilePurpose::Other(_) => {
            return Err(TransformError::unsupported(
                "file.purpose",
                "purpose has no OpenAI equivalent",
            ));
        }
    };
    let status = match facts
        .status
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("file.status"))?
    {
        FileStatusFacts::Uploaded => openai::files::FileStatus::Uploaded,
        FileStatusFacts::Active => openai::files::FileStatus::Processed,
        FileStatusFacts::Processing => {
            return Err(TransformError::unsupported(
                "file.status",
                "processing is not processed",
            ));
        }
        FileStatusFacts::Failed => openai::files::FileStatus::Error,
        FileStatusFacts::Unknown => return Err(TransformError::missing_metadata("file.status")),
    };
    Ok(Converted {
        value: openai::files::FileObject {
            id,
            bytes: i64::try_from(bytes)
                .map_err(|_| TransformError::shape("file.bytes", "size overflows target"))?,
            created_at,
            filename: facts
                .filename
                .clone()
                .unwrap_or_else(|| filename.to_owned()),
            object: openai::files::FileObjectType::File,
            purpose,
            status,
            status_details: None,
            expires_at: facts
                .expires_at
                .as_ref()
                .map(|value| epoch(value).map(Some))
                .transpose()?,
            rest: Rest::new(),
        },
        report: mapping_report(),
    })
}

pub fn gemini_to_openai(
    source: &gemini::files::File,
    facts: &FileFacts,
) -> Result<Converted<openai::files::FileObject>, TransformError> {
    let size = gemini_size(source)?;
    let filename = facts
        .filename
        .clone()
        .or_else(|| source.display_name.clone().flatten())
        .ok_or_else(|| TransformError::missing_metadata("file.filename"))?;
    let created = facts
        .created_at
        .clone()
        .or_else(|| source.create_time.clone().flatten())
        .ok_or_else(|| TransformError::missing_metadata("file.created_at"))?;
    let mut target_facts = facts.clone();
    target_facts.created_at = Some(created);
    target_facts.expires_at = target_facts
        .expires_at
        .or_else(|| source.expiration_time.clone().flatten());
    if target_facts.status.is_none() {
        target_facts.status =
            source
                .state
                .as_ref()
                .and_then(Option::as_ref)
                .map(|state| match state {
                    gemini::files::FileState::Processing => FileStatusFacts::Processing,
                    gemini::files::FileState::Active => FileStatusFacts::Active,
                    gemini::files::FileState::Failed => FileStatusFacts::Failed,
                    gemini::files::FileState::StateUnspecified => FileStatusFacts::Unknown,
                });
    }
    let size = i64::try_from(
        facts
            .bytes
            .or(size)
            .ok_or_else(|| TransformError::missing_metadata("file.bytes"))?,
    )
    .map_err(|_| TransformError::shape("file.bytes", "size exceeds i64"))?;
    openai_metadata(
        size,
        &filename,
        &target_facts.created_at.clone().unwrap(),
        &target_facts,
    )
}

pub fn claude_to_gemini(
    source: &claude::files::FileMetadata,
    facts: &FileFacts,
) -> Result<Converted<gemini::files::File>, TransformError> {
    let id = require_id(facts)?;
    gemini_id(&id)?;
    let size = u64::try_from(source.size_bytes)
        .map_err(|_| TransformError::shape("file.bytes", "negative size"))?;
    Ok(Converted {
        value: gemini::files::File {
            name: Some(Some(id)),
            display_name: Some(Some(
                facts
                    .filename
                    .clone()
                    .unwrap_or_else(|| source.filename.clone()),
            )),
            mime_type: Some(Some(
                facts
                    .mime
                    .clone()
                    .unwrap_or_else(|| source.mime_type.clone()),
            )),
            size_bytes: Some(Some(gemini::files::FileSize::String(
                facts.bytes.unwrap_or(size).to_string(),
            ))),
            create_time: Some(Some(iso(facts
                .created_at
                .as_deref()
                .unwrap_or(&source.created_at))?)),
            update_time: None,
            expiration_time: facts
                .expires_at
                .as_ref()
                .map(|v| iso(v).map(Some))
                .transpose()?,
            sha256_hash: None,
            uri: None,
            download_uri: None,
            state: gemini_state(facts.status.as_ref()),
            source: None,
            video_metadata: None,
            error: None,
            rest: Rest::new(),
        },
        report: mapping_report(),
    })
}

pub fn gemini_to_claude(
    source: &gemini::files::File,
    facts: &FileFacts,
) -> Result<Converted<claude::files::FileMetadata>, TransformError> {
    let id = require_id(facts)?;
    let source_size = gemini_size(source)?;
    let bytes = facts
        .bytes
        .or(source_size)
        .ok_or_else(|| TransformError::missing_metadata("file.bytes"))?;
    let created = iso(&facts
        .created_at
        .clone()
        .or_else(|| source.create_time.clone().flatten())
        .ok_or_else(|| TransformError::missing_metadata("file.created_at"))?)?;
    Ok(Converted {
        value: claude::files::FileMetadata {
            id,
            created_at: created,
            filename: facts
                .filename
                .clone()
                .or_else(|| source.display_name.as_ref().and_then(|v| v.clone()))
                .ok_or_else(|| TransformError::missing_metadata("file.filename"))?,
            mime_type: facts
                .mime
                .clone()
                .or_else(|| source.mime_type.as_ref().and_then(|v| v.clone()))
                .ok_or_else(|| TransformError::missing_metadata("file.mime"))?,
            size_bytes: i64::try_from(bytes)
                .map_err(|_| TransformError::shape("file.bytes", "size overflows target"))?,
            type_: claude::files::FileType::File,
            downloadable: None,
            scope: None,
            rest: Rest::new(),
        },
        report: mapping_report(),
    })
}

fn date(value: &str) -> Result<time::OffsetDateTime, TransformError> {
    if let Ok(seconds) = value.parse::<i64>() {
        time::OffsetDateTime::from_unix_timestamp(seconds)
            .map_err(|e| TransformError::shape("file.timestamp", e.to_string()))
    } else {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
            .map_err(|e| TransformError::shape("file.timestamp", e.to_string()))
    }
}
fn iso(value: &str) -> Result<String, TransformError> {
    date(value)?
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| TransformError::shape("file.timestamp", e.to_string()))
}
fn epoch(value: &str) -> Result<i64, TransformError> {
    let value = date(value)?;
    if value.nanosecond() != 0 {
        return Err(TransformError::unsupported(
            "file.timestamp",
            "fractional timestamp cannot fit Unix seconds exactly",
        ));
    }
    Ok(value.unix_timestamp())
}
fn gemini_size(source: &gemini::files::File) -> Result<Option<u64>, TransformError> {
    source
        .size_bytes
        .as_ref()
        .and_then(Option::as_ref)
        .map(|value| match value {
            gemini::files::FileSize::String(value) => value
                .parse::<u64>()
                .map_err(|_| TransformError::invalid_result("file.size", "invalid size")),
            gemini::files::FileSize::Integer(value) => u64::try_from(*value)
                .map_err(|_| TransformError::invalid_result("file.size", "negative size")),
        })
        .transpose()
}

fn gemini_id(value: &str) -> Result<(), TransformError> {
    if !value.starts_with("files/") || value.len() == 6 {
        Err(TransformError::shape(
            "target file.name",
            "files/{id} resource required",
        ))
    } else {
        Ok(())
    }
}

fn gemini_state(value: Option<&FileStatusFacts>) -> Option<Option<gemini::files::FileState>> {
    match value {
        Some(FileStatusFacts::Processing) => Some(Some(gemini::files::FileState::Processing)),
        Some(FileStatusFacts::Active) => Some(Some(gemini::files::FileState::Active)),
        Some(FileStatusFacts::Failed) => Some(Some(gemini::files::FileState::Failed)),
        Some(FileStatusFacts::Uploaded | FileStatusFacts::Unknown) | None => None,
    }
}
fn mapping_report() -> Report {
    let mut report = Report::default();
    report.omitted("file.provider_metadata","native URLs/hash/scope/downloadability and provider lifecycle details require separately bound resource facts");
    report
}
