//! Gateway ownership of provider-native files and video jobs.
//!
//! # Why this exists
//!
//! A file or video id is minted by the upstream account that holds it, and
//! one upstream credential is routinely shared by many gateway callers. The
//! upstream cannot tell them apart: to it every request is the credential's
//! own. Without a check of ours, any caller who learns an id — guessed,
//! leaked in a log, or simply listed — could retrieve, download or delete a
//! file another caller uploaded, or poll and download another caller's video,
//! and a file or video *list* would return everything under the credential.
//!
//! # The registry
//!
//! Ownership is recorded in `resource_bindings`, the store's existing
//! client/upstream resource mapping (the same table service resources and
//! publications live in), one row per created resource:
//!
//! | column | value |
//! |---|---|
//! | `scope` | the request's opaque scope plus the provider, `{scope}\u{1f}{provider}` — the `ResourceScope::column` layout, so one caller's ids never alias across providers |
//! | `kind` | [`FILE_KIND`] or [`VIDEO_KIND`] |
//! | `public_id` | the id the client was given (canonical form, see below) |
//! | `provider_id`, `credential_id` | where the resource lives |
//! | `upstream_id` | the same id for passthrough; absent for a converted video, whose upstream operation name lives in the conversion's job state |
//! | `summary` | the creating `user_id` and `api_key_id` from the request's attribution, and the operation |
//!
//! `user_id` is deliberately left out of the foreign-keyed column: core takes
//! attribution as reported facts, and an embedding host is free to attribute
//! to an id that is not a `users` row, which the cascade key would refuse.
//!
//! The owner is the **scope**, not the user or the key. Scope is the upper
//! layer's answer to "may these two requests be treated as the same caller's
//! traffic?" (`user:{id}` for every key and session of one person,
//! `grant:{id}` for each OAuth grant), and it is what continuations and
//! service resources are already isolated by. Core never sees roles, so there
//! is no admin bypass here either: an operator reaches any resource through
//! the upstream's own console, not by borrowing a caller's id.
//!
//! # The rules
//!
//! * **Create** (`CreateFile`, `CreateVideo`): after a successful answer the
//!   id is read from the client-dialect response and a row is written for the
//!   request's scope on the credential that served it.
//! * **Read and delete** (`RetrieveFile`, `RetrieveFileContent`, `DeleteFile`,
//!   `RetrieveVideo`, `DownloadVideoContent`, `DeleteVideo`): before anything
//!   is sent the id is taken from the path and looked up in the caller's
//!   scope. No row — never created through the gateway, or created by someone
//!   else — is [`CoreError::ResourceNotFound`], the same answer either way so
//!   an id's existence leaks nothing. A row pins the request to its recorded
//!   credential; when the caller may no longer use that credential it is the
//!   same not-found. A successful delete removes the row.
//! * **List** (`ListFiles`, `ListVideos`): the upstream's page is filtered to
//!   the ids the scope owns on the credential that answered. Cursors
//!   (`has_more`, `first_id`/`last_id`, `nextPageToken`) are left exactly as
//!   the upstream sent them so paging still advances: a page may therefore
//!   hold fewer items than `limit` — even none while `has_more` is true — and
//!   a cursor may name an id the caller does not own, which grants nothing.
//!   A list body that cannot be read is refused rather than passed through
//!   unfiltered.
//!
//! Ids are canonical: Gemini's `files/{id}` resource name is stored bare, as
//! the path segment `/v1beta/files/{id}` carries it, so a file uploaded in one
//! dialect is the same row in another.
//!
//! # Known limits
//!
//! * Resources created before this registry existed, or directly at the
//!   upstream, have no row and are not reachable through the gateway.
//! * A Gemini *resumable* upload is proxied through a gateway upload session
//!   ([`upload`]) so its finalized file is registered like any other; only an
//!   upstream upload URL on an origin other than the provider's base cannot
//!   be proxied, and such a file stays unregistered.
//!
//! # Files referenced inside other requests
//!
//! A file id travels inside a generation body as well — OpenAI Chat's
//! `{"type": "file", "file": {"file_id"}}`, Responses' `input_file` /
//! `input_image` `file_id`, a code-interpreter container's `file_ids`,
//! Claude's `{"source": {"type": "file", "file_id"}}` and `container_upload`,
//! Gemini's `fileData.fileUri` naming a `files/{id}`, a video
//! `input_reference.file_id`. Passthrough hands that body to the upstream as
//! it is, so before dispatch every such id ([`referenced_files`]) must be one
//! the scope owns on the request's provider — or a publication core minted
//! for the same scope — and the request is pinned to the one credential that
//! holds them. An id not owned is [`CoreError::ResourceNotFound`]; ids owned
//! on two different credentials cannot be served by one upstream call and are
//! refused as an invalid request. Conversion reads referenced files through
//! `Resources`, which applies the same check itself; its refusal surfaces as
//! the same not-found ([`NotOwned`]).
//!
//! A multipart form is read too ([`request_references`]): text fields named
//! for file ids (`file_id`, `file_ids`, `file_ids[]`, `x[file_id]`) and the
//! JSON walk over any text field holding JSON; file parts are skipped unread.
//! On a passthrough socket every client message is walked before it is
//! forwarded ([`socket`]). A Responses WebSocket turn is checked against the
//! credential its chain is bound to, and a new chain is opened on the
//! credential holding its first turn's files.
//!
//! The scan is a walk over the JSON for those field names rather than a typed
//! parse per dialect: it has to see a passthrough body exactly as the
//! upstream will, including fields the typed wire models do not carry.
//! Values that are caller data rather than request structure — tool-call
//! `input`/`args`, function responses, JSON schemas, `metadata` — are not
//! walked, so a tool argument that happens to be called `file_id` is not
//! mistaken for a reference.

pub(crate) mod socket;
pub mod upload;

use crate::{Core, CoreError, CoreResult, RequestContext};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    codec::{CodecLimits, read_http_body},
    connection::Bytes,
    transform::TransformError,
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::resource::resource_binding;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// `resource_bindings.kind` of an upstream file owned through the gateway.
pub const FILE_KIND: &str = "upstream_file";
/// `resource_bindings.kind` of an upstream video job owned through the gateway.
pub const VIDEO_KIND: &str = "upstream_video";

