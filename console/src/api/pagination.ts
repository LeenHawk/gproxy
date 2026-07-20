export const PAGE_SIZE = 50;

export interface PageMeta {
  page: number;
  page_size: number;
  total_items: number;
  total_pages: number;
}

export interface PageResult<T> {
  items: T[];
  pagination: PageMeta;
}
