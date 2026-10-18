//! Business types and rules. **No I/O here**: everything is pure and unit tested.
//!
//! - [`status`]: state machines of ideas, contributions and reports (AGENTS.md §5.2), the single
//!   source of truth for allowed transitions.
//! - [`scoring`]: composite score (AGENTS.md §6.3).
//! - [`money`]: euro amounts and AI cost helpers.
//! - [`slug`]: URL slugs from French titles.
//! - [`validation`]: normalization and validation of user-submitted texts.
//! - [`types`]: enumerations shared by the database, the API and the AI pipeline.

#[macro_use]
mod text_enum;

pub mod money;
pub mod scoring;
pub mod slug;
pub mod status;
pub mod types;
pub mod validation;

pub use text_enum::UnknownVariant;
pub(crate) use text_enum::text_enum;