/// The two resource families the registry covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedKind {
    File,
    Video,
}

impl OwnedKind {
    /// The `resource_bindings.kind` value.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::File => FILE_KIND,
            Self::Video => VIDEO_KIND,
        }
    }

    /// What an error message calls it.
    const fn noun(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Video => "video",
        }
    }

    /// The collection segment of the path, `/v1/files/{id}`.
    const fn segment(self) -> &'static str {
        match self {
            Self::File => "files",
            Self::Video => "videos",
        }
    }
}

/// What an operation does to an owned resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedAccess {
    Create(OwnedKind),
    /// Reads the resource named in the path: metadata, content or status.
    Read(OwnedKind),
    Delete(OwnedKind),
    List(OwnedKind),
}

impl OwnedAccess {
    /// The access an operation performs, or `None` for one the registry does
    /// not govern.
    pub const fn of(operation: Operation) -> Option<Self> {
        use OwnedKind::{File, Video};
        Some(match operation {
            Operation::CreateFile => Self::Create(File),
            Operation::RetrieveFile | Operation::RetrieveFileContent => Self::Read(File),
            Operation::DeleteFile => Self::Delete(File),
            Operation::ListFiles => Self::List(File),
            Operation::CreateVideo => Self::Create(Video),
            Operation::RetrieveVideo | Operation::DownloadVideoContent => Self::Read(Video),
            Operation::DeleteVideo => Self::Delete(Video),
            Operation::ListVideos => Self::List(Video),
            _ => return None,
        })
    }

    /// The kind of an access that names one existing resource in its path.
    const fn addressed(self) -> Option<OwnedKind> {
        match self {
            Self::Read(kind) | Self::Delete(kind) => Some(kind),
            Self::Create(_) | Self::List(_) => None,
        }
    }
}

/// The persisted scope column: the `ResourceScope::column` layout.
fn scope_column(scope: &str, provider_id: &str) -> String {
    format!("{scope}\u{1f}{provider_id}")
}

/// Gemini names a file `files/{id}`; every other dialect and every path
/// segment carries the bare id.
fn canonical(id: &str) -> &str {
    id.strip_prefix("files/").unwrap_or(id)
}

/// The resource id a read or delete names: the segment after the last
/// `files`/`videos` segment, without Gemini's `:download` method suffix,
/// percent-decoded. A path with no id there is the caller's error.
pub fn path_id(kind: OwnedKind, path: &str) -> CoreResult<String> {
    let segments: Vec<&str> = path.split('/').collect();
    let raw = segments
        .iter()
        .rposition(|segment| *segment == kind.segment())
        .and_then(|at| segments.get(at + 1))
        .copied()
        .unwrap_or("");
    let raw = raw.strip_suffix(":download").unwrap_or(raw);
    let id = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_err(|e| CoreError::InvalidTarget(format!("{} id: {e}", kind.noun())))?;
    if id.is_empty() {
        return Err(CoreError::InvalidTarget(format!(
            "{} id missing from path",
            kind.noun()
        )));
    }
    Ok(canonical(&id).to_owned())
}

/// The id of one resource object in a client-dialect body. Files carry `id`
/// (OpenAI, Claude) or `name` (Gemini); video jobs carry `id`, or xAI's
/// `request_id` and MiniMax's `task_id` where a channel passes those through.
fn object_id(kind: OwnedKind, object: &Value) -> Option<String> {
    let field = |name: &str| object.get(name).and_then(Value::as_str);
    let id = match kind {
        OwnedKind::File => field("id").or_else(|| field("name")),
        OwnedKind::Video => field("id")
            .or_else(|| field("request_id"))
            .or_else(|| field("task_id")),
    }?;
    let id = canonical(id);
    (!id.is_empty()).then(|| id.to_owned())
}

/// The id a successful create answered with. Gemini wraps the file in
/// `{"file": {...}}`.
fn created_id(kind: OwnedKind, body: &Value) -> Option<String> {
    match (kind, body.get("file")) {
        (OwnedKind::File, Some(file)) => object_id(kind, file),
        _ => object_id(kind, body),
    }
}

/// Keep only the items whose id is owned. `Ok(None)` when the body has no
/// items array at all (an empty Gemini page); `Err` when it is not a list.
fn filter_list(
    kind: OwnedKind,
    dialect: Dialect,
    body: &[u8],
    owned: &BTreeSet<String>,
) -> Result<Option<Vec<u8>>, TransformError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|e| TransformError::invalid_result("owned.list", e.to_string()))?;
    if !value.is_object() {
        return Err(TransformError::invalid_result(
            "owned.list",
            "list response is not an object",
        ));
    }
    // Gemini's files page is `{"files": [...]}`, every other list `data`.
    let gemini_files = kind == OwnedKind::File && dialect == Dialect::Gemini;
    let field = if gemini_files { "files" } else { "data" };
    match value.get_mut(field) {
        Some(Value::Array(items)) => {
            items.retain(|item| object_id(kind, item).is_some_and(|id| owned.contains(&id)));
        }
        // Gemini omits `files` from an empty page.
        None if gemini_files => return Ok(None),
        _ => {
            return Err(TransformError::invalid_result(
                "owned.list",
                "list response has no item array",
            ));
        }
    }
    serde_json::to_vec(&value)
        .map(Some)
        .map_err(|e| TransformError::invalid_result("owned.list", e.to_string()))
}

/// Whether an operation's JSON body may reference uploaded files. File and
/// video CRUD address resources by path instead; catalog, audio and realtime
/// signaling bodies carry no file ids.
pub const fn scans_references(operation: Operation) -> bool {
    !matches!(
        operation,
        Operation::ListModels
            | Operation::GetModel
            | Operation::CreateSpeech
            | Operation::CreateTranscription
            | Operation::CreateTranslation
            | Operation::CreateFile
            | Operation::ListFiles
            | Operation::RetrieveFile
            | Operation::RetrieveFileContent
            | Operation::DeleteFile
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent
            | Operation::CreateRealtimeCall
            | Operation::ConnectRealtime
    )
}

