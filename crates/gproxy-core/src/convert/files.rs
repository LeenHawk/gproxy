//! files conversion family: the client's file CRUD in one dialect against an
//! upstream that speaks another. Metadata crosses through the typed pairwise
//! mappings in `transform::files`; bytes move through `adapt::files`, which
//! never retries an upload. Content downloads pass the upstream body through
//! untouched.
//!
//! Deliberate boundaries:
//! * File ids are the upstream's ids. Gemini clients see them as `files/{id}`,
//!   the other dialects see them bare.
//! * A client filter the target cannot express (OpenAI `purpose`/`order`,
//!   Claude `before_id`/`scope_id`) is rejected rather than silently dropped.
//! * Listing follows upstream pages to a bounded depth and applies the client's
//!   `limit` itself; the continuation cursor is the last upstream id.
//! * Files reaching OpenAI from a Claude or Gemini upstream report purpose
//!   `user_data`: the only purpose those file APIs model.
//! * Gemini clients may upload as a multipart form (`metadata` + `file`); the
//!   resumable protocol is a multi-request exchange this family does not drive.

use super::{Call, Converted, endpoints};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    adapt::{
        JsonInvocation,
        files::{
            self as adapt, FileCrudLimits, FileFailure, FileOperationError, GeminiUploadProgress,
            UploadError, UploadFailure,
        },
    },
    capability::Upstream,
    codec::{
        CodecError, CodecErrorKind, CodecLimits, MultipartDecoder, encode_json, read_http_body,
    },
    connection::{Bytes, MultipartPart},
    transform::{
        TransformError, TransformErrorKind,
        files::{
            FileFacts, FilePurpose, FileStatusFacts, claude_to_gemini, claude_to_openai,
            gemini_to_claude, gemini_to_openai, openai_to_claude, openai_to_gemini,
        },
    },
    wire::{claude::files as c, gemini::files as g, openai::files as o},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

/// The three file API shapes; OpenAI Chat shares OpenAI's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    OpenAi,
    Claude,
    Gemini,
}

impl Family {
    fn of(dialect: Dialect) -> Result<Self, TransformError> {
        match dialect {
            Dialect::OpenAi | Dialect::OpenAiChat => Ok(Self::OpenAi),
            Dialect::Claude => Ok(Self::Claude),
            Dialect::Gemini => Ok(Self::Gemini),
            Dialect::OpenAiResponsesWebSocket => Err(TransformError::unsupported(
                "files",
                "the Responses WebSocket dialect has no files API",
            )),
        }
    }
}

fn codec(error: CodecError, context: &'static str) -> TransformError {
    let kind = match error.kind() {
        CodecErrorKind::Limit => TransformErrorKind::Limit,
        CodecErrorKind::Transport => TransformErrorKind::Host,
        _ => TransformErrorKind::InvalidInput,
    };
    TransformError::with_source(kind, context, error.to_string(), error)
}

/// One step's outcome: a typed value or the upstream's own rejection.
enum Step<T> {
    Ok(T),
    Rejected(WireResponse<HttpBody>),
}

impl<T> From<JsonInvocation<T>> for Step<WireResponse<T>> {
    fn from(value: JsonInvocation<T>) -> Self {
        match value {
            JsonInvocation::Success(response) => Step::Ok(response),
            JsonInvocation::Rejected(response) => Step::Rejected(response),
        }
    }
}

fn list_failure<T>(error: FileOperationError) -> Result<Step<T>, TransformError> {
    match error.failure {
        FileFailure::Transform(error) => Err(error),
        FileFailure::Rejected(response) => Ok(Step::Rejected(*response)),
    }
}

fn upload_failure<T>(error: UploadError) -> Result<Step<T>, TransformError> {
    match error.failure {
        UploadFailure::Transform(error) => Err(error),
        UploadFailure::Rejected(response) => Ok(Step::Rejected(*response)),
    }
}

