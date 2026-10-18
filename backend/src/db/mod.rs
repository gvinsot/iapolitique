//! PostgreSQL access: connection pool, embedded migrations and per-aggregate queries.
//!
//! Queries are checked at runtime (`sqlx::query_as` + `FromRow`), never with the `query!`
//! macros, so building needs no database and no `.sqlx/` cache (AGENTS.md §4.1).

use std::str::FromStr;
use std::time::Duration;

use anyhow::Context;
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

use crate::config::DatabaseConfig;

pub mod admin_actions;
pub mod admin_moderation;
pub mod ai_calls;
pub mod analyses;
pub mod dashboard;
pub mod embeddings;
pub mod exports;
pub mod ideas;
pub mod portal;
pub mod screenings;
pub mod settings;
pub mod status;
pub mod submissions;
pub mod tracked;
pub mod tracking;

/// Migrations of `backend/migrations`, embedded in the binary.
///
/// `ignore_missing`: after an automatic rollback (PulsarCD redeploys the previous image on the
/// migrated database, AGENTS.md §2.2), the database knows migrations that the older binary does
/// not embed. They are additive and backward compatible, so the older binary must still start.
pub static MIGRATOR: Migrator = Migrator {
    ignore_missing: true,
    ..sqlx::migrate!("./migrations")
};

/// libpq parameters that sqlx does not support and that we drop silently.
const UNSUPPORTED_PARAMS: &[&str] = &[
    "target_session_attrs",
    "connect_timeout",
    "load_balance_hosts",
];

/// Adapts a libpq-style URL to what sqlx understands.
///
/// The cluster documents multi-host URLs (`postgres://u:p@pg-primary:5432,pg-standby1:5432/db`
/// with `target_session_attrs=read-write`), which sqlx cannot parse: only the first host, the
/// primary, is kept, and libpq-only parameters are removed. Returns the normalized URL and
/// whether hosts were dropped.
pub fn normalize_connection_string(raw: &str) -> (String, bool) {
    let raw = raw.trim();
    let Some(scheme_end) = raw.find("://") else {
        return (raw.to_owned(), false);
    };
    let (scheme, rest) = raw.split_at(scheme_end + 3);
    let (authority, tail) = match rest.find(['/', '?']) {
        Some(pos) => rest.split_at(pos),
        None => (rest, ""),
    };
    let (userinfo, hosts) = match authority.rfind('@') {
        Some(pos) => authority.split_at(pos + 1),
        None => ("", authority),
    };
    let mut reduced = false;
    let host = match hosts.split_once(',') {
        Some((first, _)) => {
            reduced = true;
            first
        }
        None => hosts,
    };
    let (path, query) = match tail.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (tail, None),
    };
    let mut url = format!("{scheme}{userinfo}{host}{path}");
    if let Some(query) = query {
        let kept: Vec<&str> = query
            .split('&')
            .filter(|pair| {
                let key = pair.split('=').next().unwrap_or_default();
                !pair.is_empty() && !UNSUPPORTED_PARAMS.contains(&key)
            })
            .collect();
        if !kept.is_empty() {
            url.push('?');
            url.push_str(&kept.join("&"));
        }
    }
    (url, reduced)
}

/// Builds the connection options from `DATABASE_CONNECTION_STRING`.
pub fn connect_options(
    connection_string: &str,
    application_name: &str,
) -> anyhow::Result<PgConnectOptions> {
    let (url, reduced) = normalize_connection_string(connection_string);
    if reduced {
        tracing::warn!("DATABASE_CONNECTION_STRING lists several hosts: only the first is used");
    }
    // The error of sqlx may quote the URL: do not propagate it, it contains the password.
    let options = PgConnectOptions::from_str(&url).map_err(|_| {
        anyhow::anyhow!("DATABASE_CONNECTION_STRING is not a valid postgres:// URL")
    })?;
    let options = if url.contains("application_name=") {
        options
    } else {
        options.application_name(application_name)
    };
    Ok(options)
}

/// Opens the pool, retrying for about a minute so that a restarting database does not make the
/// service crash-loop immediately.
pub async fn connect(config: &DatabaseConfig, application_name: &str) -> anyhow::Result<PgPool> {
    let options = connect_options(config.connection_string.expose(), application_name)?;
    let pool_options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout);

    let mut delay = Duration::from_millis(500);
    let mut attempt = 1u32;
    loop {
        match pool_options.clone().connect_with(options.clone()).await {
            Ok(pool) => return Ok(pool),
            Err(err) if attempt < 8 => {
                tracing::warn!(attempt, error = %err, "database not reachable yet, retrying");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(15));
                attempt += 1;
            }
            Err(err) => return Err(err).context("cannot connect to the database"),
        }
    }
}

/// Applies pending migrations. sqlx takes a PostgreSQL advisory lock, so concurrent replicas
/// starting together apply them only once.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("database migration failed (is the `vector` extension pre-created?)")?;
    Ok(())
}

/// Readiness probe: the database answers a trivial query.
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_host_urls_are_unchanged() {
        let url = "postgres://app:s3cr%40t@pg-primary:5432/iapolitique_prod?sslmode=disable";
        assert_eq!(normalize_connection_string(url), (url.to_owned(), false));
        let url = "postgresql://localhost/db";
        assert_eq!(normalize_connection_string(url), (url.to_owned(), false));
    }

    #[test]
    fn multi_host_urls_keep_the_first_host() {
        let (url, reduced) = normalize_connection_string(
            "postgresql://user:pass@pg-primary:5432,pg-standby1:5432,pg-standby2:5432/mydb?target_session_attrs=read-write&connect_timeout=10&application_name=myapp",
        );
        assert!(reduced);
        assert_eq!(
            url,
            "postgresql://user:pass@pg-primary:5432/mydb?application_name=myapp"
        );
    }

    #[test]
    fn drops_the_query_when_nothing_is_left() {
        let (url, reduced) =
            normalize_connection_string("postgres://u:p@h:5432/db?target_session_attrs=any");
        assert!(!reduced);
        assert_eq!(url, "postgres://u:p@h:5432/db");
    }

    #[test]
    fn handles_urls_without_path() {
        let (url, reduced) = normalize_connection_string("postgres://u@h1,h2");
        assert!(reduced);
        assert_eq!(url, "postgres://u@h1");
    }

    #[test]
    fn password_with_at_sign_and_comma() {
        // Reserved characters must be percent-encoded, but a raw '@' in the password is still
        // split on the last '@'.
        let (url, _) = normalize_connection_string("postgres://u:p@ss@h1:5432/db");
        assert_eq!(url, "postgres://u:p@ss@h1:5432/db");
    }

    #[test]
    fn connect_options_parse_and_hide_errors() {
        let options = connect_options("postgres://u:p@h1:5433,h2/db", "iapolitique-test").unwrap();
        assert_eq!(options.get_host(), "h1");
        assert_eq!(options.get_port(), 5433);
        assert_eq!(options.get_database(), Some("db"));
        assert_eq!(options.get_application_name(), Some("iapolitique-test"));

        let options =
            connect_options("postgres://u:p@h/db?application_name=custom", "ignored").unwrap();
        assert_eq!(options.get_application_name(), Some("custom"));

        let err = connect_options("postgres://u:topsecret@h:notaport/db", "x").unwrap_err();
        assert!(!err.to_string().contains("topsecret"));
    }
}

#[cfg(test)]
mod schema_tests;
