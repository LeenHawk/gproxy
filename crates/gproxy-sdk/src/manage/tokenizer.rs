//! Tokenizer vocabularies: the files local token estimation counts with.
//!
//! Core estimates tokens for exchanges whose upstream reported none, and it
//! does that against a vocabulary file. A deployment that cares about the
//! accuracy of its own cost figures downloads the real vocabulary for the
//! models it serves; without one the built-in estimate is a rough guess.
//!
//! A vocabulary is an ordinary `file_objects` row, so it needs file storage
//! configured. Without it this whole family answers
//! [`SdkError::Unsupported`]: there is nowhere to put the bytes, and writing
//! the row without them would leave core loading a file that does not exist.
//!
//! The source token is sealed exactly like a credential secret, under a fixed
//! identity, so a copy of the database carries no usable Hugging Face token.

use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use futures_util::StreamExt;
use gproxy_client::OutboundClient;
use gproxy_protocol::{HttpBody, connection::Bytes};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::{
    config::setting,
    resource::file_object,
    upstream::model::{self},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Scope, Writer, crud};
use crate::{SdkError, SdkResult};

/// Where a vocabulary comes from. Only the repository and the file inside it
/// vary; the host and the `main` revision do not.
const HUGGING_FACE: &str = "https://huggingface.co";
/// The file every Hugging Face repository keeps its tokenizer in.
const DEFAULT_FILENAME: &str = "tokenizer.json";
/// The `file_objects.storage_name` a vocabulary is written under. It is what
/// tells a vocabulary apart from a published request body in the same table.
const STORAGE_NAME: &str = "tokenizer";
use super::settings::TOKENIZER_SECRET_ID;

/// Download progress, published as the bytes arrive.
///
/// This is one cell per process, not per handle: a console polls it from a
/// different request than the one driving the download, and two handles in the
/// same process would be two views of the same operator anyway. It is also not
/// shared between processes — a peer's download is invisible here, which is
/// why it is progress rather than state.
static PROGRESS: LazyLock<ArcSwap<Option<TokenizerProgressDto>>> =
    LazyLock::new(|| ArcSwap::from_pointee(None));

/// What to fetch, and what to point at it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct TokenizerFetch {
    /// A Hugging Face repository id, `owner/name`.
    pub repo: String,
    /// The file inside it; `tokenizer.json` when absent.
    #[serde(default)]
    pub filename: Option<String>,
    /// The catalog model whose `vocabulary_file_id` should point at the
    /// result. Absent leaves every model alone.
    #[serde(default)]
    pub model_id: Option<String>,
    /// Whether this also becomes the instance default, used by models that
    /// select no vocabulary of their own.
    #[serde(default)]
    pub set_as_default: bool,
}

/// One stored vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct VocabularyDto {
    pub file_id: String,
    pub filename: Option<String>,
    pub size_bytes: i64,
    pub created_at_ms: i64,
    /// The catalog models that select it, by id.
    pub models: Vec<String>,
    /// Whether it is the instance default.
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct TokenizerProgressDto {
    pub repo: String,
    pub filename: String,
    pub downloaded_bytes: u64,
    /// From `Content-Length`, when the upstream sent one.
    pub total_bytes: Option<u64>,
}

/// Whether a source token is configured. The token itself is never reported
/// by a read; `reveal_auth` is the one deliberate disclosure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct TokenizerAuthDto {
    pub configured: bool,
}

