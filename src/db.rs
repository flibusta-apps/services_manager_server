use crate::config::CONFIG;

use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
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
    let connect_options = PgConnectOptions::new()
        .host(&CONFIG.postgres_host)
        .port(CONFIG.postgres_port)
        .username(&CONFIG.postgres_user)
        .password(&CONFIG.postgres_password)
        .database(&CONFIG.postgres_db)
        .application_name(&CONFIG.application_name)
        .options([("statement_timeout", "10000"), ("lock_timeout", "5000")]);

    info!(
        "Connecting to Postgres as {} (max_connections={}, acquire_timeout={}s, max_lifetime={}s)",
        CONFIG.application_name,
        CONFIG.postgres_pool_max_connections,
        CONFIG.postgres_pool_acquire_timeout_sec,
        CONFIG.postgres_pool_max_lifetime_sec
    );

    PgPoolOptions::new()
        .max_connections(CONFIG.postgres_pool_max_connections)
        .acquire_timeout(std::time::Duration::from_secs(
            CONFIG.postgres_pool_acquire_timeout_sec,
        ))
        .max_lifetime(std::time::Duration::from_secs(
            CONFIG.postgres_pool_max_lifetime_sec,
        ))
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to Postgres — check POSTGRES_HOST/PORT/USER/PASSWORD/DB")
}
