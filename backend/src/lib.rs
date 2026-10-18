//! iapolitique backend: public and admin APIs, background worker and migrations.
//!
//! See `AGENTS.md` at the repository root for the product rules and technical contracts.

pub mod ai;
pub mod antispam;
pub mod app;
pub mod config;
pub mod db;
pub mod domain;
pub mod healthcheck;
pub mod http;
pub mod jobs;
pub mod moderation;
pub mod telemetry;

#[cfg(test)]
pub(crate) mod test_support;