pub struct Tokenizer<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Tokenizer<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }

    /// How far the download running in this process has got, or None when
    /// none is. See the note on the static: this is process-wide.
    pub fn progress(&self) -> Option<TokenizerProgressDto> {
        PROGRESS.load().as_ref().clone()
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Tokenizer<'_, C> {
    /// Every stored vocabulary, newest first, with what selects it.
    pub async fn vocabularies(&self) -> SdkResult<Vec<VocabularyDto>> {
        let files = self
            .writer
            .store()
            .file_objects()
            .query(
                file_object::Entity::find()
                    .filter(file_object::Column::StorageName.eq(STORAGE_NAME))
                    .order_by_desc(file_object::Column::CreatedAtMs),
            )
            .await?;
        let models = self
            .writer
            .store()
            .models()
            .query(model::Entity::find().filter(model::Column::VocabularyFileId.is_not_null()))
            .await?;
        let default = self
            .writer
            .store()
            .settings()
            .get()
            .await?
            .and_then(|settings| settings.default_vocabulary_file_id);
        Ok(files
            .into_iter()
            .map(|file| VocabularyDto {
                models: models
                    .iter()
                    .filter(|row| row.vocabulary_file_id.as_deref() == Some(file.id.as_str()))
                    .map(|row| row.id.clone())
                    .collect(),
                is_default: default.as_deref() == Some(file.id.as_str()),
                filename: file.filename,
                size_bytes: file.size_bytes,
                created_at_ms: file.created_at_ms,
                file_id: file.id,
            })
            .collect())
    }

    /// Download a vocabulary and store it, then point whatever was named at
    /// it — the file row, the model's selection and the instance default all
    /// in one revision commit.
    ///
    /// The download runs on the calling task and publishes its progress as it
    /// goes, so a console asks for the fetch on one request and polls
    /// [`Tokenizer::progress`] on another. A host that wants it in the
    /// background spawns this future itself.
    pub async fn fetch(&self, request: TokenizerFetch) -> SdkResult<VocabularyDto> {
        if !self
            .writer
            .store()
            .settings()
            .get()
            .await?
            .is_some_and(|settings| settings.enable_tokenizer_download)
        {
            return Err(SdkError::invalid(
                "tokenizer downloads are disabled in global settings",
            ));
        }
        let storage = self
            .writer
            .core()
            .file_storage()
            .ok_or(SdkError::Unsupported(
                "this instance has no file storage configured, so a vocabulary has nowhere to go",
            ))?
            .clone();
        let repo = repository(&request.repo)?;
        let filename = filename(request.filename.as_deref())?;
        if let Some(model_id) = &request.model_id {
            crud::require_rows(
                self.writer.store().models(),
                "model",
                std::slice::from_ref(model_id),
            )
            .await?;
        }

        let limit = self.writer.core().snapshot().limits.max_response_body_bytes;
        let bytes = self.download(&repo, &filename, limit).await?;

        let id = crate::ids::random_id();
        let object_key = format!("vocabularies/{id}");
        // The bytes land first. A row pointing at an object that was never
        // written would fail every reload that tries to read it, while an
        // object with no row is only wasted space.
        storage.write(&object_key, bytes.clone()).await?;

        let now = crate::rt::now_ms();
        let file = file_object::ActiveModel {
            id: Set(id.clone()),
            storage_name: Set(STORAGE_NAME.to_owned()),
            object_key: Set(object_key.clone()),
            filename: Set(Some(filename.clone())),
            mime: Set(Some("application/json".to_owned())),
            size_bytes: Set(i64::try_from(bytes.len()).unwrap_or(i64::MAX)),
            created_at_ms: Set(now),
            // A vocabulary is configuration, not a transient body: it expires
            // when someone deletes it.
            expires_at_ms: Set(None),
        };
        let mut statements = vec![BatchStatement::Execute(
            self.writer.store().file_objects().insert_statement(file)?,
        )];
        if let Some(model_id) = &request.model_id
            && let Some(statement) =
                self.writer
                    .store()
                    .models()
                    .update_statement(model::ActiveModel {
                        id: Set(model_id.clone()),
                        vocabulary_file_id: Set(Some(id.clone())),
                        ..Default::default()
                    })?
        {
            statements.push(BatchStatement::Execute(statement));
        }
        if request.set_as_default {
            statements.push(BatchStatement::Execute(
                self.writer
                    .store()
                    .settings()
                    .update_statement(setting::ActiveModel {
                        id: Set(setting::GLOBAL_SETTINGS_ID),
                        default_vocabulary_file_id: Set(Some(id.clone())),
                        ..Default::default()
                    })?,
            ));
        }
        // Models and settings both feed the estimator, which is rebuilt by a
        // full reload.
        self.writer
            .commit(statements, &[Scope::Models, Scope::Settings])
            .await?;

        Ok(VocabularyDto {
            file_id: id.clone(),
            filename: Some(filename),
            size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            created_at_ms: now,
            models: request.model_id.into_iter().collect(),
            is_default: request.set_as_default,
        })
    }

    /// Forget a vocabulary. The row goes first, inside the revision commit
    /// that also releases whatever selected it; the stored object is removed
    /// afterwards, because an object without a row costs nothing while a row
    /// without an object breaks estimation.
    pub async fn delete(&self, file_id: &str) -> SdkResult<()> {
        let file = self
            .writer
            .store()
            .file_objects()
            .get_many(std::slice::from_ref(&file_id.to_owned()))
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| SdkError::not_found("file object", file_id))?;
        self.writer
            .commit(
                vec![BatchStatement::Execute(
                    self.writer
                        .store()
                        .file_objects()
                        .delete_statement(file.id.clone()),
                )],
                &[Scope::Models, Scope::Settings],
            )
            .await?;
        if let Some(storage) = self.writer.core().file_storage()
            && let Err(error) = storage.delete(&file.object_key).await
        {
            // The row is gone and nothing reads the object any more.
            tracing::warn!(%error, file_id = %file.id, "stored vocabulary object was not removed");
        }
        Ok(())
    }

    /// Whether a source token is configured.
    pub async fn auth(&self) -> SdkResult<TokenizerAuthDto> {
        Ok(TokenizerAuthDto {
            configured: self.sealed_token().await?.is_some(),
        })
    }

    /// Store or clear the source token. `None` clears it and downloads
    /// anonymously again, which is enough for a public repository.
    pub async fn set_auth(&self, token: Option<String>) -> SdkResult<TokenizerAuthDto> {
        let sealed = match crud::optional_text(token) {
            Some(token) => Some(
                self.writer
                    .core()
                    .secret_codec()
                    .seal(TOKENIZER_SECRET_ID, &serde_json::json!({ "token": token }))?,
            ),
            None => None,
        };
        let configured = sealed.is_some();
        let statement = self
            .writer
            .store()
            .settings()
            .update_statement(setting::ActiveModel {
                id: Set(setting::GLOBAL_SETTINGS_ID),
                tokenizer_auth_token: Set(sealed),
                ..Default::default()
            })?;
        self.writer
            .commit(vec![BatchStatement::Execute(statement)], &[Scope::Settings])
            .await?;
        Ok(TokenizerAuthDto { configured })
    }

    /// The token in the clear. Separate from `auth` for the same reason
    /// revealing a credential secret is separate from listing credentials: it
    /// is rare, and it is the one call worth authorizing and auditing.
    pub async fn reveal_auth(&self) -> SdkResult<String> {
        let sealed = self
            .sealed_token()
            .await?
            .ok_or_else(|| SdkError::not_found("tokenizer auth token", TOKENIZER_SECRET_ID))?;
        let opened = self
            .writer
            .core()
            .secret_codec()
            .open(TOKENIZER_SECRET_ID, &sealed)?;
        opened
            .get("token")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| SdkError::invalid("the stored tokenizer token is not readable"))
    }

    async fn sealed_token(&self) -> SdkResult<Option<Vec<u8>>> {
        Ok(self
            .writer
            .store()
            .settings()
            .get()
            .await?
            .and_then(|settings| settings.tokenizer_auth_token)
            .filter(|token| !token.is_empty()))
    }

    /// `GET https://huggingface.co/{repo}/resolve/main/{filename}`, with the
    /// stored token when there is one. `Accept-Encoding: identity` is not
    /// politeness: it is what makes `Content-Length` a truthful denominator
    /// for the progress this publishes.
    async fn download(&self, repo: &str, filename: &str, limit: u64) -> SdkResult<Bytes> {
        let url = format!("{HUGGING_FACE}/{repo}/resolve/main/{filename}");
        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .header(http::header::ACCEPT_ENCODING, "identity");
        if let Some(sealed) = self.sealed_token().await?
            && let Ok(opened) = self
                .writer
                .core()
                .secret_codec()
                .open(TOKENIZER_SECRET_ID, &sealed)
            && let Some(token) = opened.get("token").and_then(Value::as_str)
        {
            builder = builder.header(http::header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let request = builder
            .body(HttpBody::Bytes(Bytes::new()))
            .map_err(|error| SdkError::invalid(format!("invalid vocabulary URL: {error}")))?;

        let client = self
            .writer
            .core()
            .clients()
            .get(&gproxy_client::ConnectionConfig::default())
            .await
            .map_err(|error| SdkError::invalid(format!("no usable client: {error}")))?;
        let response = client
            .send(request)
            .await
            .map_err(|error| SdkError::invalid(format!("vocabulary download failed: {error}")))?;

        let total = response
            .headers
            .get(http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if !response.status.is_success() {
            // The upstream's own status travels: "404" and "401" mean very
            // different things to whoever typed the repository name.
            return Err(SdkError::Upstream {
                status: response.status.as_u16(),
                body: format!("{url} could not be downloaded"),
            });
        }
        if total.is_some_and(|total| total > limit) {
            return Err(SdkError::invalid(format!(
                "the vocabulary at {url} is larger than this instance's {limit}-byte response limit"
            )));
        }

        let publish = |downloaded: u64| {
            PROGRESS.store(Arc::new(Some(TokenizerProgressDto {
                repo: repo.to_owned(),
                filename: filename.to_owned(),
                downloaded_bytes: downloaded,
                total_bytes: total,
            })));
        };
        publish(0);
        let collected = collect(response.body, limit, &publish).await;
        // The cell is cleared whether the download finished or failed: a
        // console that keeps polling should see it stop, not stall.
        PROGRESS.store(Arc::new(None));
        collected
    }
}

/// Read the body, publishing progress per chunk and refusing to buffer past
/// the instance's response limit.
///
/// `publish` is `+ Sync` so that the future this is awaited inside stays
/// `Send`: a `&dyn Fn` held across an await is only `Send` when the trait
/// object is `Sync`, and without it `Tokenizer::fetch` could not be called
/// from an axum handler at all — every host requires a `Send` future.
async fn collect(body: HttpBody, limit: u64, publish: &(dyn Fn(u64) + Sync)) -> SdkResult<Bytes> {
    let too_large = || {
        SdkError::invalid(format!(
            "the vocabulary is larger than this instance's {limit}-byte response limit"
        ))
    };
    match body {
        HttpBody::Bytes(bytes) => {
            if bytes.len() as u64 > limit {
                return Err(too_large());
            }
            publish(bytes.len() as u64);
            Ok(bytes)
        }
        HttpBody::Stream(mut stream) => {
            let mut out: Vec<u8> = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| {
                    SdkError::invalid(format!("vocabulary transfer failed: {error}"))
                })?;
                if out.len() as u64 + chunk.len() as u64 > limit {
                    return Err(too_large());
                }
                out.extend_from_slice(&chunk);
                publish(out.len() as u64);
            }
            Ok(Bytes::from(out))
        }
    }
}

/// `owner/name`, and nothing that could climb out of the URL path.
fn repository(value: &str) -> SdkResult<String> {
    let value = crud::text(value, "repo")?;
    let mut parts = value.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(SdkError::invalid("repo must be `owner/name`"));
    };
    if !segment(owner) || !segment(name) {
        return Err(SdkError::invalid(
            "repo may only contain letters, digits, `-`, `_` and `.`, and may not start or end with `.` or `-`",
        ));
    }
    Ok(value)
}

/// The file inside the repository. A path is refused rather than joined: the
/// URL is built by interpolation, so a `..` would leave the repository.
fn filename(value: Option<&str>) -> SdkResult<String> {
    let value = match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value.to_owned(),
        None => return Ok(DEFAULT_FILENAME.to_owned()),
    };
    if !segment(&value) {
        return Err(SdkError::invalid(
            "filename may only contain letters, digits, `-`, `_` and `.`, and may not start or end with `.` or `-`",
        ));
    }
    Ok(value)
}

fn segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !value.starts_with(['.', '-'])
        && !value.ends_with(['.', '-'])
        && !value.contains("..")
}