fn json_response<T: Serialize>(
    status: StatusCode,
    value: &T,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    let body = encode_json(value, limits).map_err(|e| codec(e, "client.response"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(Converted::Success(WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(body),
    }))
}

fn template(method: Method, path: String) -> WireRequest<()> {
    WireRequest {
        method,
        path,
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}

/// The file id named by the client's path, bare of any dialect prefix.
fn client_file_id<C>(
    call: &Call<'_, C>,
    family: Family,
    content: bool,
) -> Result<String, TransformError> {
    let mut path = call.request.path;
    if content {
        path = path.strip_suffix("/content").ok_or_else(|| {
            TransformError::shape("files.path", "content download path must end in /content")
        })?;
    }
    let raw = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let id = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_err(|e| TransformError::shape("files.path", e.to_string()))?
        .into_owned();
    if id.is_empty() || id == "files" {
        return Err(TransformError::shape(
            "files.path",
            "file id missing from path",
        ));
    }
    let _ = family;
    Ok(id)
}

/// The id form the upstream dialect names a file by.
fn upstream_name(id: &str, target: Family) -> String {
    match target {
        Family::Gemini => format!("files/{id}"),
        Family::OpenAi | Family::Claude => id.to_owned(),
    }
}

/// The id form the client dialect expects back.
fn client_name(id: &str, client: Family) -> String {
    let bare = id.strip_prefix("files/").unwrap_or(id);
    match client {
        Family::Gemini => format!("files/{bare}"),
        Family::OpenAi | Family::Claude => bare.to_owned(),
    }
}

fn openai_purpose(purpose: &o::FilePurpose) -> FilePurpose {
    match purpose {
        o::FilePurpose::Assistants => FilePurpose::Assistants,
        o::FilePurpose::Batch => FilePurpose::Batch,
        o::FilePurpose::FineTune => FilePurpose::FineTune,
        o::FilePurpose::Vision => FilePurpose::Vision,
        o::FilePurpose::UserData => FilePurpose::UserData,
        o::FilePurpose::AssistantsOutput => FilePurpose::AssistantsOutput,
        o::FilePurpose::BatchOutput => FilePurpose::BatchOutput,
        o::FilePurpose::FineTuneResults => FilePurpose::FineTuneResults,
        #[allow(unreachable_patterns)]
        other => FilePurpose::Other(format!("{other:?}")),
    }
}

fn openai_status(status: &o::FileStatus) -> FileStatusFacts {
    match status {
        o::FileStatus::Uploaded => FileStatusFacts::Uploaded,
        o::FileStatus::Processed => FileStatusFacts::Active,
        o::FileStatus::Error => FileStatusFacts::Failed,
        #[allow(unreachable_patterns)]
        _ => FileStatusFacts::Unknown,
    }
}

fn gemini_status(state: Option<&g::FileState>) -> Option<FileStatusFacts> {
    state.map(|state| match state {
        g::FileState::Processing => FileStatusFacts::Processing,
        g::FileState::Active => FileStatusFacts::Active,
        g::FileState::Failed => FileStatusFacts::Failed,
        g::FileState::StateUnspecified => FileStatusFacts::Unknown,
        #[allow(unreachable_patterns)]
        _ => FileStatusFacts::Unknown,
    })
}

/// One upstream file in its native shape.
enum NativeFile {
    OpenAi(o::FileObject),
    Claude(c::FileMetadata),
    Gemini(Box<g::File>),
}

impl NativeFile {
    fn id(&self) -> Result<String, TransformError> {
        Ok(match self {
            Self::OpenAi(file) => file.id.clone(),
            Self::Claude(file) => file.id.clone(),
            Self::Gemini(file) => file
                .name
                .clone()
                .flatten()
                .ok_or_else(|| TransformError::missing_metadata("file.name"))?,
        })
    }

    /// Facts the upstream response establishes, in the client's id form.
    /// `known` carries what the client itself supplied (upload filename, MIME).
    fn facts(&self, client: Family, known: &FileFacts) -> Result<FileFacts, TransformError> {
        let mut facts = known.clone();
        facts.id = Some(client_name(&self.id()?, client));
        match self {
            Self::OpenAi(file) => {
                facts.filename.get_or_insert_with(|| file.filename.clone());
                facts.bytes = u64::try_from(file.bytes).ok();
                facts.created_at = Some(file.created_at.to_string());
                facts.expires_at = file.expires_at.flatten().map(|v| v.to_string());
                facts.purpose = Some(openai_purpose(&file.purpose));
                facts.status = Some(openai_status(&file.status));
            }
            Self::Claude(file) => {
                facts.filename.get_or_insert_with(|| file.filename.clone());
                facts.mime.get_or_insert_with(|| file.mime_type.clone());
                facts.bytes = u64::try_from(file.size_bytes).ok();
                facts.created_at = Some(file.created_at.clone());
                facts.purpose.get_or_insert(FilePurpose::UserData);
                facts.status = Some(FileStatusFacts::Active);
            }
            Self::Gemini(file) => {
                if let Some(name) = file.display_name.clone().flatten() {
                    facts.filename.get_or_insert(name);
                }
                if let Some(mime) = file.mime_type.clone().flatten() {
                    facts.mime.get_or_insert(mime);
                }
                facts.created_at = file.create_time.clone().flatten();
                facts.expires_at = file.expiration_time.clone().flatten();
                facts.purpose.get_or_insert(FilePurpose::UserData);
                facts.status = gemini_status(file.state.as_ref().and_then(Option::as_ref));
            }
        }
        Ok(facts)
    }

    /// The client-dialect shape of this file.
    fn convert(&self, client: Family, known: &FileFacts) -> Result<ClientFile, TransformError> {
        let facts = self.facts(client, known)?;
        Ok(match (self, client) {
            (Self::OpenAi(file), Family::OpenAi) => ClientFile::OpenAi(file.clone()),
            (Self::OpenAi(file), Family::Claude) => {
                ClientFile::Claude(openai_to_claude(file, &facts)?.value)
            }
            (Self::OpenAi(file), Family::Gemini) => {
                ClientFile::Gemini(Box::new(openai_to_gemini(file, &facts)?.value))
            }
            (Self::Claude(file), Family::OpenAi) => {
                ClientFile::OpenAi(claude_to_openai(file, &facts)?.value)
            }
            (Self::Claude(file), Family::Claude) => ClientFile::Claude(file.clone()),
            (Self::Claude(file), Family::Gemini) => {
                ClientFile::Gemini(Box::new(claude_to_gemini(file, &facts)?.value))
            }
            (Self::Gemini(file), Family::OpenAi) => {
                ClientFile::OpenAi(gemini_to_openai(file, &facts)?.value)
            }
            (Self::Gemini(file), Family::Claude) => {
                ClientFile::Claude(gemini_to_claude(file, &facts)?.value)
            }
            (Self::Gemini(file), Family::Gemini) => ClientFile::Gemini(file.clone()),
        })
    }
}

enum ClientFile {
    OpenAi(o::FileObject),
    Claude(c::FileMetadata),
    Gemini(Box<g::File>),
}

impl ClientFile {
    fn id(&self) -> String {
        match self {
            Self::OpenAi(file) => file.id.clone(),
            Self::Claude(file) => file.id.clone(),
            Self::Gemini(file) => file.name.clone().flatten().unwrap_or_default(),
        }
    }
}

fn file_response(
    status: StatusCode,
    file: &ClientFile,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    match file {
        ClientFile::OpenAi(file) => json_response(status, file, limits),
        ClientFile::Claude(file) => json_response(status, file, limits),
        ClientFile::Gemini(file) => json_response(status, file, limits),
    }
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let client = Family::of(call.client.dialect)?;
    let target = Family::of(call.target)?;
    match call.client.operation {
        Operation::ListFiles => list(call, client, target).await,
        Operation::RetrieveFile => retrieve(call, client, target).await,
        Operation::RetrieveFileContent => content(call, client, target).await,
        Operation::DeleteFile => delete(call, client, target).await,
        Operation::CreateFile => create(call, client, target).await,
        other => Err(TransformError::unsupported(
            "files",
            format!("{other:?} is not a files operation"),
        )),
    }
}

fn key(call: &Call<'_, impl Sized>, operation: Operation) -> OperationKey {
    OperationKey {
        operation,
        dialect: call.target,
    }
}

async fn retrieve<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    client: Family,
    target: Family,
) -> Result<Converted, TransformError> {
    let id = client_file_id(call, client, false)?;
    let key = key(call, Operation::RetrieveFile);
    let template = template(Method::GET, endpoints::files_endpoint(call.target)?.path);
    let name = upstream_name(&id, target);
    let step: Step<(StatusCode, NativeFile)> = match target {
        Family::OpenAi => {
            match adapt::openai_get(call.upstream, &key, template, &name, call.limits).await? {
                JsonInvocation::Success(r) => Step::Ok((r.status, NativeFile::OpenAi(r.body))),
                JsonInvocation::Rejected(r) => Step::Rejected(r),
            }
        }
        Family::Claude => {
            match adapt::claude_get(call.upstream, &key, template, &name, call.limits).await? {
                JsonInvocation::Success(r) => Step::Ok((r.status, NativeFile::Claude(r.body))),
                JsonInvocation::Rejected(r) => Step::Rejected(r),
            }
        }
        Family::Gemini => {
            match adapt::gemini_get(call.upstream, &key, template, &name, call.limits).await? {
                JsonInvocation::Success(r) => {
                    Step::Ok((r.status, NativeFile::Gemini(Box::new(r.body))))
                }
                JsonInvocation::Rejected(r) => Step::Rejected(r),
            }
        }
    };
    match step {
        Step::Rejected(response) => Ok(Converted::Rejected(response)),
        Step::Ok((status, file)) => {
            let converted = file.convert(client, &FileFacts::default())?;
            file_response(status, &converted, call.limits)
        }
    }
}

async fn content<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    client: Family,
    _: Family,
) -> Result<Converted, TransformError> {
    let id = client_file_id(call, client, true)?;
    let endpoint = endpoints::file_content_endpoint(call.target, &id)?;
    let key = key(call, Operation::RetrieveFileContent);
    let response = call
        .upstream
        .send(
            &key,
            WireRequest {
                method: Method::GET,
                path: endpoint.path,
                query: endpoint.query,
                headers: endpoint.headers,
                body: HttpBody::Bytes(Bytes::new()),
            },
        )
        .await?;
    Ok(if response.status.is_success() {
        Converted::Success(response)
    } else {
        Converted::Rejected(response)
    })
}

