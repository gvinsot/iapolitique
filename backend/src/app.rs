//! Entry points of the long-running processes (AGENTS.md §13.1).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::ai::decision::{DecisionClient, Screener};
use crate::ai::mistral::{MistralClient, MistralError};
use crate::ai::prompts::Prompts;
use crate::antispam::altcha::Altcha;
use crate::config::Config;
use crate::db;
use crate::http::{self, AppState};
use crate::jobs::health::{self, Liveness};
use crate::jobs::worker::{self, JobContext, Worker, WorkerOptions};

/// Watch channel flipped to `true` on SIGTERM / Ctrl+C.
pub fn shutdown_channel() -> watch::Receiver<bool> {
    let (sender, receiver) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = sender.send(true);
    });
    receiver
}

/// Resolves when the shutdown channel flips (or its sender is dropped).
pub async fn wait_for_shutdown(mut shutdown: watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stop| *stop).await;
}

/// `iapolitique worker`: job consumer plus its internal health endpoint.
pub async fn run_worker(config: Config) -> anyhow::Result<()> {
    let prompts = load_prompts(&config)?;
    let config = Arc::new(config);
    let pool = db::connect(&config.database, "iapolitique-worker").await?;
    // Normally done by the api; harmless here (advisory lock) and avoids a worker running
    // against a schema it does not know when it starts first.
    db::migrate(&pool).await?;

    let options = WorkerOptions::from_config(&config.worker);
    // A healthy loop ticks at least every poll interval; leave room for a slow maintenance pass.
    let max_silence = (options.poll_interval * 10).max(Duration::from_secs(120));
    let liveness = Liveness::new(max_silence);

    let shutdown = shutdown_channel();
    let address = SocketAddr::new(config.http.bind_address, config.http.worker_health_port);
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("cannot listen on {address}"))?;
    tracing::info!(%address, "worker health endpoint listening");
    let health_server = tokio::spawn({
        let shutdown = shutdown.clone();
        let router = health::router(liveness.clone());
        async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(wait_for_shutdown(shutdown))
                .await
        }
    });

    let ctx = JobContext {
        pool: pool.clone(),
        config: Arc::clone(&config),
        screener: Screener::new(
            DecisionClient::new(&config.decision_service)
                .context("cannot build the decision service client")?,
            Arc::clone(&prompts),
        ),
        mistral: match MistralClient::new(&config.mistral) {
            Ok(client) => Some(client),
            Err(MistralError::MissingApiKey) => None,
            Err(err) => return Err(err).context("cannot build the Mistral client"),
        },
        prompts,
    };
    let worker = Worker::new(ctx, worker::default_registry(), options, liveness);
    let result = worker.run(shutdown).await;

    health_server.abort();
    pool.close().await;
    result
}

/// Loads and validates `PROMPTS_DIR` (§13.4): an invalid file prevents the process from starting.
pub fn load_prompts(config: &Config) -> anyhow::Result<Arc<Prompts>> {
    let prompts = Prompts::load(&config.prompts_dir)?;
    tracing::info!(
        dir = %config.prompts_dir.display(),
        screening_version = %prompts.screening.version,
        taxonomy_version = %prompts.taxonomy.version,
        "prompts loaded"
    );
    Ok(Arc::new(prompts))
}

/// Resolves on SIGTERM (Docker stop) or Ctrl+C.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "cannot listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::error!(error = %err, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutdown requested");
}

/// `iapolitique api`: public API on PUBLIC_PORT (8080) and admin API on ADMIN_PORT (8081).
pub async fn run_api(config: Config) -> anyhow::Result<()> {
    let admin_token = config
        .http
        .admin_api_token
        .clone()
        .context("ADMIN_API_TOKEN is required by the api")?;
    let prompts = load_prompts(&config)?;
    let config = Arc::new(config);
    let pool = db::connect(&config.database, "iapolitique-api").await?;
    db::migrate(&pool).await?;
    tracing::info!("database migrations up to date");

    let state = AppState {
        pool: pool.clone(),
        config: Arc::clone(&config),
        screener: Screener::new(
            DecisionClient::new(&config.decision_service)
                .context("cannot build the decision service client")?,
            Arc::clone(&prompts),
        ),
        altcha: Arc::new(Altcha::new(
            config
                .antispam
                .altcha_hmac_key
                .clone()
                .context("ALTCHA_HMAC_KEY is required by the api")?,
        )),
        prompts,
    };
    let public = http::public_router(state.clone());
    let admin = http::admin_router(state, admin_token);

    let bind = config.http.bind_address;
    let public_address = SocketAddr::new(bind, config.http.public_port);
    let admin_address = SocketAddr::new(bind, config.http.admin_port);
    let public_listener = TcpListener::bind(public_address)
        .await
        .with_context(|| format!("cannot listen on {public_address}"))?;
    let admin_listener = TcpListener::bind(admin_address)
        .await
        .with_context(|| format!("cannot listen on {admin_address}"))?;
    tracing::info!(%public_address, %admin_address, "api listening");

    let shutdown = shutdown_channel();
    let public_server = axum::serve(
        public_listener,
        public.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(wait_for_shutdown(shutdown.clone()));
    let admin_server = axum::serve(
        admin_listener,
        admin.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(wait_for_shutdown(shutdown));
    let result = tokio::try_join!(
        async { public_server.await.context("public server failed") },
        async { admin_server.await.context("admin server failed") },
    );
    pool.close().await;
    result.map(|_| tracing::info!("api stopped"))
}
