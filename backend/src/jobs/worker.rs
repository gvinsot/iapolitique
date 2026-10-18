//! The worker loop: claims jobs, runs their handler with a lock heartbeat, records the outcome.
//!
//! Later phases plug their processing in by implementing [`JobHandler`] and registering it in
//! [`default_registry`]. Kinds that need Mistral are not claimed when `MISTRAL_API_KEY` is
//! missing: they stay queued and a warning is logged.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::{Semaphore, watch};
use tokio::time::Instant;

use super::health::Liveness;
use super::purge::PurgeHandler;
use super::schedule::{self, DailySchedule};
use super::{Job, JobError, JobKind, JobOutcome, queue};
use crate::ai::decision::Screener;
use crate::ai::mistral::MistralClient;
use crate::ai::prompts::Prompts;
use crate::config::{Config, WorkerConfig};

/// Future returned by [`JobHandler::handle`].
pub type HandlerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<JobOutcome, JobError>> + Send + 'a>>;

/// Shared resources available to handlers.
#[derive(Debug, Clone)]
pub struct JobContext {
    pub pool: PgPool,
    pub config: Arc<Config>,
    /// Versioned prompts, validated at startup.
    pub prompts: Arc<Prompts>,
    /// Screening retries when the decision service was unavailable (§5.3).
    pub screener: Screener,
    /// `None` without `MISTRAL_API_KEY`: the kinds that need it are not claimed.
    pub mistral: Option<MistralClient>,
}

/// Processing of one [`JobKind`].
pub trait JobHandler: Send + Sync + 'static {
    fn kind(&self) -> JobKind;

    /// Daily schedule on which the worker enqueues this kind automatically, if any.
    fn schedule(&self) -> Option<DailySchedule> {
        None
    }

    /// Processes a job. Must be idempotent: a job may run again after a crash. Errors must not
    /// contain user text, IPs or tokens (they are stored in `jobs.last_error`).
    fn handle<'a>(&'a self, ctx: &'a JobContext, job: &'a Job) -> HandlerFuture<'a>;
}

/// Handlers by kind.
#[derive(Default, Clone)]
pub struct Registry {
    handlers: BTreeMap<JobKind, Arc<dyn JobHandler>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.handlers.keys()).finish()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a handler.
    ///
    /// # Panics
    ///
    /// If a handler is already registered for the same kind (programming error).
    pub fn register(&mut self, handler: impl JobHandler) -> &mut Self {
        let kind = handler.kind();
        let previous = self.handlers.insert(kind, Arc::new(handler));
        assert!(previous.is_none(), "two handlers registered for {kind}");
        self
    }

    pub fn get(&self, kind: JobKind) -> Option<Arc<dyn JobHandler>> {
        self.handlers.get(&kind).cloned()
    }

    pub fn kinds(&self) -> Vec<JobKind> {
        self.handlers.keys().copied().collect()
    }

    pub fn schedules(&self) -> Vec<DailySchedule> {
        self.handlers
            .values()
            .filter_map(|h| h.schedule())
            .collect()
    }

    /// Kinds this worker may claim: without Mistral, kinds that need it are left queued.
    pub fn claimable_kinds(&self, mistral_available: bool) -> Vec<JobKind> {
        self.kinds()
            .into_iter()
            .filter(|kind| mistral_available || !kind.requires_mistral())
            .collect()
    }
}

/// Handlers of the production worker.
pub fn default_registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(PurgeHandler);
    registry
}

/// Tuning of the loop.
#[derive(Debug, Clone)]
pub struct WorkerOptions {
    /// Identifier written in `jobs.locked_by`.
    pub id: String,
    pub concurrency: usize,
    pub poll_interval: Duration,
    pub lock_timeout: Duration,
    /// Period of the maintenance tasks (stale locks, schedules).
    pub maintenance_interval: Duration,
    /// Time given to running jobs at shutdown.
    pub shutdown_grace: Duration,
}

impl WorkerOptions {
    pub fn from_config(config: &WorkerConfig) -> Self {
        let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".to_owned());
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        Self {
            id: format!("{host}-{}", &suffix[..8]),
            concurrency: config.concurrency,
            poll_interval: config.poll_interval,
            lock_timeout: config.job_lock_timeout,
            maintenance_interval: Duration::from_secs(30),
            shutdown_grace: Duration::from_secs(25),
        }
    }
}

pub struct Worker {
    ctx: Arc<JobContext>,
    registry: Arc<Registry>,
    options: WorkerOptions,
    liveness: Liveness,
}

