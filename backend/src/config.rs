//! Runtime configuration.
//!
//! Every setting comes from an environment variable (see AGENTS.md §10.3 and §13.1). At startup,
//! [`load_secrets_into_env`] copies each file of `/run/secrets/` into the environment when the
//! variable is not already set (PulsarCD convention), then [`Config::from_env`] parses and
//! validates everything once. Secrets are wrapped in [`Secret`] so they never end up in logs.

use std::collections::BTreeMap;
use std::fmt;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use crate::domain::money::{MicroEur, parse_eur_to_micro};

/// Directory where Docker Swarm mounts secrets.
pub const DEFAULT_SECRETS_DIR: &str = "/run/secrets";

/// Minimum length accepted for `ADMIN_API_TOKEN` and `ALTCHA_HMAC_KEY`.
pub const MIN_SECRET_LEN: usize = 32;

/// Process role: decides which variables are mandatory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Public (8080) and admin (8081) HTTP APIs.
    Api,
    /// Background job consumer.
    Worker,
    /// One-shot migration runner.
    Migrate,
}

/// A sensitive value. Its `Debug` and `Display` implementations never print the content.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret value. Call sites must never log the result.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Output format of the logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Text,
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "text" | "pretty" => Ok(Self::Text),
            other => Err(format!("expected `json` or `text`, got `{other}`")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub worker: WorkerConfig,
    pub decision_service: DecisionServiceConfig,
    pub mistral: MistralConfig,
    pub antispam: AntispamConfig,
    /// Cosine similarity at or above which a new idea is merged into an existing one (§5.4).
    pub dedup_similarity_threshold: f64,
    /// Directory holding the versioned prompts (§13.4).
    pub prompts_dir: PathBuf,
    pub log: LogConfig,
    /// Public host (`TRAEFIK_HOST`): canonical URLs, origin accepted by the anti-CSRF check.
    pub public_host: Option<String>,
    /// Admin host (`ADMIN_HOST`): origin accepted by the anti-CSRF check of both ports.
    pub admin_host: Option<String>,
    /// Publisher details served by `GET /api/legal` (§7.5).
    pub legal: LegalConfig,
}

/// Identity and contacts of the publisher (LCEN art. 1-1, AGENTS.md §7.5), shown by the legal
/// pages through `GET /api/legal`. Not secret, but never committed (public repository): they
/// come from the `LEGAL_*` variables. Optional so that a deployment without them still starts;
/// the api then warns and the pages show « coordonnées momentanément indisponibles ».
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegalConfig {
    /// `LEGAL_PUBLISHER_NAME`: name of the publisher (a natural person).
    pub publisher_name: Option<String>,
    /// `LEGAL_PUBLISHER_ADDRESS`: postal address.
    pub publisher_address: Option<String>,
    /// `LEGAL_PUBLISHER_PHONE`: phone number.
    pub publisher_phone: Option<String>,
    /// `LEGAL_CONTACT_EMAIL`: contact address (also in `security.txt`).
    pub contact_email: Option<String>,
    /// `LEGAL_PRIVACY_EMAIL`: address for personal data requests.
    pub privacy_email: Option<String>,
}