async fn delete<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    client: Family,
    target: Family,
) -> Result<Converted, TransformError> {
    let id = client_file_id(call, client, false)?;
    let key = key(call, Operation::DeleteFile);
    let path = endpoints::file_endpoint(call.target, &id)?.path;
    let _ = target;
    let response = adapt::delete_empty(call.upstream, &key, template(Method::DELETE, path)).await?;
    if !response.status.is_success() {
        return Ok(Converted::Rejected(response));
    }
    let name = client_name(&id, client);
    match client {
        Family::OpenAi => json_response(
            StatusCode::OK,
            &o::DeleteFileResponseBody::builder(name, o::DeletedFileObject::File, true).build(),
            call.limits,
        ),
        Family::Claude => json_response(
            StatusCode::OK,
            &c::DeleteFileResponseBody::builder(name)
                .type_(c::DeletedFileType::FileDeleted)
                .build(),
            call.limits,
        ),
        Family::Gemini => json_response(
            StatusCode::OK,
            &g::DeleteFileResponseBody::builder().build(),
            call.limits,
        ),
    }
}

/// The client's list request, reduced to what the three dialects share.
#[derive(Default)]
struct ListSpec {
    limit: Option<i64>,
    after: Option<String>,
    purpose: Option<String>,
    order: Option<o::FileOrder>,
    before: Option<String>,
    scope_id: Option<String>,
}