/// The file id a Gemini `fileUri` names, when it names an uploaded file:
/// `files/{id}`, or the Files API URL the upload answered with
/// (`https://generativelanguage.googleapis.com/v1beta/files/{id}`). Any other
/// URI — a `gs://` object, a YouTube link, a public URL — is not a file of
/// the Files API and is left alone.
fn gemini_file_id(uri: &str) -> Option<String> {
    let uri = uri.split(['?', '#']).next().unwrap_or(uri);
    let path = match uri.strip_prefix("https://") {
        Some(rest) => {
            let (host, path) = rest.split_once('/')?;
            if !host.ends_with("googleapis.com") {
                return None;
            }
            path
        }
        None if uri.starts_with("files/") => uri,
        None => return None,
    };
    let mut segments = path.rsplit('/');
    let id = segments.next()?;
    (segments.next() == Some("files") && !id.is_empty()).then(|| id.to_owned())
}

/// Keys whose values are caller data or schemas, never request structure.
const OPAQUE_KEYS: &[&str] = &["parameters", "input_schema", "schema", "metadata"];

fn walk_references(value: &Value, gemini: bool, out: &mut BTreeSet<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                walk_references(item, gemini, out);
            }
        }
        Value::Object(map) => {
            let tool_use = map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.ends_with("tool_use"));
            for (key, child) in map {
                let key = key.as_str();
                if OPAQUE_KEYS.contains(&key) {
                    continue;
                }
                if gemini {
                    match key {
                        "fileUri" | "file_uri" => {
                            if let Some(id) = child.as_str().and_then(gemini_file_id) {
                                out.insert(id);
                            }
                            continue;
                        }
                        // functionCall arguments and functionResponse payloads.
                        "args" | "response" => continue,
                        _ => {}
                    }
                } else {
                    match key {
                        "file_id" => {
                            if let Some(id) = child.as_str().filter(|id| !id.is_empty()) {
                                out.insert(canonical(id).to_owned());
                            }
                            continue;
                        }
                        "file_ids" => {
                            for id in child.as_array().into_iter().flatten() {
                                if let Some(id) = id.as_str().filter(|id| !id.is_empty()) {
                                    out.insert(canonical(id).to_owned());
                                }
                            }
                            continue;
                        }
                        // A tool call's arguments are the model's data.
                        "input" if tool_use => continue,
                        _ => {}
                    }
                }
                walk_references(child, gemini, out);
            }
        }
        _ => {}
    }
}

/// Whether `body` can name a file reference at all, without parsing it.
///
/// Every generation request passes through [`referenced_files`], and almost
/// none reference a file, so a byte search spares them the full parse. A key
/// can be spelled with `\u` escapes (`"file\u005fid"`) that the upstream
/// decodes like any other, so a body with any `\u` escape is parsed anyway:
/// this is a shortcut for the plain case, never a way around the check.
fn may_reference(body: &[u8], gemini: bool) -> bool {
    let contains = |needle: &[u8]| body.windows(needle.len()).any(|window| window == needle);
    let named = if gemini {
        contains(b"fileUri") || contains(b"file_uri")
    } else {
        contains(b"file_id")
    };
    named || contains(b"\\u")
}

/// The uploaded-file ids a request body references, in the client's dialect.
/// Empty for an operation that is not scanned and for a body that is not
/// JSON (a multipart form is not scanned).
pub fn referenced_files(operation: OperationKey, body: &[u8]) -> BTreeSet<String> {
    if !scans_references(operation.operation) || body.is_empty() {
        return BTreeSet::new();
    }
    if !may_reference(body, operation.dialect == Dialect::Gemini) {
        return BTreeSet::new();
    }
    serde_json::from_slice::<Value>(body)
        .map(|value| referenced_files_in(operation, &value))
        .unwrap_or_default()
}

/// The multipart boundary of a `multipart/form-data` content type.
fn multipart_boundary(headers: &http::HeaderMap) -> Option<String> {
    let content_type = headers.get(http::header::CONTENT_TYPE)?.to_str().ok()?;
    let mut parts = content_type.split(';');
    if !parts
        .next()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("multipart/form-data"))
    {
        return None;
    }
    parts
        .filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("boundary"))
        .map(|(_, v)| v.trim().trim_matches('"').to_owned())
        .filter(|b| !b.is_empty())
}

/// `name` and whether a `filename` is present, from a part's
/// content-disposition. An RFC 2231 `name*` counts as a name too.
fn part_name(headers: &http::HeaderMap) -> (Option<String>, bool) {
    let Some(value) = headers
        .get(http::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
    else {
        return (None, false);
    };
    let mut name = None;
    let mut file = false;
    for param in value.split(';').skip(1) {
        if let Some((k, v)) = param.trim().split_once('=') {
            let v = v.trim().trim_matches('"');
            match k.trim() {
                "name" => name = Some(v.to_owned()),
                // `utf-8''file_id`
                "name*" => name = Some(v.rsplit('\'').next().unwrap_or(v).to_owned()),
                "filename" | "filename*" => file = true,
                _ => {}
            }
        }
    }
    (name, file)
}

/// Whether a form field carries file ids by its name: `file_id`,
/// `file_ids`, `file_ids[]`, and the bracketed forms clients use for nested
/// fields (`input_reference[file_id]`, `images[0][file_id]`).
fn names_file_ids(name: &str) -> bool {
    let name = name.strip_suffix("[]").unwrap_or(name);
    matches!(name, "file_id" | "file_ids")
        || name.ends_with("[file_id]")
        || name.ends_with("[file_ids]")
        || name.ends_with(".file_id")
        || name.ends_with(".file_ids")
}

/// File ids in a multipart form: every text field named for file ids
/// (a JSON array value counts as several), and the JSON walk over any text
/// field whose value is a JSON object or array. File parts are skipped
/// unread. A form that cannot be read is refused: what the gateway cannot
/// parse, it cannot vouch for.
async fn multipart_references(
    operation: OperationKey,
    boundary: String,
    body: &[u8],
    limits: CodecLimits,
) -> CoreResult<BTreeSet<String>> {
    let unreadable = |e: gproxy_protocol::codec::CodecError| {
        CoreError::Transform(TransformError::new(
            gproxy_protocol::transform::TransformErrorKind::InvalidInput,
            "client.body",
            format!("multipart body must be readable to check its file references: {e}"),
        ))
    };
    let mut decoder = gproxy_protocol::codec::MultipartDecoder::new(
        HttpBody::Bytes(Bytes::copy_from_slice(body)),
        boundary,
        limits,
    )
    .map_err(unreadable)?;
    let gemini = operation.dialect == Dialect::Gemini;
    let mut out = BTreeSet::new();
    while let Some(part) = decoder.next_part().await.map_err(unreadable)? {
        let (name, file) = part_name(&part.headers);
        if file {
            drop(part);
            continue;
        }
        let value = read_http_body(part.body, limits)
            .await
            .map_err(unreadable)?;
        let text = String::from_utf8_lossy(&value);
        let text = text.trim();
        let json = (text.starts_with('{') || text.starts_with('['))
            .then(|| serde_json::from_str::<Value>(text).ok())
            .flatten();
        let named = name.as_deref().is_some_and(names_file_ids);
        match json {
            Some(Value::Array(items)) if named => {
                out.extend(
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|id| !id.is_empty())
                        .map(|id| canonical(id).to_owned()),
                );
            }
            Some(value) => walk_references(&value, gemini, &mut out),
            None if named && !text.is_empty() => {
                out.insert(canonical(text).to_owned());
            }
            None => {}
        }
    }
    Ok(out)
}

