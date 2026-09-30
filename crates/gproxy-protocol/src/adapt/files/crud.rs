use super::{FileCrudLimits, FileFailure, FileOperationError};
use crate::{
    HttpBody, WireRequest, WireResponse,
    adapt::{JsonInvocation, invoke_empty},
    capability::Upstream,
    codec::{self, CodecLimits},
    transform::{TransformError, TransformErrorKind},
    wire::{DeclaredFields, claude::files as c, gemini::files as g, openai::files as o},
};
use std::collections::BTreeSet;

fn template(request: &WireRequest<()>, method: http::Method) -> Result<(), TransformError> {
    super::path(&request.path)?;
    if request.method != method || request.query.as_ref().is_some_and(|v| !v.is_empty()) {
        return Err(TransformError::shape(
            "files.template",
            "selected template method must match operation; pass filters using typed query",
        ));
    }
    Ok(())
}

fn get_request(
    mut request: WireRequest<()>,
    id: &str,
    gemini: bool,
) -> Result<WireRequest<()>, TransformError> {
    template(&request, http::Method::GET)?;
    request.path = format!(
        "{}/{}",
        request.path.trim_end_matches('/'),
        if gemini {
            super::name(id, true)?
                .strip_prefix("files/")
                .unwrap()
                .to_owned()
        } else {
            super::name(id, false)?
        }
    );
    Ok(request)
}

pub async fn openai_get<U: Upstream>(
    u: &U,
    target: &U::Target,
    template: WireRequest<()>,
    id: &str,
    limits: CodecLimits,
) -> Result<JsonInvocation<o::FileObject>, TransformError> {
    invoke_empty(u, target, get_request(template, id, false)?, limits).await
}

pub async fn claude_get<U: Upstream>(
    u: &U,
    target: &U::Target,
    template: WireRequest<()>,
    id: &str,
    limits: CodecLimits,
) -> Result<JsonInvocation<c::FileMetadata>, TransformError> {
    invoke_empty(u, target, get_request(template, id, false)?, limits).await
}

pub async fn gemini_get<U: Upstream>(
    u: &U,
    target: &U::Target,
    template: WireRequest<()>,
    name: &str,
    limits: CodecLimits,
) -> Result<JsonInvocation<g::File>, TransformError> {
    invoke_empty(u, target, get_request(template, name, true)?, limits).await
}

pub async fn delete_empty<U: Upstream>(
    u: &U,
    target: &U::Target,
    mut request: WireRequest<()>,
) -> Result<WireResponse<HttpBody>, TransformError> {
    super::path(&request.path)?;
    if request.method != http::Method::DELETE {
        return Err(TransformError::shape(
            "files.delete",
            "DELETE template required",
        ));
    }
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
        http::header::CONTENT_TYPE,
    ] {
        request.headers.remove(name);
    }
    u.send(
        target,
        WireRequest {
            method: request.method,
            path: request.path,
            query: request.query,
            headers: request.headers,
            body: HttpBody::Bytes(bytes::Bytes::new()),
        },
    )
    .await
    .map_err(TransformError::from)
}

struct Progress {
    calls: usize,
    files: usize,
    bytes: u64,
    ids: BTreeSet<String>,
    tokens: BTreeSet<String>,
    limits: FileCrudLimits,
}

