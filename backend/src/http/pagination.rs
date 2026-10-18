//! Bounded pagination of every list (AGENTS.md §14.1: `per_page ≤ 50`).

use super::error::ApiError;
use super::ideas::invalid_query;

/// Default number of items per page.
pub const DEFAULT_PER_PAGE: u32 = 20;
/// Largest page size.
pub const MAX_PER_PAGE: u32 = 50;
/// Largest page number accepted (far above any real list).
pub const MAX_PAGE: u32 = 10_000;

/// Validated page and page size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub page: u32,
    pub per_page: u32,
}

impl PageRequest {
    /// `page` from 1 to 10 000 (1 by default), `per_page` from 1 to 50 (20 by default);
    /// `400 invalid_query` otherwise.
    pub fn parse(page: Option<u32>, per_page: Option<u32>) -> Result<Self, ApiError> {
        let page = page.unwrap_or(1);
        let per_page = per_page.unwrap_or(DEFAULT_PER_PAGE);
        if !(1..=MAX_PAGE).contains(&page) {
            return Err(invalid_query(
                "Le numéro de page doit être compris entre 1 et 10 000.",
            ));
        }
        if !(1..=MAX_PER_PAGE).contains(&per_page) {
            return Err(invalid_query(
                "Le nombre d'éléments par page doit être compris entre 1 et 50.",
            ));
        }
        Ok(Self { page, per_page })
    }

    pub fn limit(self) -> i64 {
        i64::from(self.per_page)
    }

    pub fn offset(self) -> i64 {
        i64::from(self.page - 1) * i64::from(self.per_page)
    }

    /// Number of pages for `total` items.
    pub fn page_count(self, total: i64) -> u32 {
        let total = u64::try_from(total).unwrap_or(0);
        u32::try_from(total.div_ceil(u64::from(self.per_page))).unwrap_or(u32::MAX)
    }
}

/// Declares a page of `$item`: `{ items, page, per_page, total, total_pages }`.
macro_rules! page_type {
    ($(#[$meta:meta])* $name:ident, $item:ty) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, serde::Serialize, utoipa::ToSchema)]
        pub struct $name {
            pub items: Vec<$item>,
            pub page: u32,
            pub per_page: u32,
            /// Items matching the filters, all pages included.
            pub total: u64,
            pub total_pages: u32,
        }

        impl $name {
            pub fn new(
                items: Vec<$item>,
                pages: $crate::http::pagination::PageRequest,
                total: i64,
            ) -> Self {
                Self {
                    items,
                    page: pages.page,
                    per_page: pages.per_page,
                    total: u64::try_from(total).unwrap_or(0),
                    total_pages: pages.page_count(total),
                }
            }
        }
    };
}

pub(crate) use page_type;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_bounded() {
        assert_eq!(
            PageRequest::parse(None, None).unwrap(),
            PageRequest {
                page: 1,
                per_page: DEFAULT_PER_PAGE
            }
        );
        assert!(PageRequest::parse(Some(0), None).is_err());
        assert!(PageRequest::parse(Some(MAX_PAGE + 1), None).is_err());
        assert!(PageRequest::parse(None, Some(0)).is_err());
        assert!(PageRequest::parse(None, Some(MAX_PER_PAGE + 1)).is_err());
        let pages = PageRequest::parse(Some(3), Some(10)).unwrap();
        assert_eq!((pages.limit(), pages.offset()), (10, 20));
        assert_eq!(pages.page_count(0), 0);
        assert_eq!(pages.page_count(10), 1);
        assert_eq!(pages.page_count(11), 2);
    }
}