/// The uploaded-file ids a request references, JSON or multipart. The one
/// entry point execution and planning use; `limits` bounds a multipart read.
pub async fn request_references(
    operation: OperationKey,
    headers: &http::HeaderMap,
    body: &[u8],
    limits: CodecLimits,
) -> CoreResult<BTreeSet<String>> {
    if !scans_references(operation.operation) || body.is_empty() {
        return Ok(BTreeSet::new());
    }
    match multipart_boundary(headers) {
        Some(boundary) => {
            let gemini = operation.dialect == Dialect::Gemini;
            let contains =
                |needle: &[u8]| body.windows(needle.len()).any(|window| window == needle);
            // Field names are plain in a form; an RFC 2231 `name*` could
            // spell one otherwise, so such a form is always read.
            if !may_reference(body, gemini) && !contains(b"name*") {
                return Ok(BTreeSet::new());
            }
            multipart_references(operation, boundary, body, limits).await
        }
        None => Ok(referenced_files(operation, body)),
    }
}

/// The codec limits a request-body scan reads under: the operation's request
/// cap for the whole body and for any one part.
fn scan_limits(limits: &crate::ExecutionLimits) -> CodecLimits {
    let mut codec = limits.codec();
    codec.max_body_bytes = limits.max_request_body_bytes;
    codec.max_part_bytes = limits.max_request_body_bytes;
    codec
}

/// The same, over an already parsed body (a Responses WebSocket turn).
pub fn referenced_files_in(operation: OperationKey, body: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if scans_references(operation.operation) {
        walk_references(body, operation.dialect == Dialect::Gemini, &mut out);
    }
    out
}

/// The refusal `Resources` raises for a file id the caller's scope does not
/// own. It travels as the source of a `CapabilityError` and then of the
/// conversion's `TransformError`, both of which keep their source, so the
/// attempt loop finds it again ([`not_owned_in`]) and answers the same
/// [`CoreError::ResourceNotFound`] a pre-dispatch refusal does — instead of
/// the "host capability" failure the capability kind alone would map to.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} `{id}` not found")]
pub struct NotOwned {
    pub kind: &'static str,
    pub id: String,
}

impl From<NotOwned> for CoreError {
    fn from(error: NotOwned) -> Self {
        CoreError::ResourceNotFound {
            kind: error.kind,
            id: error.id,
        }
    }
}

/// The [`NotOwned`] anywhere in `error`'s source chain.
pub fn not_owned_in<'e>(error: &'e (dyn std::error::Error + 'static)) -> Option<&'e NotOwned> {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(found) = error.downcast_ref::<NotOwned>() {
            return Some(found);
        }
        current = error.source();
    }
    None
}

/// Where a request's referenced files let it run on one provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Only publications (or nothing upstream): any credential will do.
    Anywhere,
    /// Every referenced upstream file lives on this credential.
    Credential(String),
}

/// Where a resource the caller owns lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBinding {
    pub provider_id: String,
    pub credential_id: String,
}

