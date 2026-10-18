//! Entry points of the long-running processes (AGENTS.md §13.1).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::Config;
use crate::db;
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
    };
    let worker = Worker::new(ctx, worker::default_registry(), options, liveness);
    let result = worker.run(shutdown).await;

    health_server.abort();
    pool.close().await;
    result
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