impl Progress {
    fn new(request: &WireRequest<()>, limits: FileCrudLimits) -> Result<Self, FileOperationError> {
        template(request, http::Method::GET)?;

        Ok(Self {
            calls: 0,
            files: 0,
            bytes: 0,
            ids: BTreeSet::new(),
            tokens: BTreeSet::new(),
            limits,
        })
    }
    fn err(&self, error: TransformError) -> FileOperationError {
        FileOperationError {
            completed_calls: self.calls,
            completed_files: self.files,
            failure: FileFailure::Transform(error),
        }
    }
    fn limit(&self, field: &str) -> FileOperationError {
        self.err(TransformError::new(
            TransformErrorKind::Limit,
            field,
            "file collection limit exceeded",
        ))
    }
    async fn page<
        U: Upstream,
        T: serde::de::DeserializeOwned + DeclaredFields + serde::Serialize,
    >(
        &mut self,
        u: &U,
        target: &U::Target,
        request: WireRequest<()>,
    ) -> Result<T, FileOperationError> {
        let response = invoke_empty(u, target, request, self.limits.codec)
            .await
            .map_err(|e| self.err(e))?;
        let body = match response {
            JsonInvocation::Success(response) => response.body,
            JsonInvocation::Rejected(response) => {
                return Err(FileOperationError {
                    completed_calls: self.calls,
                    completed_files: self.files,
                    failure: FileFailure::Rejected(Box::new(response)),
                });
            }
        };
        self.calls += 1;
        let bytes = codec::encode_json(&body, self.limits.codec)
            .map_err(|e| {
                self.err(TransformError::new(
                    TransformErrorKind::Limit,
                    "files.bytes",
                    e.to_string(),
                ))
            })?
            .len() as u64;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| self.limit("files.bytes"))?;
        if self.bytes > self.limits.max_declared_bytes {
            return Err(self.limit("files.bytes"));
        }
        Ok(body)
    }
    fn item(&mut self, id: &str) -> Result<(), FileOperationError> {
        if id.is_empty() || !self.ids.insert(id.to_owned()) {
            return Err(self.err(TransformError::invalid_result(
                "files.id",
                "empty or duplicate file identity",
            )));
        }

        self.files += 1;
        Ok(())
    }
    fn cursor(&mut self, token: String) -> Result<String, FileOperationError> {
        if token.is_empty() || !self.tokens.insert(token.clone()) {
            return Err(self.err(TransformError::invalid_result(
                "files.cursor",
                "empty or repeated cursor",
            )));
        }
        Ok(token)
    }
}

fn page_request(request: &WireRequest<()>, pairs: Vec<(&str, String)>) -> WireRequest<()> {
    WireRequest {
        method: request.method.clone(),
        path: request.path.clone(),
        headers: request.headers.clone(),
        query: (!pairs.is_empty()).then(|| {
            pairs
                .into_iter()
                .map(|(k, v)| format!("{k}={}", super::component(&v)))
                .collect::<Vec<_>>()
                .join("&")
        }),
        body: (),
    }
}

pub async fn openai_list<U: Upstream>(
    u: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: o::ListFilesQuery,
    limits: FileCrudLimits,
) -> Result<Vec<o::FileObject>, FileOperationError> {
    let query = query.into_declared();
    if query.limit.is_some_and(|v| v <= 0 || v > 10000) {
        return Err(TransformError::shape("limit", "OpenAI list limit must be 1..10000").into());
    }
    let mut progress = Progress::new(&request, limits)?;
    let mut cursor = query.after;
    let mut out = Vec::new();
    if let Some(cursor) = &cursor {
        progress.tokens.insert(cursor.clone());
    }
    loop {
        let mut pairs = Vec::new();
        if let Some(value) = &cursor {
            pairs.push(("after", value.clone()));
        }
        if let Some(value) = &query.purpose {
            pairs.push(("purpose", value.clone()));
        }
        if let Some(value) = query.limit {
            pairs.push(("limit", value.to_string()));
        }
        if let Some(value) = query.order {
            pairs.push((
                "order",
                match value {
                    o::FileOrder::Asc => "asc",
                    o::FileOrder::Desc => "desc",
                }
                .into(),
            ));
        }
        let page: o::ListFilesResponseBody = progress
            .page(u, target, page_request(&request, pairs))
            .await?;
        if let (Some(first), Some(last)) = (page.data.first(), page.data.last()) {
            if first.id != page.first_id || last.id != page.last_id {
                return Err(progress.err(TransformError::invalid_result(
                    "pagination",
                    "file boundaries disagree",
                )));
            }
        } else if page.has_more {
            return Err(progress.err(TransformError::invalid_result(
                "pagination",
                "empty continuing page",
            )));
        }
        for file in page.data {
            progress.item(&file.id)?;
            out.push(file);
        }
        if !page.has_more {
            return Ok(out);
        }
        cursor = Some(progress.cursor(page.last_id)?);
    }
}