impl LegalConfig {
    /// Names of the `LEGAL_*` variables that are not set.
    pub fn missing(&self) -> Vec<&'static str> {
        [
            ("LEGAL_PUBLISHER_NAME", &self.publisher_name),
            ("LEGAL_PUBLISHER_ADDRESS", &self.publisher_address),
            ("LEGAL_PUBLISHER_PHONE", &self.publisher_phone),
            ("LEGAL_CONTACT_EMAIL", &self.contact_email),
            ("LEGAL_PRIVACY_EMAIL", &self.privacy_email),
        ]
        .into_iter()
        .filter(|(_, value)| value.is_none())
        .map(|(key, _)| key)
        .collect()
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// `DATABASE_CONNECTION_STRING`, a `postgres://` URL. Always required.
    pub connection_string: Secret,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
    /// `DATABASE_STATEMENT_TIMEOUT_SECS`: `statement_timeout` of the api's connections (§14.5).
    pub statement_timeout: Duration,
    /// `DATABASE_JOB_STATEMENT_TIMEOUT_SECS`: `statement_timeout` of the worker's connections,
    /// longer (exports, purge, dashboards of the jobs).
    pub job_statement_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub bind_address: IpAddr,
    pub public_port: u16,
    pub admin_port: u16,
    pub worker_health_port: u16,
    pub request_timeout: Duration,
    pub body_limit_bytes: usize,
    /// Number of reverse proxies (Traefik) in front of the api that append to
    /// `X-Forwarded-For`. See `http::client_ip`.
    pub trusted_proxy_hops: usize,
    /// Bearer token protecting the admin port. Required for [`Role::Api`].
    pub admin_api_token: Option<Secret>,
    /// `ALLOWED_DEV_ORIGINS`: origins accepted by the anti-CSRF check besides
    /// `https://TRAEFIK_HOST` and `https://ADMIN_HOST` (`http://localhost:5173` for the Vite dev
    /// server…), normalized as `scheme://host[:port]` in lower case. Empty by default.
    pub allowed_dev_origins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub concurrency: usize,
    pub poll_interval: Duration,
    /// A running job whose lock has not been refreshed for this long is considered abandoned.
    pub job_lock_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct DecisionServiceConfig {
    pub url: String,
    /// `DECISION_SERVICE_TIMEOUT_SECS`: synchronous screening by the api, while the author waits.
    pub timeout: Duration,
    /// `DECISION_SERVICE_JOB_TIMEOUT_SECS`: screening retried by the worker's `screen_*` jobs.
    /// Longer, so that a long text slowed down by concurrent requests is settled (AGENTS.md
    /// §5.3) instead of timing out at every attempt.
    pub job_timeout: Duration,
}

impl DecisionServiceConfig {
    /// The same service with the timeout of the worker's jobs.
    pub fn for_jobs(&self) -> Self {
        Self {
            url: self.url.clone(),
            timeout: self.job_timeout,
            job_timeout: self.job_timeout,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MistralConfig {
    /// Optional: without it the worker leaves AI jobs queued and logs a warning.
    pub api_key: Option<Secret>,
    pub base_url: String,
    pub analysis_model: String,
    pub review_model: String,
    pub embed_model: String,
    pub daily_budget: MicroEur,
    pub prices: MistralPrices,
}

/// Unit prices in micro-euros, configurable because Mistral changes them (§6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MistralPrices {
    pub analysis_input_per_mtok: MicroEur,
    pub analysis_output_per_mtok: MicroEur,
    pub review_input_per_mtok: MicroEur,
    pub review_output_per_mtok: MicroEur,
    pub embed_per_mtok: MicroEur,
    pub web_search_per_call: MicroEur,
}

#[derive(Debug, Clone)]
pub struct AntispamConfig {
    /// HMAC key of the ALTCHA proof-of-work. Required for [`Role::Api`].
    pub altcha_hmac_key: Option<Secret>,
    pub ideas_per_hour: u32,
    pub ideas_per_day: u32,
    pub contributions_per_day: u32,
    pub reports_per_day: u32,
}

#[derive(Debug, Clone)]
pub struct LogConfig {
    /// `RUST_LOG` directive.
    pub filter: String,
    pub format: LogFormat,
}

/// Startup configuration error: lists every problem at once.
#[derive(Debug, thiserror::Error)]
#[error("invalid configuration:\n  - {}", .problems.join("\n  - "))]
pub struct ConfigError {
    pub problems: Vec<String>,
}

impl Config {
    /// Reads the configuration from the process environment.
    pub fn from_env(role: Role) -> Result<Self, ConfigError> {
        Self::from_lookup(role, |key| std::env::var(key).ok())
    }

    /// Reads the configuration through `lookup` (used by tests to avoid touching the process
    /// environment). Empty values are treated as unset.
    pub fn from_lookup(
        role: Role,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let mut env = Reader::new(lookup);

        let connection_string = env.secret("DATABASE_CONNECTION_STRING");
        if connection_string.is_none() {
            env.missing("DATABASE_CONNECTION_STRING");
        }
        let database = DatabaseConfig {
            connection_string: connection_string.unwrap_or_else(|| Secret::new("")),
            max_connections: env.parse_in("DATABASE_MAX_CONNECTIONS", 10u32, 1..=200),
            acquire_timeout: Duration::from_secs(env.parse_in(
                "DATABASE_ACQUIRE_TIMEOUT_SECS",
                5u64,
                1..=120,
            )),
            statement_timeout: Duration::from_secs(env.parse_in(
                "DATABASE_STATEMENT_TIMEOUT_SECS",
                10u64,
                1..=600,
            )),
            job_statement_timeout: Duration::from_secs(env.parse_in(
                "DATABASE_JOB_STATEMENT_TIMEOUT_SECS",
                120u64,
                1..=3_600,
            )),
        };

        let admin_api_token = env.secret("ADMIN_API_TOKEN");
        let http = HttpConfig {
            bind_address: env.parse("BIND_ADDRESS", IpAddr::from([0, 0, 0, 0])),
            public_port: env.parse_in("PUBLIC_PORT", 8080u16, 1..=u16::MAX),
            admin_port: env.parse_in("ADMIN_PORT", 8081u16, 1..=u16::MAX),
            worker_health_port: env.parse_in("WORKER_HEALTH_PORT", 8082u16, 1..=u16::MAX),
            request_timeout: Duration::from_secs(env.parse_in(
                "HTTP_REQUEST_TIMEOUT_SECS",
                30u64,
                1..=600,
            )),
            body_limit_bytes: env.parse_in(
                "HTTP_BODY_LIMIT_BYTES",
                64 * 1024usize,
                1024..=10 << 20,
            ),
            trusted_proxy_hops: env.parse_in("TRUSTED_PROXY_HOPS", 1usize, 0..=10),
            admin_api_token,
            allowed_dev_origins: env.origins("ALLOWED_DEV_ORIGINS"),
        };

        let worker = WorkerConfig {
            concurrency: env.parse_in("WORKER_CONCURRENCY", 2usize, 1..=64),
            poll_interval: Duration::from_millis(env.parse_in(
                "WORKER_POLL_INTERVAL_MS",
                2_000u64,
                50..=60_000,
            )),
            job_lock_timeout: Duration::from_secs(env.parse_in(
                "JOB_LOCK_TIMEOUT_SECS",
                300u64,
                30..=86_400,
            )),
        };

        let decision_service = DecisionServiceConfig {
            url: env.url(
                "DECISION_SERVICE_URL",
                "http://llamacpp-decision-service:8000/v1/systemone",
            ),
            timeout: Duration::from_secs(env.parse_in(
                "DECISION_SERVICE_TIMEOUT_SECS",
                5u64,
                1..=60,
            )),
            job_timeout: Duration::from_secs(env.parse_in(
                "DECISION_SERVICE_JOB_TIMEOUT_SECS",
                30u64,
                1..=120,
            )),
        };
        if decision_service.job_timeout < decision_service.timeout {
            env.problem(
                "DECISION_SERVICE_JOB_TIMEOUT_SECS must not be below DECISION_SERVICE_TIMEOUT_SECS"
                    .to_owned(),
            );
        }

        let analysis_model = env.string("MISTRAL_ANALYSIS_MODEL", "mistral-large-latest");
        let review_model = env.string("MISTRAL_REVIEW_MODEL", &analysis_model);
        let analysis_input = env.eur("MISTRAL_ANALYSIS_INPUT_EUR_PER_MTOK", "2");
        let analysis_output = env.eur("MISTRAL_ANALYSIS_OUTPUT_EUR_PER_MTOK", "6");
        let prices = MistralPrices {
            analysis_input_per_mtok: analysis_input,
            analysis_output_per_mtok: analysis_output,
            review_input_per_mtok: env.eur_or("MISTRAL_REVIEW_INPUT_EUR_PER_MTOK", analysis_input),
            review_output_per_mtok: env
                .eur_or("MISTRAL_REVIEW_OUTPUT_EUR_PER_MTOK", analysis_output),
            embed_per_mtok: env.eur("MISTRAL_EMBED_EUR_PER_MTOK", "0.1"),
            web_search_per_call: env.eur("MISTRAL_WEB_SEARCH_EUR_PER_CALL", "0.03"),
        };
        let mistral = MistralConfig {
            api_key: env.secret("MISTRAL_API_KEY"),
            base_url: env.url("MISTRAL_API_BASE_URL", "https://api.mistral.ai"),
            analysis_model,
            review_model,
            embed_model: env.string("MISTRAL_EMBED_MODEL", "mistral-embed"),
            daily_budget: env.eur("MISTRAL_DAILY_BUDGET_EUR", "5"),
            prices,
        };

        let antispam = AntispamConfig {
            altcha_hmac_key: env.secret("ALTCHA_HMAC_KEY"),
            ideas_per_hour: env.parse_in("RATE_LIMIT_IDEAS_PER_HOUR", 5u32, 1..=10_000),
            ideas_per_day: env.parse_in("RATE_LIMIT_IDEAS_PER_DAY", 20u32, 1..=100_000),
            contributions_per_day: env.parse_in(
                "RATE_LIMIT_CONTRIBUTIONS_PER_DAY",
                20u32,
                1..=100_000,
            ),
            reports_per_day: env.parse_in("RATE_LIMIT_REPORTS_PER_DAY", 20u32, 1..=100_000),
        };
        if antispam.ideas_per_hour > antispam.ideas_per_day {
            env.problem(
                "RATE_LIMIT_IDEAS_PER_HOUR must not exceed RATE_LIMIT_IDEAS_PER_DAY".to_owned(),
            );
        }

        let dedup_similarity_threshold = env.parse("DEDUP_SIMILARITY_THRESHOLD", 0.95f64);
        if !(0.0..=1.0).contains(&dedup_similarity_threshold) {
            env.problem("DEDUP_SIMILARITY_THRESHOLD must be between 0 and 1".to_owned());
        }

        let log = LogConfig {
            filter: env.string("RUST_LOG", "info,sqlx=warn"),
            format: env.parse("LOG_FORMAT", LogFormat::Json),
        };

        let legal = LegalConfig {
            publisher_name: env.bounded_line("LEGAL_PUBLISHER_NAME", 200),
            publisher_address: env.bounded_line("LEGAL_PUBLISHER_ADDRESS", 300),
            publisher_phone: env.bounded_line("LEGAL_PUBLISHER_PHONE", 40),
            contact_email: env.email("LEGAL_CONTACT_EMAIL"),
            privacy_email: env.email("LEGAL_PRIVACY_EMAIL"),
        };

        let config = Config {
            database,
            http,
            worker,
            decision_service,
            mistral,
            antispam,
            dedup_similarity_threshold,
            prompts_dir: PathBuf::from(env.string("PROMPTS_DIR", "/app/prompts")),
            log,
            public_host: env.opt_string("TRAEFIK_HOST"),
            admin_host: env.opt_string("ADMIN_HOST"),
            legal,
        };

        if role == Role::Api {
            env.require_strong_secret("ADMIN_API_TOKEN", config.http.admin_api_token.as_ref());
            env.require_strong_secret("ALTCHA_HMAC_KEY", config.antispam.altcha_hmac_key.as_ref());
            let ports = [config.http.public_port, config.http.admin_port];
            if ports[0] == ports[1] {
                env.problem("PUBLIC_PORT and ADMIN_PORT must differ".to_owned());
            }
        }

        env.finish().map(|()| config)
    }
}

/// Collects parse errors so that a misconfigured deployment reports everything in one go.
struct Reader<F> {
    lookup: F,
    problems: Vec<String>,
}

impl<F: Fn(&str) -> Option<String>> Reader<F> {
    fn new(lookup: F) -> Self {
        Self {
            lookup,
            problems: Vec::new(),
        }
    }

    fn raw(&self, key: &str) -> Option<String> {
        (self.lookup)(key).filter(|v| !v.trim().is_empty())
    }

    fn problem(&mut self, message: String) {
        self.problems.push(message);
    }

    fn missing(&mut self, key: &str) {
        self.problem(format!("{key} is required but not set"));
    }

    fn opt_string(&self, key: &str) -> Option<String> {
        self.raw(key).map(|v| v.trim().to_owned())
    }

    /// Optional single-line text of at most `max` characters.
    fn bounded_line(&mut self, key: &str, max: usize) -> Option<String> {
        let value = self.opt_string(key)?;
        if value.chars().count() > max || value.chars().any(char::is_control) {
            self.problem(format!(
                "{key}: expected a single line of at most {max} characters"
            ));
            return None;
        }
        Some(value)
    }

    /// Optional e-mail address.
    fn email(&mut self, key: &str) -> Option<String> {
        let value = self.opt_string(key)?;
        match crate::domain::validation::validate_email(Some(&value)) {
            Ok(Some(email)) if email == value => Some(email),
            _ => {
                self.problem(format!("{key}: expected an e-mail address"));
                None
            }
        }
    }

    fn string(&self, key: &str, default: &str) -> String {
        self.opt_string(key).unwrap_or_else(|| default.to_owned())
    }

    fn secret(&self, key: &str) -> Option<Secret> {
        // Secrets are not trimmed beyond the trailing newline already removed when loading
        // /run/secrets, except for surrounding whitespace that cannot be intentional.
        self.raw(key).map(|v| Secret::new(v.trim()))
    }

    fn parse<T>(&mut self, key: &str, default: T) -> T
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        match self.raw(key) {
            None => default,
            Some(raw) => match raw.trim().parse::<T>() {
                Ok(value) => value,
                Err(err) => {
                    self.problem(format!("{key}: invalid value ({err})"));
                    default
                }
            },
        }
    }

    fn parse_in<T>(&mut self, key: &str, default: T, range: std::ops::RangeInclusive<T>) -> T
    where
        T: FromStr + PartialOrd + fmt::Display + Copy,
        T::Err: fmt::Display,
    {
        let value = self.parse(key, default);
        if range.contains(&value) {
            value
        } else {
            self.problem(format!(
                "{key}: {value} is out of range ({}..={})",
                range.start(),
                range.end()
            ));
            default
        }
    }

    /// Comma-separated list of web origins (`http(s)://host[:port]`, no path), normalized.
    fn origins(&mut self, key: &str) -> Vec<String> {
        let Some(raw) = self.opt_string(key) else {
            return Vec::new();
        };
        let mut origins = Vec::new();
        for item in raw
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            match normalize_origin(item) {
                Some(origin) => origins.push(origin),
                None => self.problem(format!(
                    "{key}: `{item}` is not an origin (expected http(s)://host[:port])"
                )),
            }
        }
        origins
    }

