use super::{ModelDirectory, ModelListError, ModelListFailure, ModelListLimits, query};
use crate::{
    WireRequest,
    adapt::{JsonInvocation, invoke_empty},
    capability::Upstream,
    codec,
    transform::{TransformError, TransformErrorKind},
    wire::{DeclaredFields, claude::models as c, gemini::models as g, openai::models as o},
};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeSet;

struct Progress {
    calls: usize,
    models: usize,
    bytes: u64,
    ids: BTreeSet<String>,
    limits: ModelListLimits,
}
impl Progress {
    fn new(request: &WireRequest<()>, limits: ModelListLimits) -> Result<Self, ModelListError> {
        query::template(request)?;
        if limits.max_calls == 0 {
            return Err(TransformError::new(
                TransformErrorKind::Limit,
                "pagination.max_calls",
                "at least one call is required",
            )
            .into());
        }
        Ok(Self {
            calls: 0,
            models: 0,
            bytes: 0,
            ids: BTreeSet::new(),
            limits,
        })
    }
    fn error(&self, error: TransformError) -> ModelListError {
        ModelListError {
            completed_calls: self.calls,
            completed_models: self.models,
            failure: ModelListFailure::Transform(error),
        }
    }
    fn invalid(&self, path: &str, detail: &str) -> ModelListError {
        self.error(TransformError::new(
            TransformErrorKind::InvalidResult,
            path,
            detail,
        ))
    }
    async fn page<U: Upstream, O: DeserializeOwned + DeclaredFields + Serialize>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        request: WireRequest<()>,
    ) -> Result<O, ModelListError> {
        if self.calls >= self.limits.max_calls {
            return Err(self.error(TransformError::new(
                TransformErrorKind::Limit,
                "pagination.max_calls",
                "directory has more pages than allowed",
            )));
        }
        let result = invoke_empty(upstream, target, request, self.limits.codec)
            .await
            .map_err(|e| self.error(e))?;
        let response = match result {
            JsonInvocation::Success(response) => response,
            JsonInvocation::Rejected(response) => {
                return Err(ModelListError {
                    completed_calls: self.calls,
                    completed_models: self.models,
                    failure: ModelListFailure::Rejected(Box::new(response)),
                });
            }
        };
        self.calls += 1;
        let encoded = codec::encode_json(&response.body, self.limits.codec).map_err(|e| {
            self.error(TransformError::new(
                TransformErrorKind::Limit,
                "pagination.page",
                e.to_string(),
            ))
        })?;
        self.bytes = self
            .bytes
            .checked_add(encoded.len() as u64)
            .ok_or_else(|| self.invalid("pagination.bytes", "byte counter overflow"))?;
        if self.bytes > self.limits.max_declared_bytes {
            return Err(self.error(TransformError::new(
                TransformErrorKind::Limit,
                "pagination.max_declared_bytes",
                "aggregate declared page bytes exceeded",
            )));
        }
        Ok(response.body)
    }
    fn item(&mut self, id: &str) -> Result<(), ModelListError> {
        if id.is_empty() || !self.ids.insert(id.to_owned()) {
            return Err(self.invalid("models.id", "empty or repeated model identity across pages"));
        }
        if self.models >= self.limits.max_models {
            return Err(self.error(TransformError::new(
                TransformErrorKind::Limit,
                "pagination.max_models",
                "directory exceeds model count limit",
            )));
        }
        self.models += 1;
        Ok(())
    }
}

