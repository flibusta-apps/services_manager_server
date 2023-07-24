pub mod config;
pub mod db;
pub mod prisma;
pub mod views;

use tracing::info;

use std::net::SocketAddr;

async fn start_app() {
    let app = views::get_router().await;

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    info!("Start webserver...");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    info!("Webserver shutdown...")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let _guard = sentry::init(config::CONFIG.sentry_dsn.clone());

    start_app().await;
}
