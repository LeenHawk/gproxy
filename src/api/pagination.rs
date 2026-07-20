//! Shared numeric-pagination response DTOs.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PageResponse<T> {
    pub items: Vec<T>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize)]
pub struct Pagination {
    pub page: u64,
    pub page_size: u64,
    pub total_items: u64,
    pub total_pages: u64,
}

impl<T> PageResponse<T> {
    pub fn new(items: Vec<T>, page: u64, page_size: u64, total_items: u64) -> Self {
        let total_pages = if total_items == 0 {
            0
        } else {
            1 + (total_items - 1) / page_size
        };
        Self {
            items,
            pagination: Pagination {
                page,
                page_size,
                total_items,
                total_pages,
            },
        }
    }
}