fn list_spec(raw: Option<&str>, client: Family) -> Result<ListSpec, TransformError> {
    Ok(match client {
        Family::OpenAi => {
            let q: o::ListFilesQuery = query_or(raw, || o::ListFilesQuery::builder().build())?;
            ListSpec {
                limit: q.limit,
                after: q.after,
                purpose: q.purpose,
                order: q.order,
                ..Default::default()
            }
        }
        Family::Claude => {
            let q: c::ListFilesQuery = query_or(raw, || c::ListFilesQuery::builder().build())?;
            ListSpec {
                limit: q.limit,
                after: q.after_id.map(|id| client_name(&id, Family::Claude)),
                before: q.before_id,
                scope_id: q.scope_id,
                ..Default::default()
            }
        }
        Family::Gemini => {
            let q: g::ListFilesQuery = query_or(raw, || g::ListFilesQuery::builder().build())?;
            ListSpec {
                limit: q.page_size.flatten().map(i64::from),
                after: q
                    .page_token
                    .flatten()
                    .map(|token| token.strip_prefix("files/").unwrap_or(&token).to_owned()),
                ..Default::default()
            }
        }
    })
}

fn query_or<T: DeserializeOwned>(
    raw: Option<&str>,
    empty: impl FnOnce() -> T,
) -> Result<T, TransformError> {
    match raw.filter(|q| !q.is_empty()) {
        None => Ok(empty()),
        Some(raw) => serde_urlencoded::from_str(raw)
            .map_err(|e| TransformError::shape("files.query", e.to_string())),
    }
}

