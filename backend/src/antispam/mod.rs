//! Anti-abuse measures of the public forms (AGENTS.md §7.2).
//!
//! - [`altcha`]: self-hosted ALTCHA proof of work.
//! - [`rate_limit`]: in-memory limit per daily-salted fingerprint of the client IP.

pub mod altcha;
pub mod rate_limit;
