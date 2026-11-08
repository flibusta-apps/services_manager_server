pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod views;

use sentry::{integrations::debug_images::DebugImagesIntegration, types::Dsn, ClientOptions};
use tracing::info;
use tracing_subscriber::{
    filter::LevelFilter, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
    EnvFilter, Layer,
};

use std::{net::SocketAddr, str::FromStr};

async fn start_app() {
    // Validate configuration and secrets before touching DB / binding.
    let _ = &*config::CONFIG;
    crypto::init();
    if config::CONFIG.metrics_api_key.is_none() {
        tracing::warn!(
            "METRICS_API_KEY is unset; /metrics uses API_KEY, which also permits token export and mutations. Configure a separate metrics key."
        );
    }

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

fn sentry_options(dsn: Option<Dsn>) -> ClientOptions {
    let mut options = ClientOptions::default();
    options.dsn = dsn;
    options.add_integration(DebugImagesIntegration::new())
}

fn sentry_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    sentry_tracing::ErrorLayer::new().with_filter(LevelFilter::ERROR)
}

fn sentry_span_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    sentry_tracing::SpanLayer::new().with_filter(LevelFilter::INFO)
}

#[tokio::main]
async fn main() {
    let dsn = config::CONFIG
        .sentry_dsn
        .as_ref()
        .map(|dsn| Dsn::from_str(dsn).expect("Invalid SENTRY_DSN value"));

    let _guard = sentry::init(sentry_options(dsn));

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(sentry_span_layer())
        .with(sentry_layer())
        .init();

    start_app().await;
}

#[cfg(test)]
mod telemetry_tests {
    use super::*;

    #[test]
    fn tracing_errors_reach_the_configured_sentry_client() {
        let dsn = Dsn::from_str("https://public@example.invalid/1").unwrap();
        let events = sentry::test::with_captured_events_options(
            || {
                let subscriber = tracing_subscriber::registry()
                    .with(sentry_span_layer())
                    .with(sentry_layer());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::trace!("ignored trace");
                    tracing::debug!("ignored debug");
                    tracing::info!("ignored info");
                    tracing::warn!("ignored warning");
                    tracing::error!(
                        tags.component = "services-manager",
                        "captured tracing error"
                    );
                });
            },
            sentry_options(Some(dsn)),
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, sentry::Level::Error);
        assert_eq!(events[0].message.as_deref(), Some("captured tracing error"));
        assert_eq!(events[0].tags["component"], "services-manager");
        assert!(events[0].breadcrumbs.is_empty());
    }

    #[test]
    fn tracing_retains_info_span_context_but_excludes_debug_spans() {
        let events = sentry::test::with_captured_events_options(
            || {
                let subscriber = tracing_subscriber::registry()
                    .with(sentry_span_layer())
                    .with(sentry_layer());
                tracing::subscriber::with_default(subscriber, || {
                    assert!(sentry::configure_scope(|scope| scope.get_span()).is_none());
                    let _info = tracing::info_span!("retained info").entered();
                    assert!(sentry::configure_scope(|scope| scope.get_span()).is_some());
                    tracing::error!("info context");
                    {
                        let _debug = tracing::debug_span!("excluded debug").entered();
                        assert!(sentry::configure_scope(|scope| scope.get_span()).is_some());
                        tracing::error!("same info context");
                    }
                    assert!(sentry::configure_scope(|scope| scope.get_span()).is_some());
                });
                assert!(sentry::configure_scope(|scope| scope.get_span()).is_none());
            },
            sentry_options(Some(
                Dsn::from_str("https://public@example.invalid/1").unwrap(),
            )),
        );
        assert_eq!(events.len(), 2);
        let trace = events[0].contexts.get("trace").expect("INFO trace context");
        assert_eq!(Some(trace), events[1].contexts.get("trace"));
        assert!(events.iter().all(|event| event.breadcrumbs.is_empty()));
    }
}