impl<C: BatchConnectionTrait + Send + Sync> Core<C> {
    fn bindings(&self) -> gproxy_store::Repository<'_, C, resource_binding::Entity> {
        self.store().resource_bindings()
    }

    async fn owned_rows(
        &self,
        columns: Vec<String>,
        kind: OwnedKind,
        public_id: Option<&str>,
        credential_id: Option<&str>,
    ) -> CoreResult<Vec<resource_binding::Model>> {
        let mut query = resource_binding::Entity::find()
            .filter(resource_binding::Column::Scope.is_in(columns))
            .filter(resource_binding::Column::Kind.eq(kind.kind()))
            .filter(resource_binding::Column::Generation.eq(0i64));
        if let Some(id) = public_id {
            query = query.filter(resource_binding::Column::PublicId.eq(id));
        }
        if let Some(credential) = credential_id {
            query = query.filter(resource_binding::Column::CredentialId.eq(credential));
        }
        Ok(self.bindings().query(query).await?)
    }

    /// For a host walking several providers: where the caller's resources let
    /// this request run, per provider among `providers`. `Ok(None)` when the
    /// request names no owned resource, so nothing is narrowed. Otherwise a
    /// provider is in the map when everything the request names — the file or
    /// video in the path of a read or delete, or every file `body` references
    /// — is the scope's there, with the credential it must run on (`None`:
    /// any). An empty map: no provider has it all.
    ///
    /// Keep only the targets in the map, each narrowed to its credential.
    /// Execution checks again, so this is planning, not the enforcement.
    /// `body` is `None` when the host could not buffer it; execution then
    /// buffers and checks it on the one target it can still try.
    // The request's parts as a host holds them before a `WireRequest` exists;
    // bundling them would only move the same eight names into a struct.
    #[allow(clippy::too_many_arguments)]
    pub async fn owned_resource_bindings<'p>(
        &self,
        scope: &str,
        operation: OperationKey,
        path: &str,
        query: Option<&str>,
        headers: &http::HeaderMap,
        body: Option<&[u8]>,
        providers: impl IntoIterator<Item = &'p str>,
    ) -> CoreResult<Option<BTreeMap<String, Option<String>>>> {
        let providers: Vec<&str> = providers.into_iter().collect();
        if upload::session_token(operation, query).is_some() {
            return Ok(Some(
                self.upload_session_target(scope, operation, query)
                    .await?
                    .into_iter()
                    .filter(|(provider, _)| providers.contains(&provider.as_str()))
                    .map(|(provider, credential)| (provider, Some(credential)))
                    .collect(),
            ));
        }
        if let Some(kind) = OwnedAccess::of(operation.operation).and_then(OwnedAccess::addressed) {
            let id = path_id(kind, path)?;
            let columns: Vec<String> = providers
                .iter()
                .map(|provider| scope_column(scope, provider))
                .collect();
            if columns.is_empty() {
                return Ok(Some(BTreeMap::new()));
            }
            let rows = self.owned_rows(columns, kind, Some(&id), None).await?;
            return Ok(Some(
                rows.into_iter()
                    .map(|row| (row.provider_id, Some(row.credential_id)))
                    .collect(),
            ));
        }
        let limits = scan_limits(&self.snapshot().limits.for_operation(operation.operation));
        let ids = request_references(operation, headers, body.unwrap_or_default(), limits).await?;
        if ids.is_empty() {
            return Ok(None);
        }
        let mut placed = BTreeMap::new();
        for provider in providers {
            match self.placement(scope, provider, &ids).await {
                Ok(Placement::Anywhere) => {
                    placed.insert(provider.to_owned(), None);
                }
                Ok(Placement::Credential(credential)) => {
                    placed.insert(provider.to_owned(), Some(credential));
                }
                Err(CoreError::ResourceNotFound { .. } | CoreError::InvalidTarget(_)) => {}
                Err(other) => return Err(other),
            }
        }
        Ok(Some(placed))
    }

    /// Where `ids` (uploaded-file ids a request references) let it run on
    /// `provider` in `scope`: each must be a file the scope created there, on
    /// one shared credential, or a publication core minted for the scope.
    pub async fn placement(
        &self,
        scope: &str,
        provider: &str,
        ids: &BTreeSet<String>,
    ) -> CoreResult<Placement> {
        placement_in(self.store(), scope, provider, ids).await
    }

    /// The credential holding file `id` that `scope` uploaded on `provider`,
    /// or `None` when the scope owns no such file there.
    pub async fn owned_file_credential(
        &self,
        scope: &str,
        provider: &str,
        id: &str,
    ) -> CoreResult<Option<String>> {
        Ok(self
            .owned_rows(
                vec![scope_column(scope, provider)],
                OwnedKind::File,
                Some(canonical(id)),
                None,
            )
            .await?
            .into_iter()
            .next()
            .map(|row| row.credential_id))
    }

    /// The enforcement for files referenced inside a request body: every id
    /// must be the scope's on this provider (or its publication), and the
    /// request is pinned to the credential holding them.
    pub(crate) async fn bind_referenced_files(
        &self,
        context: Arc<RequestContext>,
        headers: &http::HeaderMap,
        body: &[u8],
    ) -> CoreResult<Arc<RequestContext>> {
        let limits = scan_limits(
            &context
                .snapshot
                .limits
                .for_operation(context.operation.operation),
        );
        let ids = request_references(context.operation, headers, body, limits).await?;
        if ids.is_empty() {
            return Ok(context);
        }
        let provider_id = &context.target.provider.entity.id;
        let Placement::Credential(credential_id) =
            self.placement(&context.scope, provider_id, &ids).await?
        else {
            return Ok(context);
        };
        let credential = context
            .target
            .credentials
            .iter()
            .find(|credential| credential.id == credential_id)
            .cloned()
            .ok_or_else(|| CoreError::ResourceNotFound {
                kind: OwnedKind::File.noun(),
                id: ids.iter().next().cloned().unwrap_or_default(),
            })?;
        Ok(pin(context, credential))
    }

    /// The enforcement: refuse a read or delete of an id the request's scope
    /// does not own on this provider, and pin an owned one to the credential
    /// that holds it. Any other operation passes through unchanged.
    pub(crate) async fn bind_owned_resource(
        &self,
        context: Arc<RequestContext>,
        path: &str,
    ) -> CoreResult<Arc<RequestContext>> {
        let Some(kind) =
            OwnedAccess::of(context.operation.operation).and_then(OwnedAccess::addressed)
        else {
            return Ok(context);
        };
        let id = path_id(kind, path)?;
        let provider_id = &context.target.provider.entity.id;
        let not_found = || CoreError::ResourceNotFound {
            kind: kind.noun(),
            id: id.clone(),
        };
        let row = self
            .owned_rows(
                vec![scope_column(&context.scope, provider_id)],
                kind,
                Some(&id),
                None,
            )
            .await?
            .into_iter()
            .next()
            .ok_or_else(not_found)?;
        // A credential the caller may no longer spend reads the same as a
        // resource it never had: the id stays unreachable either way.
        let credential = context
            .target
            .credentials
            .iter()
            .find(|credential| credential.id == row.credential_id)
            .cloned()
            .ok_or_else(not_found)?;
        Ok(pin(context, credential))
    }

    /// After a successful answer on `credential_id`: record a created
    /// resource, forget a deleted one, or filter a list to what the scope
    /// owns. `converted` says whether the answer came out of a protocol
    /// conversion, whose video ids are core's own rather than the upstream's.
    /// The body is buffered when a create or list needs to be read.
    pub(crate) async fn settle_owned_resource(
        &self,
        request: &RequestContext,
        credential_id: &str,
        path: &str,
        converted: bool,
        response: &mut WireResponse<HttpBody>,
        limits: CodecLimits,
    ) -> CoreResult<()> {
        let Some(access) = OwnedAccess::of(request.operation.operation) else {
            return Ok(());
        };
        if !response.status.is_success() {
            return Ok(());
        }
        let provider_id = request.target.provider.entity.id.as_str();
        let column = scope_column(&request.scope, provider_id);
        match access {
            OwnedAccess::Read(_) => Ok(()),
            OwnedAccess::Delete(kind) => {
                let id = path_id(kind, path)?;
                let ids: Vec<String> = self
                    .owned_rows(vec![column], kind, Some(&id), Some(credential_id))
                    .await?
                    .into_iter()
                    .map(|row| row.id)
                    .collect();
                if !ids.is_empty() {
                    self.bindings().delete_many(&ids).await?;
                }
                Ok(())
            }
            OwnedAccess::Create(kind) => {
                // A Gemini resumable start creates no file yet: its upload
                // URL becomes a gateway session (`upload`).
                if self
                    .open_upload_session(request, credential_id, response)
                    .await?
                {
                    return Ok(());
                }
                // An intermediate resumable chunk: still no file.
                if response
                    .headers
                    .get("x-goog-upload-status")
                    .is_some_and(|status| status.as_bytes().eq_ignore_ascii_case(b"active"))
                {
                    return Ok(());
                }
                let body = buffer(response, limits).await?;
                // Gemini's resumable start answers with no body: the file is
                // finished at the upload URL, out of the gateway's sight.
                let Some(id) = serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|value| created_id(kind, &value))
                else {
                    tracing::warn!(
                        provider = provider_id,
                        operation = ?request.operation.operation,
                        "created resource carries no readable id; it is not registered and stays unreachable through the gateway"
                    );
                    return Ok(());
                };
                let existing = self
                    .owned_rows(vec![column.clone()], kind, Some(&id), None)
                    .await?;
                if !existing.is_empty() {
                    return Ok(());
                }
                let now = crate::api::lifecycle::now_ms();
                let upstream_id = (!(converted && kind == OwnedKind::Video)).then(|| id.clone());
                let summary = serde_json::json!({
                    "user_id": request.attribution.user_id,
                    "api_key_id": request.attribution.api_key_id,
                    "operation": request.operation.operation.id(),
                });
                self.bindings()
                    .create_many(vec![resource_binding::ActiveModel {
                        id: Set(crate::ids::random_id()),
                        scope: Set(column),
                        kind: Set(kind.kind().to_owned()),
                        public_id: Set(id),
                        generation: Set(0),
                        assignment_id: Set(None),
                        provider_id: Set(provider_id.to_owned()),
                        upstream_id: Set(upstream_id),
                        user_id: Set(None),
                        credential_id: Set(credential_id.to_owned()),
                        parent_binding_id: Set(None),
                        secret: Set(None),
                        summary: Set(summary),
                        file_id: Set(None),
                        created_at_ms: Set(now),
                        updated_at_ms: Set(now),
                        expires_at_ms: Set(None),
                    }])
                    .await?;
                Ok(())
            }
            OwnedAccess::List(kind) => {
                let body = buffer(response, limits).await?;
                let owned: BTreeSet<String> = self
                    .owned_rows(vec![column], kind, None, Some(credential_id))
                    .await?
                    .into_iter()
                    .map(|row| row.public_id)
                    .collect();
                if let Some(filtered) = filter_list(kind, request.operation.dialect, &body, &owned)?
                {
                    response.body = HttpBody::Bytes(Bytes::from(filtered));
                    response.headers.remove(http::header::CONTENT_LENGTH);
                    response.headers.remove(http::header::CONTENT_ENCODING);
                }
                Ok(())
            }
        }
    }
}

