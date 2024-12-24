use crate::config::CONFIG;

use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn get_pg_pool() -> PgPool {
    let database_url: String = format!(
        "postgresql://{}:{}@{}:{}/{}",
        CONFIG.postgres_user,
        CONFIG.postgres_password,
        CONFIG.postgres_host,
        CONFIG.postgres_port,
        CONFIG.postgres_db
    );

    PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .unwrap()
}