impl Worker {
    pub fn new(
        ctx: JobContext,
        registry: Registry,
        options: WorkerOptions,
        liveness: Liveness,
    ) -> Self {
        Self {
            ctx: Arc::new(ctx),
            registry: Arc::new(registry),
            options,
            liveness,
        }
    }

    /// Runs until `shutdown` becomes `true` (or its sender is dropped), then waits up to
    /// `shutdown_grace` for the running jobs.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = &self.ctx.pool;
        let mistral = self.ctx.config.mistral.api_key.is_some();
        let kinds = self.registry.claimable_kinds(mistral);
        let idle: Vec<&str> = self
            .registry
            .kinds()
            .into_iter()
            .filter(|k| !kinds.contains(k))
            .map(JobKind::as_str)
            .collect();
        if !idle.is_empty() {
            tracing::warn!(
                kinds = idle.join(","),
                "MISTRAL_API_KEY is not set: AI jobs stay queued"
            );
        }
        let schedules = self.registry.schedules();
        schedule::ensure(pool, &schedules).await?;
        tracing::info!(
            worker_id = %self.options.id,
            concurrency = self.options.concurrency,
            kinds = kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(","),
            "worker started"
        );

        let slots = Arc::new(Semaphore::new(self.options.concurrency));
        let mut next_maintenance = Instant::now();
        loop {
            self.liveness.tick();
            if *shutdown.borrow() {
                break;
            }
            if Instant::now() >= next_maintenance {
                self.maintenance(&schedules).await;
                next_maintenance = Instant::now() + self.options.maintenance_interval;
            }

            let permit = tokio::select! {
                permit = slots.clone().acquire_owned() => permit?,
                _ = shutdown.changed() => break,
            };
            match queue::claim(pool, &self.options.id, &kinds).await {
                Ok(Some(job)) => {
                    let task = process(
                        Arc::clone(&self.ctx),
                        Arc::clone(&self.registry),
                        job,
                        self.options.id.clone(),
                        self.options.lock_timeout,
                    );
                    tokio::spawn(async move {
                        task.await;
                        drop(permit);
                    });
                    continue;
                }
                Ok(None) => drop(permit),
                Err(err) => {
                    drop(permit);
                    tracing::error!(error = %err, "cannot claim a job");
                }
            }
            tokio::select! {
                () = tokio::time::sleep(self.options.poll_interval) => {}
                _ = shutdown.changed() => break,
            }
        }

        tracing::info!("worker stopping, waiting for running jobs");
        let all = u32::try_from(self.options.concurrency).unwrap_or(u32::MAX);
        match tokio::time::timeout(self.options.shutdown_grace, slots.acquire_many(all)).await {
            Ok(_) => tracing::info!("worker stopped"),
            Err(_) => tracing::warn!(
                "running jobs did not finish in time; their lock will expire and they will be retried"
            ),
        }
        Ok(())
    }

    async fn maintenance(&self, schedules: &[DailySchedule]) {
        let pool = &self.ctx.pool;
        match queue::reclaim_stale(pool, self.options.lock_timeout).await {
            Ok(r) if r.requeued > 0 || r.dead > 0 => {
                tracing::warn!(requeued = r.requeued, dead = r.dead, "reclaimed stale jobs");
            }
            Ok(_) => {}
            Err(err) => tracing::error!(error = %err, "cannot reclaim stale jobs"),
        }
        match schedule::enqueue_due(pool, schedules).await {
            Ok(ids) => {
                for id in ids {
                    tracing::info!(job_id = id, "scheduled job enqueued");
                }
            }
            Err(err) => tracing::error!(error = %err, "cannot enqueue scheduled jobs"),
        }
    }
}