fn reject_filter(name: &str, target: Family) -> TransformError {
    TransformError::unsupported(
        "files.query",
        format!("`{name}` has no equivalent in the {target:?} files API"),
    )
}

async fn list<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    client: Family,
    target: Family,
) -> Result<Converted, TransformError> {
    let spec = list_spec(call.request.query, client)?;
    if spec.limit.is_some_and(|l| l <= 0) {
        return Err(TransformError::shape(
            "files.query",
            "limit must be positive",
        ));
    }
    let key = key(call, Operation::ListFiles);
    let template = template(Method::GET, endpoints::files_endpoint(call.target)?.path);
    let limits = FileCrudLimits {
        codec: call.limits,

        max_declared_bytes: u64::MAX,
    };
    if spec.before.is_some() && target != Family::Claude {
        return Err(reject_filter("before_id", target));
    }
    if spec.scope_id.is_some() && target != Family::Claude {
        return Err(reject_filter("scope_id", target));
    }
    if spec.purpose.is_some() && target != Family::OpenAi {
        return Err(reject_filter("purpose", target));
    }
    if spec.order.is_some() && target != Family::OpenAi {
        return Err(reject_filter("order", target));
    }
    let after = spec.after.as_deref().map(|a| upstream_name(a, target));
    let files: Step<Vec<NativeFile>> = match target {
        Family::OpenAi => {
            let mut q = o::ListFilesQuery::builder();
            if let Some(v) = spec.limit {
                q = q.limit(v);
            }
            if let Some(v) = after {
                q = q.after(v);
            }
            if let Some(v) = spec.purpose.clone() {
                q = q.purpose(v);
            }
            if let Some(v) = spec.order {
                q = q.order(v);
            }
            match adapt::openai_list(call.upstream, &key, template, q.build(), limits).await {
                Ok(files) => Step::Ok(files.into_iter().map(NativeFile::OpenAi).collect()),
                Err(error) => list_failure(error)?,
            }
        }
        Family::Claude => {
            let mut q = c::ListFilesQuery::builder();
            if let Some(v) = spec.limit {
                q = q.limit(v);
            }
            if let Some(v) = after {
                q = q.after_id(v);
            }
            if let Some(v) = spec.before.clone() {
                q = q.before_id(v);
            }
            if let Some(v) = spec.scope_id.clone() {
                q = q.scope_id(v);
            }
            match adapt::claude_list(call.upstream, &key, template, q.build(), limits).await {
                Ok(files) => Step::Ok(files.into_iter().map(NativeFile::Claude).collect()),
                Err(error) => list_failure(error)?,
            }
        }
        Family::Gemini => {
            let mut q = g::ListFilesQuery::builder();
            if let Some(v) = spec.limit {
                let size = i32::try_from(v)
                    .map_err(|_| TransformError::shape("files.query", "limit exceeds i32"))?;
                q = q.page_size(Some(size));
            }
            if let Some(v) = after {
                q = q.page_token(Some(v));
            }
            match adapt::gemini_list(call.upstream, &key, template, q.build(), limits).await {
                Ok(files) => Step::Ok(
                    files
                        .into_iter()
                        .map(|file| NativeFile::Gemini(Box::new(file)))
                        .collect(),
                ),
                Err(error) => list_failure(error)?,
            }
        }
    };
    let files = match files {
        Step::Ok(files) => files,
        Step::Rejected(response) => return Ok(Converted::Rejected(response)),
    };
    let mut converted = files
        .iter()
        .map(|file| file.convert(client, &FileFacts::default()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut has_more = false;
    if let Some(limit) = spec.limit.and_then(|l| usize::try_from(l).ok())
        && converted.len() > limit
    {
        converted.truncate(limit);
        has_more = true;
    }
    let first = converted.first().map(ClientFile::id);
    let last = converted.last().map(ClientFile::id);
    match client {
        Family::OpenAi => {
            let data = converted
                .into_iter()
                .map(|file| match file {
                    ClientFile::OpenAi(file) => file,
                    _ => unreachable!("converted to the client family"),
                })
                .collect();
            json_response(
                StatusCode::OK,
                &o::ListFilesResponseBody::builder(
                    "list".into(),
                    data,
                    has_more,
                    first.unwrap_or_default(),
                    last.unwrap_or_default(),
                )
                .build(),
                call.limits,
            )
        }
        Family::Claude => {
            let data = converted
                .into_iter()
                .map(|file| match file {
                    ClientFile::Claude(file) => file,
                    _ => unreachable!("converted to the client family"),
                })
                .collect();
            let mut body = c::ListFilesResponseBody::builder(data).has_more(has_more);
            if let Some(first) = first {
                body = body.first_id(first);
            }
            if let Some(last) = last {
                body = body.last_id(last);
            }
            json_response(StatusCode::OK, &body.build(), call.limits)
        }
        Family::Gemini => {
            let data: Vec<g::File> = converted
                .into_iter()
                .map(|file| match file {
                    ClientFile::Gemini(file) => *file,
                    _ => unreachable!("converted to the client family"),
                })
                .collect();
            let mut body = g::ListFilesResponseBody::builder().files(Some(data));
            if has_more && let Some(last) = last {
                body = body.next_page_token(Some(last));
            }
            json_response(StatusCode::OK, &body.build(), call.limits)
        }
    }
}

/// The client's multipart upload, decoded once.
struct Upload {
    filename: Option<String>,
    mime: Option<String>,
    bytes: Bytes,
    fields: BTreeMap<String, String>,
}

fn boundary(headers: &HeaderMap) -> Result<String, TransformError> {
    let content_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| TransformError::shape("files.upload", "multipart content-type required"))?;
    let mut parts = content_type.split(';');
    if !parts
        .next()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("multipart/form-data"))
    {
        return Err(TransformError::shape(
            "files.upload",
            "multipart/form-data body required",
        ));
    }
    parts
        .filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("boundary"))
        .map(|(_, v)| v.trim().trim_matches('"').to_owned())
        .filter(|b| !b.is_empty())
        .ok_or_else(|| TransformError::shape("files.upload", "multipart boundary missing"))
}

