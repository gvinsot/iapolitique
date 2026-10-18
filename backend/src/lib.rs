//! iapolitique backend: public and admin APIs, background worker and migrations.
//!
//! See `AGENTS.md` at the repository root for the product rules and technical contracts.

pub mod app;
pub mod config;
pub mod db;
pub mod domain;
pub mod healthcheck;
pub mod jobs;
pub mod telemetry;