/// Where `ids` (uploaded-file ids a request references) let it run on
/// `provider` in `scope`: each must be a file the scope created there, on
/// one shared credential, or a publication core minted for the scope.
pub(crate) async fn placement_in<C: BatchConnectionTrait + Send + Sync>(
    store: &gproxy_store::Store<C>,
    scope: &str,
    provider: &str,
    ids: &BTreeSet<String>,
) -> CoreResult<Placement> {
    let column = scope_column(scope, provider);
    let rows = store
        .resource_bindings()
        .query(
            resource_binding::Entity::find()
                .filter(resource_binding::Column::Scope.eq(column.as_str()))
                .filter(resource_binding::Column::Kind.eq(FILE_KIND))
                .filter(resource_binding::Column::Generation.eq(0i64))
                .filter(resource_binding::Column::PublicId.is_in(ids.iter().cloned())),
        )
        .await?;
    let mut missing: BTreeSet<&String> = ids.iter().collect();
    let mut credentials = BTreeSet::new();
    for row in &rows {
        missing.remove(&row.public_id);
        credentials.insert(row.credential_id.clone());
    }
    if !missing.is_empty() {
        // A publication is addressed by its binding id and owned by the
        // same scope column (`Resources` resolves it locally).
        let published = store
            .resource_bindings()
            .query(
                resource_binding::Entity::find()
                    .filter(resource_binding::Column::Scope.eq(column.as_str()))
                    .filter(resource_binding::Column::Kind.eq(crate::capability::PUBLICATION_KIND))
                    .filter(
                        resource_binding::Column::Id.is_in(missing.iter().map(|id| (*id).clone())),
                    ),
            )
            .await?;
        for row in &published {
            missing.remove(&row.id);
        }
    }
    if let Some(id) = missing.into_iter().next() {
        return Err(CoreError::ResourceNotFound {
            kind: OwnedKind::File.noun(),
            id: id.clone(),
        });
    }
    let mut credentials = credentials.into_iter();
    match (credentials.next(), credentials.next()) {
        (None, _) => Ok(Placement::Anywhere),
        (Some(credential), None) => Ok(Placement::Credential(credential)),
        (Some(_), Some(_)) => Err(CoreError::InvalidTarget(
            "the referenced files live on different upstream credentials; \
             one request can only use files uploaded on the same one"
                .into(),
        )),
    }
}

