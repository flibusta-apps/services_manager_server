pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod views;

use sentry::{integrations::debug_images::DebugImagesIntegration, types::Dsn, ClientOptions};
use sentry_tracing::EventFilter;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use std::{net::SocketAddr, str::FromStr};

async fn start_app() {
    // Validate configuration and secrets before touching DB / binding.
    let _ = &*config::CONFIG;
    crypto::init();

    // Initialize database pool
    let pool = db::get_pg_pool().await;

    // Run migrations
    db::run_migrations(&pool).await;

    let app = views::get_router(pool);

    let addr = SocketAddr::from(([0, 0, 0, 0], config::CONFIG.port));

    info!("Start webserver...");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
    info!("Webserver shutdown...")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[tokio::main]
async fn main() {
    let dsn = config::CONFIG
        .sentry_dsn
        .as_ref()
        .map(|dsn| Dsn::from_str(dsn).expect("Invalid SENTRY_DSN value"));

    let options = ClientOptions {
        dsn,
        ..Default::default()
    }
    .add_integration(DebugImagesIntegration::new());

    let _guard = sentry::init(options);

    let sentry_layer = sentry_tracing::layer().event_filter(|md| match md.level() {
        &tracing::Level::ERROR => EventFilter::Event,
        _ => EventFilter::Ignore,
    });

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(sentry_layer)
        .init();

    start_app().await;
}