    fn url(&mut self, key: &str, default: &str) -> String {
        let value = self.string(key, default);
        if !(value.starts_with("http://") || value.starts_with("https://")) {
            self.problem(format!("{key}: expected an http(s) URL"));
        }
        value.trim_end_matches('/').to_owned()
    }

    fn eur(&mut self, key: &str, default: &str) -> MicroEur {
        let default = parse_eur_to_micro(default).expect("valid default amount");
        self.eur_or(key, default)
    }

    fn eur_or(&mut self, key: &str, default: MicroEur) -> MicroEur {
        match self.raw(key) {
            None => default,
            Some(raw) => match parse_eur_to_micro(&raw) {
                Ok(value) if value.0 >= 0 => value,
                Ok(_) => {
                    self.problem(format!("{key}: must not be negative"));
                    default
                }
                Err(err) => {
                    self.problem(format!("{key}: invalid amount ({err})"));
                    default
                }
            },
        }
    }

    fn require_strong_secret(&mut self, key: &str, value: Option<&Secret>) {
        match value {
            None => self.missing(key),
            Some(secret) if secret.expose().chars().count() < MIN_SECRET_LEN => {
                self.problem(format!(
                    "{key} must be at least {MIN_SECRET_LEN} characters long"
                ));
            }
            Some(_) => {}
        }
    }