/// `name` and `filename` from a part's content-disposition.
fn disposition(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let Some(value) = headers
        .get(http::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
    else {
        return (None, None);
    };
    let mut name = None;
    let mut filename = None;
    for param in value.split(';').skip(1) {
        if let Some((k, v)) = param.trim().split_once('=') {
            let v = v.trim().trim_matches('"').to_owned();
            match k.trim() {
                "name" => name = Some(v),
                "filename" => filename = Some(v),
                _ => {}
            }
        }
    }
    (name, filename)
}

async fn parse_upload<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Upload, TransformError> {
    let boundary = boundary(call.request.headers)?;
    let body = HttpBody::Bytes(Bytes::copy_from_slice(call.body()));
    let mut decoder =
        MultipartDecoder::new(body, boundary, call.limits).map_err(|e| codec(e, "files.upload"))?;
    let mut part_limits = call.limits;
    part_limits.max_body_bytes = part_limits.max_body_bytes.min(part_limits.max_part_bytes);
    let mut file = None;
    let mut fields = BTreeMap::new();
    while let Some(part) = decoder
        .next_part()
        .await
        .map_err(|e| codec(e, "files.upload"))?
    {
        let (name, filename) = disposition(&part.headers);
        let mime = part
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = read_http_body(part.body, part_limits)
            .await
            .map_err(|e| codec(e, "files.upload"))?;
        match name.as_deref() {
            Some("file") => {
                if file.is_some() {
                    return Err(TransformError::shape("files.upload", "duplicate file part"));
                }
                file = Some((filename, mime, bytes));
            }
            Some(name) => {
                let text = String::from_utf8(bytes.to_vec()).map_err(|_| {
                    TransformError::shape("files.upload", "text field is not UTF-8")
                })?;
                fields.insert(name.to_owned(), text);
            }
            None => {
                return Err(TransformError::shape(
                    "files.upload",
                    "multipart part without a field name",
                ));
            }
        }
    }
    let (filename, mime, bytes) =
        file.ok_or_else(|| TransformError::shape("files.upload", "`file` part missing"))?;
    Ok(Upload {
        filename,
        mime,
        bytes,
        fields,
    })
}

fn part(
    name: &str,
    filename: Option<&str>,
    mime: Option<&str>,
    body: Bytes,
) -> Result<MultipartPart, TransformError> {
    let mut headers = HeaderMap::new();
    let disposition = match filename {
        Some(filename) => format!("form-data; name=\"{name}\"; filename=\"{filename}\""),
        None => format!("form-data; name=\"{name}\""),
    };
    headers.insert(
        http::header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .map_err(|e| TransformError::shape("files.upload", e.to_string()))?,
    );
    if let Some(mime) = mime {
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_str(mime)
                .map_err(|e| TransformError::shape("files.upload", e.to_string()))?,
        );
    }
    Ok(MultipartPart {
        headers,
        body: HttpBody::Bytes(body),
    })
}