/// Fetch all Claude pages in forward order, starting without a cursor.
pub async fn collect_claude<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: c::ListModelsQuery,
    limits: ModelListLimits,
) -> Result<ModelDirectory<c::ListModelsResponseBody>, ModelListError> {
    let query = query.into_declared();
    query::page_size(query.limit, 1000)?;
    if query.after_id.is_some() || query.before_id.is_some() {
        return Err(TransformError::shape(
            "query.cursor",
            "full-directory collection must begin at the first page",
        )
        .into());
    }
    let mut progress = Progress::new(&request, limits)?;
    let mut cursor = None;
    let mut cursors = BTreeSet::new();
    let mut data = Vec::new();
    let mut first_id = None;
    loop {
        let mut pairs = Vec::new();
        if let Some(limit) = query.limit {
            pairs.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = &cursor {
            pairs.push(("after_id", String::clone(cursor)));
        }
        let mut next = WireRequest {
            method: request.method.clone(),
            path: request.path.clone(),
            query: None,
            headers: request.headers.clone(),
            body: (),
        };
        next.query = query::encode(&pairs);
        let page: c::ListModelsResponseBody = progress.page(upstream, target, next).await?;
        if let (Some(first), Some(last)) = (page.data.first(), page.data.last()) {
            if first.id != page.first_id || last.id != page.last_id {
                return Err(progress.invalid(
                    "pagination",
                    "Claude page boundary IDs disagree with model data",
                ));
            }
        } else if page.has_more {
            return Err(progress.invalid("pagination", "empty Claude page claims continuation"));
        }
        if first_id.is_none() {
            first_id = Some(page.first_id);
        }
        for model in page.data {
            progress.item(&model.id)?;
            data.push(model);
        }
        if !page.has_more {
            let last_id = data
                .last()
                .map(|model| model.id.clone())
                .unwrap_or(page.last_id);
            return Ok(ModelDirectory {
                value: c::ListModelsResponseBody {
                    data,
                    first_id: first_id.expect("first page was decoded"),
                    last_id,
                    has_more: false,
                    rest: Default::default(),
                },
                completed_calls: progress.calls,
            });
        }
        if page.last_id.is_empty() || !cursors.insert(page.last_id.clone()) {
            return Err(progress.invalid(
                "pagination.last_id",
                "empty or repeated continuation cursor",
            ));
        }
        cursor = Some(page.last_id);
    }
}

/// Fetch all Gemini pages using each opaque `nextPageToken` verbatim.
pub async fn collect_gemini<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    query: g::ListModelsQuery,
    limits: ModelListLimits,
) -> Result<ModelDirectory<g::ListModelsResponseBody>, ModelListError> {
    let query = query.into_declared();
    query::page_size(query.page_size, i64::MAX)?;
    if query.page_token.is_some() {
        return Err(TransformError::shape(
            "query.page_token",
            "full-directory collection must begin at the first page",
        )
        .into());
    }
    let mut progress = Progress::new(&request, limits)?;
    let mut cursor = None;
    let mut cursors = BTreeSet::new();
    let mut models = Vec::new();
    loop {
        let mut pairs = Vec::new();
        if let Some(size) = query.page_size {
            pairs.push(("pageSize", size.to_string()));
        }
        if let Some(cursor) = &cursor {
            pairs.push(("pageToken", String::clone(cursor)));
        }
        let mut next = WireRequest {
            method: request.method.clone(),
            path: request.path.clone(),
            query: None,
            headers: request.headers.clone(),
            body: (),
        };
        next.query = query::encode(&pairs);
        let page: g::ListModelsResponseBody = progress.page(upstream, target, next).await?;
        for model in page.models.unwrap_or_default() {
            progress.item(&model.name)?;
            models.push(model);
        }
        // Google pagination uses an empty token as the terminal marker.
        match page.next_page_token.filter(|token| !token.is_empty()) {
            None => {
                return Ok(ModelDirectory {
                    value: g::ListModelsResponseBody {
                        models: Some(models),
                        next_page_token: None,
                        rest: Default::default(),
                    },
                    completed_calls: progress.calls,
                });
            }
            Some(token) => {
                if !cursors.insert(token.clone()) {
                    return Err(
                        progress.invalid("pagination.nextPageToken", "repeated continuation token")
                    );
                }
                cursor = Some(token);
            }
        }
    }
}

/// OpenAI model lists are unpaged, but still obey aggregate/model limits.
pub async fn collect_openai<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    limits: ModelListLimits,
) -> Result<ModelDirectory<o::ListModelsResponseBody>, ModelListError> {
    let mut progress = Progress::new(&request, limits)?;
    let page: o::ListModelsResponseBody = progress.page(upstream, target, request).await?;
    for model in &page.data {
        progress.item(&model.id)?;
    }
    Ok(ModelDirectory {
        value: page,
        completed_calls: progress.calls,
    })
}