    fn finish(self) -> Result<(), ConfigError> {
        if self.problems.is_empty() {
            Ok(())
        } else {
            Err(ConfigError {
                problems: self.problems,
            })
        }
    }
}

/// Reads the secret files of `dir` and returns `(name, value)` pairs, sorted by name.
///
/// Mirrors PulsarCD's `load_docker_secrets_into_env`: file names that are not valid
/// environment variable identifiers are ignored, a single trailing newline is removed and
/// unreadable files are skipped. A missing directory yields an empty list.
pub fn read_secrets_dir(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut secrets = BTreeMap::new();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_env_name(&name) {
            continue;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(mut value) = std::fs::read_to_string(&path) else {
            continue;
        };
        if value.ends_with('\n') {
            value.pop();
            if value.ends_with('\r') {
                value.pop();
            }
        }
        secrets.insert(name, value);
    }
    secrets.into_iter().collect()
}

/// Copies every secret of `dir` into the process environment unless the variable is already
/// set. Returns the names that were loaded (never the values).
///
/// # Safety
///
/// Mutates the process environment: must be called while the process is single-threaded,
/// i.e. at the very beginning of `main`, before the async runtime starts.
pub unsafe fn load_secrets_into_env(dir: &Path) -> Vec<String> {
    let mut loaded = Vec::new();
    for (name, value) in read_secrets_dir(dir) {
        if std::env::var_os(&name).is_some() {
            continue;
        }
        // SAFETY: the caller guarantees that no other thread reads or writes the environment.
        unsafe { std::env::set_var(&name, value) };
        loaded.push(name);
    }
    loaded
}

/// A web origin `http(s)://host[:port]` in lower case without trailing slash, `None` when
/// `value` is not one (path, query, credentials, spaces…).
pub fn normalize_origin(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/').to_ascii_lowercase();
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))?;
    let valid = !rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    valid.then_some(value)
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn api_env() -> Vec<(&'static str, &'static str)> {
        vec![
            ("DATABASE_CONNECTION_STRING", "postgres://u:p@db:5432/app"),
            ("ADMIN_API_TOKEN", TOKEN),
            ("ALTCHA_HMAC_KEY", TOKEN),
        ]
    }

    #[test]
    fn defaults_are_applied() {
        let config = Config::from_lookup(Role::Api, lookup(&api_env())).unwrap();
        assert_eq!(config.http.public_port, 8080);
        assert_eq!(config.http.admin_port, 8081);
        assert_eq!(config.http.worker_health_port, 8082);
        assert_eq!(config.http.trusted_proxy_hops, 1);
        assert_eq!(config.prompts_dir, PathBuf::from("/app/prompts"));
        assert_eq!(config.mistral.analysis_model, "mistral-large-latest");
        assert_eq!(config.mistral.review_model, "mistral-large-latest");
        assert_eq!(config.mistral.embed_model, "mistral-embed");
        assert_eq!(config.mistral.daily_budget, MicroEur(5_000_000));
        assert_eq!(
            config.mistral.prices.review_input_per_mtok,
            config.mistral.prices.analysis_input_per_mtok
        );
        assert!(config.mistral.api_key.is_none());
        assert_eq!(config.antispam.ideas_per_hour, 5);
        assert_eq!(config.antispam.ideas_per_day, 20);
        assert_eq!(config.antispam.contributions_per_day, 20);
        assert_eq!(config.antispam.reports_per_day, 20);
        assert_eq!(
            config.decision_service.url,
            "http://llamacpp-decision-service:8000/v1/systemone"
        );
        assert_eq!(config.decision_service.timeout, Duration::from_secs(5));
        assert_eq!(config.decision_service.job_timeout, Duration::from_secs(30));
        let jobs = config.decision_service.for_jobs();
        assert_eq!(jobs.timeout, Duration::from_secs(30));
        assert_eq!(jobs.url, config.decision_service.url);
        assert_eq!(config.log.format, LogFormat::Json);
        assert!(config.http.allowed_dev_origins.is_empty());
        assert_eq!(config.database.statement_timeout, Duration::from_secs(10));
        assert_eq!(
            config.database.job_statement_timeout,
            Duration::from_secs(120)
        );
        assert!(config.public_host.is_none());
    }

    #[test]
    fn dev_origins_are_normalized_and_validated() {
        let mut env = api_env();
        env.push((
            "ALLOWED_DEV_ORIGINS",
            " http://localhost:5173, HTTPS://Dev.Example.org/ ,,http://[::1]:4173",
        ));
        let config = Config::from_lookup(Role::Api, lookup(&env)).unwrap();
        assert_eq!(
            config.http.allowed_dev_origins,
            [
                "http://localhost:5173",
                "https://dev.example.org",
                "http://[::1]:4173"
            ]
        );
        for bad in [
            "localhost:5173",
            "http://localhost:5173/app",
            "ftp://x",
            "http://",
            "http://a b",
            "http://user@host",
            "null",
        ] {
            let mut env = api_env();
            env.push(("ALLOWED_DEV_ORIGINS", bad));
            let err = Config::from_lookup(Role::Api, lookup(&env)).unwrap_err();
            assert!(err.problems[0].contains("ALLOWED_DEV_ORIGINS"), "{bad}");
        }
    }

    #[test]
    fn overrides_are_parsed() {
        let mut env = api_env();
        env.extend([
            ("PUBLIC_PORT", "9000"),
            ("MISTRAL_ANALYSIS_MODEL", "mistral-medium-latest"),
            ("MISTRAL_DAILY_BUDGET_EUR", "12.5"),
            ("MISTRAL_REVIEW_INPUT_EUR_PER_MTOK", "0,4"),
            ("DEDUP_SIMILARITY_THRESHOLD", "0.9"),
            ("PROMPTS_DIR", "/tmp/prompts"),
            ("TRAEFIK_HOST", "iapolitique.fr"),
            ("LOG_FORMAT", "text"),
            ("MISTRAL_API_KEY", "abc"),
            (
                "DECISION_SERVICE_URL",
                "http://192.168.1.27:8001/v1/systemone/",
            ),
        ]);
        let config = Config::from_lookup(Role::Api, lookup(&env)).unwrap();
        assert_eq!(config.http.public_port, 9000);
        assert_eq!(config.mistral.review_model, "mistral-medium-latest");
        assert_eq!(config.mistral.daily_budget, MicroEur(12_500_000));
        assert_eq!(
            config.mistral.prices.review_input_per_mtok,
            MicroEur(400_000)
        );
        assert!((config.dedup_similarity_threshold - 0.9).abs() < f64::EPSILON);
        assert_eq!(config.prompts_dir, PathBuf::from("/tmp/prompts"));
        assert_eq!(config.public_host.as_deref(), Some("iapolitique.fr"));
        assert_eq!(config.log.format, LogFormat::Text);
        assert_eq!(config.mistral.api_key.unwrap().expose(), "abc");
        assert_eq!(
            config.decision_service.url,
            "http://192.168.1.27:8001/v1/systemone"
        );
    }

    #[test]
    fn legal_details_are_optional_and_validated() {
        let config = Config::from_lookup(Role::Api, lookup(&api_env())).unwrap();
        assert_eq!(config.legal, LegalConfig::default());
        assert_eq!(config.legal.missing().len(), 5);

        let mut env = api_env();
        env.extend([
            ("LEGAL_PUBLISHER_NAME", " Camille Exemple "),
            ("LEGAL_PUBLISHER_ADDRESS", "1 rue de l'Exemple, 75000 Paris"),
            ("LEGAL_PUBLISHER_PHONE", "01 23 45 67 89"),
            ("LEGAL_CONTACT_EMAIL", "contact@example.org"),
            ("LEGAL_PRIVACY_EMAIL", "rgpd@example.org"),
        ]);
        let config = Config::from_lookup(Role::Api, lookup(&env)).unwrap();
        assert_eq!(
            config.legal.publisher_name.as_deref(),
            Some("Camille Exemple")
        );
        assert_eq!(
            config.legal.privacy_email.as_deref(),
            Some("rgpd@example.org")
        );
        assert!(config.legal.missing().is_empty());

        let mut env = api_env();
        let long: &'static str = Box::leak("x".repeat(201).into_boxed_str());
        env.extend([
            ("LEGAL_CONTACT_EMAIL", "not an e-mail"),
            ("LEGAL_PUBLISHER_NAME", long),
        ]);
        let err = Config::from_lookup(Role::Api, lookup(&env)).unwrap_err();
        assert!(
            err.problems
                .iter()
                .any(|p| p.contains("LEGAL_CONTACT_EMAIL"))
        );
        assert!(
            err.problems
                .iter()
                .any(|p| p.contains("LEGAL_PUBLISHER_NAME"))
        );
    }

    #[test]
    fn database_is_always_required() {
        for role in [Role::Api, Role::Worker, Role::Migrate] {
            let err = Config::from_lookup(role, lookup(&[("ADMIN_API_TOKEN", TOKEN)])).unwrap_err();
            assert!(
                err.problems
                    .iter()
                    .any(|p| p.contains("DATABASE_CONNECTION_STRING")),
                "{role:?}: {err}"
            );
        }
    }

    #[test]
    fn worker_does_not_need_api_secrets() {
        let config = Config::from_lookup(
            Role::Worker,
            lookup(&[("DATABASE_CONNECTION_STRING", "postgres://db/app")]),
        )
        .unwrap();
        assert!(config.http.admin_api_token.is_none());
        assert!(config.antispam.altcha_hmac_key.is_none());
    }

    #[test]
    fn api_requires_admin_token_and_altcha_key() {
        let err = Config::from_lookup(
            Role::Api,
            lookup(&[("DATABASE_CONNECTION_STRING", "postgres://db/app")]),
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("ADMIN_API_TOKEN is required"), "{text}");
        assert!(text.contains("ALTCHA_HMAC_KEY is required"), "{text}");
    }

    #[test]
    fn api_rejects_short_secrets() {
        let err = Config::from_lookup(
            Role::Api,
            lookup(&[
                ("DATABASE_CONNECTION_STRING", "postgres://db/app"),
                ("ADMIN_API_TOKEN", "short"),
                ("ALTCHA_HMAC_KEY", TOKEN),
            ]),
        )
        .unwrap_err();
        assert_eq!(err.problems.len(), 1);
        assert!(err.problems[0].contains("ADMIN_API_TOKEN must be at least 32"));
    }

    #[test]
    fn empty_values_count_as_unset() {
        let mut env = api_env();
        env.push(("MISTRAL_API_KEY", ""));
        env.push(("PUBLIC_PORT", "  "));
        let config = Config::from_lookup(Role::Api, lookup(&env)).unwrap();
        assert!(config.mistral.api_key.is_none());
        assert_eq!(config.http.public_port, 8080);
    }

    #[test]
    fn every_invalid_value_is_reported() {
        let mut env = api_env();
        env.extend([
            ("PUBLIC_PORT", "http"),
            ("WORKER_CONCURRENCY", "0"),
            ("MISTRAL_DAILY_BUDGET_EUR", "-3"),
            ("DEDUP_SIMILARITY_THRESHOLD", "1.5"),
            ("DECISION_SERVICE_URL", "llamacpp:8000"),
            ("LOG_FORMAT", "xml"),
            ("RATE_LIMIT_IDEAS_PER_HOUR", "50"),
            ("DECISION_SERVICE_TIMEOUT_SECS", "20"),
            ("DECISION_SERVICE_JOB_TIMEOUT_SECS", "10"),
        ]);
        let err = Config::from_lookup(Role::Api, lookup(&env)).unwrap_err();
        assert_eq!(err.problems.len(), 8, "{err}");
        assert!(
            err.problems
                .iter()
                .any(|p| p.contains("DECISION_SERVICE_JOB_TIMEOUT_SECS")),
            "{err}"
        );
    }

    #[test]
    fn same_public_and_admin_port_is_rejected() {
        let mut env = api_env();
        env.push(("ADMIN_PORT", "8080"));
        let err = Config::from_lookup(Role::Api, lookup(&env)).unwrap_err();
        assert!(err.to_string().contains("must differ"));
    }

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let config = Config::from_lookup(Role::Api, lookup(&api_env())).unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains(TOKEN));
        assert!(!debug.contains("postgres://u:p@db"));
        assert!(debug.contains("REDACTED"));
        assert_eq!(Secret::new("x").to_string(), "[REDACTED]");
    }

    #[test]
    fn secrets_dir_is_read_like_pulsarcd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ADMIN_API_TOKEN"), "token\n").unwrap();
        std::fs::write(dir.path().join("CRLF_KEY"), "value\r\n").unwrap();
        std::fs::write(dir.path().join("KEEP_SPACES"), "  spaced  ").unwrap();
        std::fs::write(dir.path().join("two-words"), "ignored").unwrap();
        std::fs::write(dir.path().join("9STARTS_WITH_DIGIT"), "ignored").unwrap();
        std::fs::create_dir(dir.path().join("SUBDIR")).unwrap();

        let secrets = read_secrets_dir(dir.path());
        assert_eq!(
            secrets,
            vec![
                ("ADMIN_API_TOKEN".to_owned(), "token".to_owned()),
                ("CRLF_KEY".to_owned(), "value".to_owned()),
                ("KEEP_SPACES".to_owned(), "  spaced  ".to_owned()),
            ]
        );
    }

    #[test]
    fn missing_secrets_dir_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_secrets_dir(&dir.path().join("absent")).is_empty());
    }

    #[test]
    fn env_name_validation() {
        assert!(is_env_name("DATABASE_CONNECTION_STRING"));
        assert!(is_env_name("_x1"));
        assert!(!is_env_name(""));
        assert!(!is_env_name("1A"));
        assert!(!is_env_name("A-B"));
        assert!(!is_env_name("A.B"));
    }
}