pub async fn claude_list<U: Upstream>(
    u: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: c::ListFilesQuery,
    limits: FileCrudLimits,
) -> Result<Vec<c::FileMetadata>, FileOperationError> {
    let query = query.into_declared();
    if query.after_id.is_some() && query.before_id.is_some() {
        return Err(TransformError::shape("cursor", "after_id and before_id conflict").into());
    }
    if query.limit.is_some_and(|v| v <= 0 || v > 1000) {
        return Err(TransformError::shape("limit", "Claude limit must be 1..1000").into());
    }
    let backward = query.before_id.is_some();
    let mut cursor = query.before_id.or(query.after_id);
    let mut progress = Progress::new(&request, limits)?;
    let mut out = Vec::new();
    if let Some(cursor) = &cursor {
        progress.tokens.insert(cursor.clone());
    }
    loop {
        let mut pairs = Vec::new();
        if let Some(cursor) = &cursor {
            pairs.push((
                if backward { "before_id" } else { "after_id" },
                cursor.clone(),
            ));
        }
        if let Some(scope) = &query.scope_id {
            pairs.push(("scope_id", scope.clone()));
        }
        if let Some(limit) = query.limit {
            pairs.push(("limit", limit.to_string()));
        }
        let page: c::ListFilesResponseBody = progress
            .page(u, target, page_request(&request, pairs))
            .await?;
        let more = page
            .has_more
            .ok_or_else(|| progress.err(TransformError::missing_metadata("has_more")))?;
        if let (Some(first), Some(last)) = (page.data.first(), page.data.last()) {
            if page.first_id.as_deref().is_some_and(|v| v != first.id)
                || page.last_id.as_deref().is_some_and(|v| v != last.id)
            {
                return Err(progress.err(TransformError::invalid_result(
                    "pagination",
                    "file boundaries disagree",
                )));
            }
        } else if more {
            return Err(progress.err(TransformError::invalid_result(
                "pagination",
                "empty continuing page",
            )));
        }
        for file in page.data {
            progress.item(&file.id)?;
            out.push(file);
        }
        if !more {
            return Ok(out);
        }
        cursor = Some(
            progress.cursor(
                if backward {
                    page.first_id
                } else {
                    page.last_id
                }
                .ok_or_else(|| {
                    progress.err(TransformError::missing_metadata("pagination cursor"))
                })?,
            )?,
        );
    }
}

pub async fn gemini_list<U: Upstream>(
    u: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: g::ListFilesQuery,
    limits: FileCrudLimits,
) -> Result<Vec<g::File>, FileOperationError> {
    let query = query.into_declared();
    if query.page_size.flatten().is_some_and(|v| v <= 0 || v > 100) {
        return Err(TransformError::shape("page_size", "Gemini file limit must be 1..100").into());
    }
    let mut cursor = query.page_token.flatten();
    let mut progress = Progress::new(&request, limits)?;
    let mut out = Vec::new();
    if let Some(cursor) = &cursor {
        progress.tokens.insert(cursor.clone());
    }
    loop {
        let mut pairs = Vec::new();
        if let Some(cursor) = &cursor {
            pairs.push(("pageToken", cursor.clone()));
        }
        if let Some(size) = query.page_size.flatten() {
            pairs.push(("pageSize", size.to_string()));
        }
        let page: g::ListFilesResponseBody = progress
            .page(u, target, page_request(&request, pairs))
            .await?;
        for file in page.files.flatten().unwrap_or_default() {
            let id = file
                .name
                .as_ref()
                .and_then(Option::as_ref)
                .ok_or_else(|| progress.err(TransformError::missing_metadata("file.name")))?;
            super::name(id, true).map_err(|e| progress.err(e))?;
            progress.item(id)?;
            out.push(file);
        }
        match page.next_page_token.flatten().filter(|v| !v.is_empty()) {
            None => return Ok(out),
            Some(value) => cursor = Some(progress.cursor(value)?),
        }
    }
}