fn origin_of(base_url: Option<&str>) -> Result<String, TransformError> {
    let base = base_url.ok_or_else(|| TransformError::missing_metadata("provider.base_url"))?;
    let uri: http::Uri = base.parse().map_err(|e: http::uri::InvalidUri| {
        TransformError::shape("provider.base_url", e.to_string())
    })?;
    match (uri.scheme_str(), uri.authority()) {
        (Some(scheme), Some(authority)) => Ok(format!("{scheme}://{authority}")),
        _ => Err(TransformError::shape(
            "provider.base_url",
            "absolute base URL required",
        )),
    }
}

async fn create<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    client: Family,
    target: Family,
) -> Result<Converted, TransformError> {
    let upload = parse_upload(call).await?;
    let key = key(call, Operation::CreateFile);
    // What the client told us about the file; the upstream reply fills the rest.
    let mut known = FileFacts {
        filename: upload.filename.clone(),
        mime: upload.mime.clone(),
        bytes: Some(upload.bytes.len() as u64),
        ..Default::default()
    };
    let purpose = match client {
        Family::OpenAi => upload
            .fields
            .get("purpose")
            .cloned()
            .ok_or_else(|| TransformError::shape("files.upload", "`purpose` field required"))?,
        Family::Claude | Family::Gemini => "user_data".to_owned(),
    };
    known.purpose = Some(match purpose.as_str() {
        "assistants" => FilePurpose::Assistants,
        "batch" => FilePurpose::Batch,
        "fine-tune" => FilePurpose::FineTune,
        "vision" => FilePurpose::Vision,
        "user_data" => FilePurpose::UserData,
        other => FilePurpose::Other(other.to_owned()),
    });
    let boundary = format!("gproxy-{}", crate::ids::random_id());
    let template = template(Method::POST, endpoints::files_endpoint(call.target)?.path);
    let step: Step<(StatusCode, NativeFile)> = match target {
        Family::OpenAi => {
            let parts = vec![
                part(
                    "file",
                    upload.filename.as_deref(),
                    upload.mime.as_deref(),
                    upload.bytes.clone(),
                )?,
                part("purpose", None, None, Bytes::from(purpose.clone()))?,
            ];
            match adapt::upload_multipart_json::<_, o::FileObject>(
                call.upstream,
                &key,
                template,
                boundary,
                parts,
                call.limits,
            )
            .await
            {
                Ok(JsonInvocation::Success(r)) => Step::Ok((r.status, NativeFile::OpenAi(r.body))),
                Ok(JsonInvocation::Rejected(r)) => Step::Rejected(r),
                Err(error) => upload_failure(error)?,
            }
        }
        Family::Claude => {
            let parts = vec![part(
                "file",
                upload.filename.as_deref(),
                upload.mime.as_deref(),
                upload.bytes.clone(),
            )?];
            match adapt::upload_multipart_json::<_, c::FileMetadata>(
                call.upstream,
                &key,
                template,
                boundary,
                parts,
                call.limits,
            )
            .await
            {
                Ok(JsonInvocation::Success(r)) => Step::Ok((r.status, NativeFile::Claude(r.body))),
                Ok(JsonInvocation::Rejected(r)) => Step::Rejected(r),
                Err(error) => upload_failure(error)?,
            }
        }
        Family::Gemini => {
            let mime = upload.mime.clone().ok_or_else(|| {
                TransformError::shape(
                    "files.upload",
                    "Gemini upload needs the file's content-type",
                )
            })?;
            let origin = origin_of(
                call.upstream
                    .attempt()
                    .request
                    .target
                    .provider
                    .entity
                    .base_url
                    .as_deref(),
            )?;
            let mut file = g::File::builder();
            if let Some(name) = upload.filename.clone() {
                file = file.display_name(Some(name));
            }
            let metadata = g::UploadFileMetadata::builder()
                .file(Some(file.build()))
                .build();
            let start = WireRequest {
                method: Method::POST,
                path: endpoints::gemini_upload_endpoint()?.path,
                query: None,
                headers: HeaderMap::new(),
                body: (),
            };
            let size = upload.bytes.len() as u64;
            let mut session = match adapt::gemini_start_resumable(
                call.upstream,
                &key,
                start,
                metadata,
                size,
                &mime,
                &origin,
                call.limits,
            )
            .await
            {
                Ok(session) => session,
                Err(error) => match upload_failure::<()>(error)? {
                    Step::Rejected(response) => return Ok(Converted::Rejected(response)),
                    Step::Ok(()) => unreachable!("failure has no value"),
                },
            };
            match adapt::gemini_upload_chunk(
                call.upstream,
                &key,
                &mut session,
                HeaderMap::new(),
                &origin,
                upload.bytes.clone(),
                true,
                call.limits,
            )
            .await
            {
                Ok(GeminiUploadProgress::Finalized(r)) => {
                    let file =
                        r.body.file.flatten().ok_or_else(|| {
                            TransformError::missing_metadata("upload response file")
                        })?;
                    Step::Ok((r.status, NativeFile::Gemini(Box::new(file))))
                }
                Ok(GeminiUploadProgress::Accepted(_)) => {
                    return Err(TransformError::invalid_result(
                        "files.upload",
                        "finalized upload answered without the file",
                    ));
                }
                Err(error) => upload_failure(error)?,
            }
        }
    };
    match step {
        Step::Rejected(response) => Ok(Converted::Rejected(response)),
        Step::Ok((status, file)) => {
            let converted = file.convert(client, &known)?;
            file_response(status, &converted, call.limits)
        }
    }
}