/// Narrow a request to the one credential holding what it names.
fn pin(
    context: Arc<RequestContext>,
    credential: Arc<crate::CredentialData>,
) -> Arc<RequestContext> {
    if context.target.credentials.len() == 1 {
        return context;
    }
    let mut next = (*context).clone();
    next.target.credentials = vec![credential];
    // Affinity or an agent assignment must never steer an existing resource
    // onto another account; the realtime continuation does the same.
    next.session = None;
    Arc::new(next)
}

/// Read the whole response body and leave a buffered copy in its place.
async fn buffer(response: &mut WireResponse<HttpBody>, limits: CodecLimits) -> CoreResult<Bytes> {
    let body = std::mem::replace(&mut response.body, HttpBody::Bytes(Bytes::new()));
    let bytes = read_http_body(body, limits).await.map_err(|e| {
        CoreError::Transform(TransformError::invalid_result(
            "owned.response",
            e.to_string(),
        ))
    })?;
    response.body = HttpBody::Bytes(bytes.clone());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_and_video_operation_is_governed() {
        use OwnedKind::{File, Video};
        for (operation, access) in [
            (Operation::CreateFile, OwnedAccess::Create(File)),
            (Operation::RetrieveFile, OwnedAccess::Read(File)),
            (Operation::RetrieveFileContent, OwnedAccess::Read(File)),
            (Operation::DeleteFile, OwnedAccess::Delete(File)),
            (Operation::ListFiles, OwnedAccess::List(File)),
            (Operation::CreateVideo, OwnedAccess::Create(Video)),
            (Operation::RetrieveVideo, OwnedAccess::Read(Video)),
            (Operation::DownloadVideoContent, OwnedAccess::Read(Video)),
            (Operation::DeleteVideo, OwnedAccess::Delete(Video)),
            (Operation::ListVideos, OwnedAccess::List(Video)),
        ] {
            assert_eq!(OwnedAccess::of(operation), Some(access), "{operation:?}");
        }
        assert_eq!(OwnedAccess::of(Operation::GenerateContent), None);
    }

    #[test]
    fn path_ids_are_canonical_in_every_dialect() {
        let file = OwnedKind::File;
        assert_eq!(path_id(file, "/v1/files/file-abc").unwrap(), "file-abc");
        assert_eq!(
            path_id(file, "/v1/files/file-abc/content").unwrap(),
            "file-abc"
        );
        assert_eq!(path_id(file, "/v1beta/files/abc").unwrap(), "abc");
        assert_eq!(path_id(file, "/v1beta/files/abc:download").unwrap(), "abc");
        assert_eq!(path_id(file, "/v1/files/a%20b").unwrap(), "a b");
        assert_eq!(
            path_id(OwnedKind::Video, "/v1/videos/video_1/content").unwrap(),
            "video_1"
        );
        assert!(path_id(file, "/v1/files").is_err());
        assert!(path_id(file, "/v1/files/").is_err());
    }

    #[test]
    fn created_ids_are_read_from_each_dialect() {
        let json = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        let file = OwnedKind::File;
        let video = OwnedKind::Video;
        assert_eq!(
            created_id(file, &json(r#"{"id":"file-1","object":"file"}"#)).as_deref(),
            Some("file-1")
        );
        assert_eq!(
            created_id(file, &json(r#"{"file":{"name":"files/abc"}}"#)).as_deref(),
            Some("abc")
        );
        assert_eq!(
            created_id(video, &json(r#"{"id":"video_1"}"#)).as_deref(),
            Some("video_1")
        );
        assert_eq!(
            created_id(video, &json(r#"{"request_id":"r1"}"#)).as_deref(),
            Some("r1")
        );
        assert_eq!(created_id(file, &json(r#"{}"#)), None);
    }

    #[test]
    fn an_escaped_key_is_still_a_reference() {
        let key = OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAi,
        };
        let escaped = br#"{"input":[{"type":"input_file","file\u005fid":"file-x"}]}"#;
        assert_eq!(
            referenced_files(key, escaped)
                .into_iter()
                .collect::<Vec<_>>(),
            ["file-x"]
        );
        assert!(referenced_files(key, br#"{"input":"hello"}"#).is_empty());
    }

    fn refs(operation: Operation, dialect: Dialect, body: Value) -> Vec<String> {
        referenced_files(
            OperationKey { operation, dialect },
            &serde_json::to_vec(&body).unwrap(),
        )
        .into_iter()
        .collect()
    }

    #[test]
    fn file_references_are_found_in_every_dialect() {
        use serde_json::json;
        let generate = Operation::GenerateContent;
        // OpenAI Chat file part, Responses input_file / input_image and a
        // code-interpreter container.
        assert_eq!(
            refs(
                generate,
                Dialect::OpenAiChat,
                json!({"messages": [{"role": "user", "content": [
                    {"type": "file", "file": {"file_id": "file-chat"}}
                ]}]})
            ),
            ["file-chat"]
        );
        assert_eq!(
            refs(
                generate,
                Dialect::OpenAi,
                json!({
                    "input": [{"role": "user", "content": [
                        {"type": "input_file", "file_id": "file-a"},
                        {"type": "input_image", "file_id": "file-b"}
                    ]}],
                    "tools": [{"type": "code_interpreter", "container": {"type": "auto", "file_ids": ["file-c"]}}]
                })
            ),
            ["file-a", "file-b", "file-c"]
        );
        // Claude document source and container upload.
        assert_eq!(
            refs(
                generate,
                Dialect::Claude,
                json!({"messages": [{"role": "user", "content": [
                    {"type": "document", "source": {"type": "file", "file_id": "file_doc"}},
                    {"type": "container_upload", "file_id": "file_up"}
                ]}]})
            ),
            ["file_doc", "file_up"]
        );
        // Gemini Files API URIs, bare or as the upload's URL; other URIs are
        // not Files API files.
        assert_eq!(
            refs(
                generate,
                Dialect::Gemini,
                json!({"contents": [{"parts": [
                    {"fileData": {"fileUri": "https://generativelanguage.googleapis.com/v1beta/files/abc?x=1"}},
                    {"file_data": {"file_uri": "files/def"}},
                    {"fileData": {"fileUri": "gs://bucket/files/ghi"}},
                    {"fileData": {"fileUri": "https://www.youtube.com/watch?v=1"}}
                ]}]})
            ),
            ["abc", "def"]
        );
        // A video reference by file id.
        assert_eq!(
            refs(
                Operation::CreateVideo,
                Dialect::OpenAi,
                json!({
                    "prompt": "p", "input_reference": {"file_id": "file-ref"}
                })
            ),
            ["file-ref"]
        );
    }

    #[test]
    fn caller_data_that_looks_like_a_reference_is_not_one() {
        use serde_json::json;
        let generate = Operation::GenerateContent;
        // A tool call's input, a function's parameters schema and metadata.
        assert!(
            refs(generate, Dialect::Claude, json!({
                "messages": [{"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t", "name": "f", "input": {"file_id": "mine-to-pass"}}
                ]}],
                "tools": [{"name": "f", "input_schema": {"properties": {"file_id": {"type": "string"}}}}],
                "metadata": {"file_id": "x"}
            }))
            .is_empty()
        );
        assert!(
            refs(
                generate,
                Dialect::Gemini,
                json!({"contents": [{"parts": [
                    {"functionCall": {"name": "f", "args": {"fileUri": "files/abc"}}},
                    {"functionResponse": {"name": "f", "response": {"fileUri": "files/abc"}}}
                ]}]})
            )
            .is_empty()
        );
        // File CRUD addresses by path, and a body that is not JSON is not read.
        assert!(
            refs(
                Operation::CreateFile,
                Dialect::OpenAi,
                json!({"file_id": "x"})
            )
            .is_empty()
        );
        assert!(
            referenced_files(
                OperationKey {
                    operation: generate,
                    dialect: Dialect::OpenAi
                },
                b"--boundary\r\nfile_id"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_resources_refusal_is_found_through_the_conversion_error() {
        use gproxy_protocol::capability::{
            CapabilityError, CapabilityErrorKind, CapabilityErrorStage,
        };
        let not_owned = NotOwned {
            kind: "file",
            id: "file-x".into(),
        };
        let capability = CapabilityError::with_source(
            CapabilityErrorKind::NotFound,
            CapabilityErrorStage::Start,
            not_owned.to_string(),
            not_owned.clone(),
        );
        let transform = TransformError::from(capability);
        assert_eq!(not_owned_in(&transform), Some(&not_owned));
        assert!(matches!(
            CoreError::from(not_owned),
            CoreError::ResourceNotFound { id, .. } if id == "file-x"
        ));
        let unrelated = TransformError::invalid_result("x", "y");
        assert_eq!(not_owned_in(&unrelated), None);
    }

    async fn form_refs(fields: &[(&str, Option<&str>, &str)]) -> CoreResult<Vec<String>> {
        let mut body = String::new();
        for (name, filename, value) in fields {
            body.push_str("--B\r\n");
            body.push_str(&format!("content-disposition: form-data; name=\"{name}\""));
            if let Some(filename) = filename {
                body.push_str(&format!("; filename=\"{filename}\""));
            }
            body.push_str("\r\n\r\n");
            body.push_str(value);
            body.push_str("\r\n");
        }
        body.push_str("--B--\r\n");
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("multipart/form-data; boundary=B"),
        );
        let limits = crate::ExecutionLimits::default().codec();
        request_references(
            OperationKey {
                operation: Operation::EditImage,
                dialect: Dialect::OpenAi,
            },
            &headers,
            body.as_bytes(),
            limits,
        )
        .await
        .map(|ids| ids.into_iter().collect())
    }

    #[tokio::test]
    async fn multipart_fields_naming_files_are_references() {
        assert_eq!(
            form_refs(&[
                ("prompt", None, "a cat"),
                ("file_id", None, "file-a"),
                ("file_ids[]", None, "file-b"),
                ("file_ids", None, r#"["file-c","file-d"]"#),
                ("input_reference[file_id]", None, "file-e"),
                (
                    "tools",
                    None,
                    r#"[{"type":"code_interpreter","container":{"file_ids":["file-f"]}}]"#
                ),
                // A file part's bytes are content, never a reference.
                ("image", Some("file_id.png"), "file_id=nope"),
            ])
            .await
            .unwrap(),
            ["file-a", "file-b", "file-c", "file-d", "file-e", "file-f"]
        );
        assert!(
            form_refs(&[("prompt", None, "no references")])
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn lists_keep_only_owned_items_and_their_cursors() {
        let owned: BTreeSet<String> = ["mine".to_owned()].into();
        let body = br#"{"object":"list","data":[{"id":"mine"},{"id":"theirs"}],"has_more":true,"last_id":"theirs"}"#;
        let out = filter_list(OwnedKind::File, Dialect::OpenAi, body, &owned)
            .unwrap()
            .unwrap();
        let out: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(out["data"], serde_json::json!([{"id": "mine"}]));
        assert_eq!(out["has_more"], true);
        assert_eq!(out["last_id"], "theirs");

        let gemini =
            br#"{"files":[{"name":"files/mine"},{"name":"files/theirs"}],"nextPageToken":"t"}"#;
        let out = filter_list(OwnedKind::File, Dialect::Gemini, gemini, &owned)
            .unwrap()
            .unwrap();
        let out: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(out["files"], serde_json::json!([{"name": "files/mine"}]));
        assert_eq!(out["nextPageToken"], "t");

        // Gemini leaves `files` out of an empty page; nothing to filter.
        assert!(
            filter_list(OwnedKind::File, Dialect::Gemini, b"{}", &owned)
                .unwrap()
                .is_none()
        );
        // Anything that is not a readable list is refused, never passed on.
        assert!(filter_list(OwnedKind::File, Dialect::OpenAi, b"{}", &owned).is_err());
        assert!(filter_list(OwnedKind::Video, Dialect::OpenAi, b"[]", &owned).is_err());
        assert!(filter_list(OwnedKind::Video, Dialect::OpenAi, b"nope", &owned).is_err());
    }
}
