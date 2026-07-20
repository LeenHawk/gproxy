use crate::api::error::ApiError;
use crate::api::pagination::PageResponse;
use crate::store::persistence::{PageQuery, PageResult};

const DEFAULT_PAGE_SIZE: u64 = 50;
const MAX_PAGE_SIZE: u64 = 100;

pub(super) struct PageRequest {
    pub page: u64,
    pub page_size: u64,
    pub store: PageQuery,
}

impl PageRequest {
    pub fn response<T>(self, result: PageResult<T>) -> PageResponse<T> {
        PageResponse::new(result.items, self.page, self.page_size, result.total)
    }
}

/// Numeric pagination is opt-in: without `page`, all new parameters are ignored
/// so legacy cursor callers retain their previous behavior.
pub(super) fn parse(
    page: Option<&str>,
    page_size: Option<&str>,
    has_before_id: bool,
) -> Result<Option<PageRequest>, ApiError> {
    let Some(page) = page else {
        return Ok(None);
    };
    let page = page
        .parse::<u64>()
        .map_err(|_| ApiError::BadRequest("page must be a positive integer".into()))?;
    if page == 0 {
        return Err(ApiError::BadRequest("page must be at least 1".into()));
    }
    if has_before_id {
        return Err(ApiError::BadRequest(
            "page and before_id cannot be used together".into(),
        ));
    }
    let page_size = page_size
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| ApiError::BadRequest("page_size must be an integer".into()))?
        .unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
        return Err(ApiError::BadRequest(
            "page_size must be between 1 and 100".into(),
        ));
    }
    let offset = (page - 1)
        .checked_mul(page_size)
        .filter(|offset| i64::try_from(*offset).is_ok())
        .ok_or_else(|| ApiError::BadRequest("page offset overflow".into()))?;
    Ok(Some(PageRequest {
        page,
        page_size,
        store: PageQuery {
            offset,
            limit: page_size,
        },
    }))
}