/// Runs one claimed job and records its outcome.
async fn process(
    ctx: Arc<JobContext>,
    registry: Arc<Registry>,
    job: Job,
    worker_id: String,
    lock_timeout: Duration,
) {
    let started = Instant::now();
    let (job_id, kind, attempt) = (job.id, job.kind, job.attempts);
    let pool = ctx.pool.clone();

    let heartbeat = tokio::spawn({
        let pool = pool.clone();
        let worker_id = worker_id.clone();
        async move {
            let period = (lock_timeout / 3).max(Duration::from_secs(1));
            loop {
                tokio::time::sleep(period).await;
                match queue::heartbeat(&pool, job_id, &worker_id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(job_id, "job lock lost");
                        break;
                    }
                    Err(err) => tracing::warn!(job_id, error = %err, "heartbeat failed"),
                }
            }
        }
    });

    let result = match registry.get(kind) {
        None => Ok(Err(JobError::Permanent(format!("no handler for {kind}")))),
        Some(handler) => {
            let job = job.clone();
            let ctx = Arc::clone(&ctx);
            tokio::spawn(async move { handler.handle(&ctx, &job).await }).await
        }
    };
    heartbeat.abort();

    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let recorded = match result {
        Ok(Ok(JobOutcome::Done)) => queue::complete(&pool, job_id, &worker_id)
            .await
            .map(|ok| (if ok { "succeeded" } else { "lock_lost" }).to_owned()),
        Ok(Ok(JobOutcome::RetryAt(at))) => queue::reschedule(&pool, job_id, &worker_id, at)
            .await
            .map(|ok| (if ok { "rescheduled" } else { "lock_lost" }).to_owned()),
        Ok(Err(err)) => {
            let permanent = matches!(err, JobError::Permanent(_));
            queue::fail(&pool, &job, &worker_id, &err.to_string(), permanent)
                .await
                .map(|o| format!("{o:?}"))
        }
        Err(join_error) => {
            let message = if join_error.is_panic() {
                "handler panicked"
            } else {
                "handler cancelled"
            };
            queue::fail(&pool, &job, &worker_id, message, false)
                .await
                .map(|o| format!("{o:?}"))
        }
    };
    match recorded {
        Ok(outcome) => {
            tracing::info!(job_id, kind = %kind, attempt, elapsed_ms, outcome, "job processed")
        }
        Err(err) => tracing::error!(
            job_id,
            kind = %kind,
            error = %err,
            "cannot record the job outcome; the lock will expire and the job will be retried"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Role;
    use crate::jobs::{NewJob, queue::enqueue};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_config(mistral: bool) -> Arc<Config> {
        let mut env = vec![("DATABASE_CONNECTION_STRING", "postgres://unused/db")];
        if mistral {
            env.push(("MISTRAL_API_KEY", "test-key"));
        }
        Arc::new(
            Config::from_lookup(Role::Worker, |key| {
                env.iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| (*v).to_owned())
            })
            .unwrap(),
        )
    }

    fn test_screener() -> Screener {
        let config = test_config(false);
        Screener::new(
            crate::ai::decision::DecisionClient::new(&config.decision_service).unwrap(),
            crate::ai::prompts::test_prompts(),
        )
    }

    fn options() -> WorkerOptions {
        WorkerOptions {
            id: "test-worker".into(),
            concurrency: 2,
            poll_interval: Duration::from_millis(50),
            lock_timeout: Duration::from_secs(60),
            maintenance_interval: Duration::from_secs(60),
            shutdown_grace: Duration::from_secs(5),
        }
    }

    /// Scripted handler: behaviour chosen by the `mode` field of the payload.
    struct Scripted {
        kind: JobKind,
        calls: Arc<AtomicUsize>,
    }

    impl JobHandler for Scripted {
        fn kind(&self) -> JobKind {
            self.kind
        }

        fn handle<'a>(&'a self, _ctx: &'a JobContext, job: &'a Job) -> HandlerFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                match job.payload["mode"].as_str() {
                    Some("transient") => Err(JobError::Transient("boom".into())),
                    Some("permanent") => Err(JobError::Permanent("bad".into())),
                    Some("later") => Ok(JobOutcome::RetryAt(
                        chrono::Utc::now() + chrono::Duration::hours(1),
                    )),
                    Some("panic") => panic!("scripted panic"),
                    _ => Ok(JobOutcome::Done),
                }
            })
        }
    }

    #[test]
    fn registry_filters_mistral_kinds() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = default_registry();
        registry.register(Scripted {
            kind: JobKind::AnalyzeIdea,
            calls,
        });
        assert_eq!(registry.kinds(), vec![JobKind::AnalyzeIdea, JobKind::Purge]);
        assert_eq!(registry.claimable_kinds(false), vec![JobKind::Purge]);
        assert_eq!(registry.claimable_kinds(true).len(), 2);
        assert_eq!(registry.schedules(), vec![crate::jobs::purge::SCHEDULE]);
        assert_eq!(format!("{registry:?}"), "[AnalyzeIdea, Purge]");
    }

    #[test]
    #[should_panic(expected = "two handlers registered for purge")]
    fn duplicate_registration_panics() {
        let mut registry = default_registry();
        registry.register(PurgeHandler);
    }

    async fn job_states(pool: &PgPool) -> Vec<(String, String, i32)> {
        sqlx::query_as("SELECT payload->>'mode', status, attempts FROM jobs ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn processes_jobs_and_records_outcomes(pool: PgPool) {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::new();
        registry.register(Scripted {
            kind: JobKind::ScreenIdea,
            calls: Arc::clone(&calls),
        });
        for mode in ["ok", "transient", "permanent", "later", "panic"] {
            enqueue(
                &pool,
                &NewJob::new(JobKind::ScreenIdea).payload(serde_json::json!({ "mode": mode })),
            )
            .await
            .unwrap();
        }

        let ctx = JobContext {
            pool: pool.clone(),
            config: test_config(false),
            prompts: crate::ai::prompts::test_prompts(),
            screener: test_screener(),
            mistral: None,
        };
        let worker = Worker::new(
            ctx,
            registry,
            options(),
            Liveness::new(Duration::from_secs(60)),
        );
        let (stop, shutdown) = watch::channel(false);
        let running = tokio::spawn(worker.run(shutdown));

        let deadline = Instant::now() + Duration::from_secs(10);
        while calls.load(Ordering::SeqCst) < 5 {
            assert!(Instant::now() < deadline, "jobs not processed in time");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        stop.send(true).unwrap();
        running.await.unwrap().unwrap();

        assert_eq!(
            job_states(&pool).await,
            vec![
                ("ok".into(), "succeeded".into(), 1),
                ("transient".into(), "queued".into(), 1),
                ("permanent".into(), "dead".into(), 1),
                ("later".into(), "queued".into(), 0),
                ("panic".into(), "queued".into(), 1),
            ]
        );
        let (panic_error,): (Option<String>,) =
            sqlx::query_as("SELECT last_error FROM jobs WHERE payload->>'mode' = 'panic'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(panic_error.as_deref(), Some("handler panicked"));
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn mistral_jobs_stay_queued_without_api_key(pool: PgPool) {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = default_registry();
        registry.register(Scripted {
            kind: JobKind::AnalyzeIdea,
            calls: Arc::clone(&calls),
        });
        enqueue(&pool, &NewJob::new(JobKind::AnalyzeIdea))
            .await
            .unwrap();
        enqueue(&pool, &NewJob::new(JobKind::Purge)).await.unwrap();

        let ctx = JobContext {
            pool: pool.clone(),
            config: test_config(false),
            prompts: crate::ai::prompts::test_prompts(),
            screener: test_screener(),
            mistral: None,
        };
        let worker = Worker::new(
            ctx,
            registry,
            options(),
            Liveness::new(Duration::from_secs(60)),
        );
        let (stop, shutdown) = watch::channel(false);
        let running = tokio::spawn(worker.run(shutdown));

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let (done,): (i64,) = sqlx::query_as(
                "SELECT count(*) FROM jobs WHERE kind = 'purge' AND status = 'succeeded'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if done == 1 {
                break;
            }
            assert!(Instant::now() < deadline, "purge not processed in time");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        stop.send(true).unwrap();
        running.await.unwrap().unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let (status,): (String,) =
            sqlx::query_as("SELECT status FROM jobs WHERE kind = 'analyze_idea'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "queued");
        // The purge schedule was created at startup.
        let (schedules,): (i64,) = sqlx::query_as("SELECT count(*) FROM job_schedules")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(schedules, 1);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATOR")]
    async fn shutdown_waits_for_running_jobs(pool: PgPool) {
        struct Slow;
        impl JobHandler for Slow {
            fn kind(&self) -> JobKind {
                JobKind::GenerateExports
            }
            fn handle<'a>(&'a self, _: &'a JobContext, _: &'a Job) -> HandlerFuture<'a> {
                Box::pin(async {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    Ok(JobOutcome::Done)
                })
            }
        }
        let mut registry = Registry::new();
        registry.register(Slow);
        enqueue(&pool, &NewJob::new(JobKind::GenerateExports))
            .await
            .unwrap();

        let ctx = JobContext {
            pool: pool.clone(),
            config: test_config(true),
            prompts: crate::ai::prompts::test_prompts(),
            screener: test_screener(),
            mistral: None,
        };
        let worker = Worker::new(
            ctx,
            registry,
            options(),
            Liveness::new(Duration::from_secs(60)),
        );
        let (stop, shutdown) = watch::channel(false);
        let running = tokio::spawn(worker.run(shutdown));
        // Wait until the job is running, then stop.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let (running_jobs,): (i64,) =
                sqlx::query_as("SELECT count(*) FROM jobs WHERE status = 'running'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if running_jobs == 1 {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        stop.send(true).unwrap();
        running.await.unwrap().unwrap();
        let (status,): (String,) = sqlx::query_as("SELECT status FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "succeeded");
    }
}
