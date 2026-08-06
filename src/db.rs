use crate::config::CONFIG;

use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing::info;

pub async fn run_migrations(pool: &PgPool) {
    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
    info!("Database migrations completed successfully");
}

pub async fn get_pg_pool() -> PgPool {
    let database_url: String = format!(
        "postgresql://{}:{}@{}:{}/{}?application_name={}",
        CONFIG.postgres_user,
        CONFIG.postgres_password,
        CONFIG.postgres_host,
        CONFIG.postgres_port,
        CONFIG.postgres_db,
        CONFIG.application_name
    );

    info!(
        "Connecting to Postgres as {} (max_connections={}, acquire_timeout={}s)",
        CONFIG.application_name,
        CONFIG.postgres_pool_max_connections,
        CONFIG.postgres_pool_acquire_timeout_sec
    );

    PgPoolOptions::new()
        .max_connections(CONFIG.postgres_pool_max_connections)
        .acquire_timeout(std::time::Duration::from_secs(
            CONFIG.postgres_pool_acquire_timeout_sec,
        ))
        .connect(&database_url)
        .await
        .expect("Failed to connect to Postgres — check POSTGRES_HOST/PORT/USER/PASSWORD/DB")
}
