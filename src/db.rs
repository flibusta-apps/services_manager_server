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

pub async fn backfill_token_encryption(pool: &PgPool) {
    info!("Checking for legacy plaintext tokens to encrypt...");
    let rows = sqlx::query!(
        r#"SELECT id, token FROM services WHERE token_ciphertext IS NULL AND token IS NOT NULL"#
    )
    .fetch_all(pool)
    .await
    .expect("Failed to load services pending token encryption backfill");

    if rows.is_empty() {
        info!("No legacy plaintext tokens found; nothing to backfill");
        return;
    }
    info!(
        "Backfilling encrypted token columns for {} service(s)",
        rows.len()
    );

    for row in rows {
        let token = row.token.expect("filtered by WHERE token IS NOT NULL");
        let encrypted = crate::crypto::encrypt_token(&token);
        let hmac = crate::crypto::hmac_token(&token);
        sqlx::query!(
            r#"UPDATE services SET token_ciphertext = $1, token_nonce = $2, token_hmac = $3 WHERE id = $4"#,
            encrypted.ciphertext, encrypted.nonce, hmac, row.id
        )
        .execute(pool)
        .await
        .expect("Failed to persist backfilled token encryption");
    }
    info!("Token encryption backfill complete");
}

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
