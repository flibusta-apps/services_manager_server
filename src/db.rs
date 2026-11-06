use crate::config::CONFIG;

use sqlx::{
    migrate::{Migrate, Migrator},
    postgres::{PgConnectOptions, PgPoolOptions},
    Connection, PgConnection, PgPool,
};
use tracing::info;

const ENCRYPTED_COLUMNS_VERSION: i64 = 20260806120000;

fn backfill_migrator(migrator: &Migrator) -> Migrator {
    Migrator {
        migrations: migrator
            .iter()
            .filter(|migration| migration.version <= ENCRYPTED_COLUMNS_VERSION)
            .cloned()
            .collect::<Vec<_>>()
            .into(),
        // Later applied versions are checked by the full migrator below.
        ignore_missing: true,
        locking: false,
        ..Migrator::DEFAULT
    }
}

pub async fn run_migrations(pool: &PgPool) {
    info!("Running database migrations...");
    let mut conn = pool
        .acquire()
        .await
        .expect("Failed to acquire migration connection");
    // Hold SQLx's own migration lock across BOTH migrators and the Rust backfill.
    // Otherwise concurrent startups could drop `token` while another is reading it.
    conn.lock()
        .await
        .expect("Failed to lock database migrations");
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_locking(false);
    backfill_migrator(&migrator)
        .run_direct(&mut *conn)
        .await
        .expect("Failed to run migrations before token backfill");
    backfill_token_encryption(&mut conn)
        .await
        .expect("Failed to backfill legacy token encryption");
    migrator
        .run_direct(&mut *conn)
        .await
        .expect("Failed to run migrations");
    conn.unlock()
        .await
        .expect("Failed to unlock database migrations");
    info!("Database migrations completed successfully");
}

async fn backfill_token_encryption(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    // Runtime queries are intentional: the legacy column is absent from the
    // current schema/offline cache, and must never be queried after finalization.
    let has_plaintext: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = 'services' AND column_name = 'token')",
    )
    .fetch_one(&mut *conn)
    .await?;
    if !has_plaintext {
        return Ok(());
    }

    info!("Checking for legacy plaintext tokens to encrypt...");
    let mut tx = conn.begin().await?;
    // Prevent concurrent writes while rebuilding partially populated rows.
    sqlx::query("LOCK TABLE services IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT id, token FROM services WHERE token IS NOT NULL AND \
         (token_ciphertext IS NULL OR token_nonce IS NULL OR token_hmac IS NULL)",
    )
    .fetch_all(&mut *tx)
    .await?;
    info!(
        "Backfilling encrypted token columns for {} service(s)",
        rows.len()
    );
    for (id, token) in rows {
        let encrypted = crate::crypto::encrypt_token(&token);
        let hmac = crate::crypto::hmac_token(&token);
        sqlx::query(
            "UPDATE services SET token_ciphertext = $1, token_nonce = $2, token_hmac = $3 WHERE id = $4",
        )
        .bind(encrypted.ciphertext)
        .bind(encrypted.nonce)
        .bind(hmac)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    info!("Token encryption backfill complete");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn init_test_crypto() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            // Only these DB tests use the env-backed singleton. Initialize it
            // once before any of them can access CONFIG or TOKEN_CRYPTO.
            for (name, value) in [
                ("API_KEY", "integration-test-api-key"),
                (
                    "TOKEN_ENC_KEY",
                    "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk=",
                ),
                ("POSTGRES_USER", "postgres"),
                ("POSTGRES_PASSWORD", "unused"),
                ("POSTGRES_HOST", "localhost"),
                ("POSTGRES_PORT", "5432"),
                ("POSTGRES_DB", "unused"),
            ] {
                std::env::set_var(name, value);
            }
            crate::crypto::init();
        });
    }

    const TOKENS: [&str; 2] = [
        "123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "987654321:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
    ];

    async fn assert_latest_schema(pool: &PgPool) {
        let applied: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(
            "SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        let expected: Vec<_> = sqlx::migrate!("./migrations")
            .iter()
            .map(|migration| (migration.version, migration.checksum.to_vec(), true))
            .collect();
        assert_eq!(applied, expected);

        let columns: Vec<(String, String)> = sqlx::query_as(
            "SELECT column_name, is_nullable FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = 'services'",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert!(!columns.iter().any(|(name, _)| name == "token"));
        for name in ["token_ciphertext", "token_nonce", "token_hmac"] {
            assert!(columns
                .iter()
                .any(|(column, nullable)| column == name && nullable == "NO"));
        }
    }

    async fn assert_upgraded_tokens(pool: &PgPool) {
        assert_latest_schema(pool).await;
        let rows: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = sqlx::query_as(
            "SELECT token_ciphertext, token_nonce, token_hmac FROM services ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), TOKENS.len());
        for ((ciphertext, nonce, hmac), token) in rows.iter().zip(TOKENS) {
            assert_eq!(
                crate::crypto::decrypt_token(ciphertext, nonce).unwrap(),
                token
            );
            assert_eq!(*hmac, crate::crypto::hmac_token(token));
        }
    }

    async fn upgrade_legacy_fixture(pool: &PgPool, partial: bool) {
        init_test_crypto();
        let full = sqlx::migrate!("./migrations");
        let legacy = Migrator {
            migrations: full
                .iter()
                .filter(|migration| migration.version < ENCRYPTED_COLUMNS_VERSION)
                .cloned()
                .collect::<Vec<_>>()
                .into(),
            ..Migrator::DEFAULT
        };
        legacy.run(pool).await.unwrap();
        for (index, token) in TOKENS.iter().enumerate() {
            sqlx::query(
                "INSERT INTO services (token, \"user\", status, created_time, cache, username) \
                 VALUES ($1, $2, 'approved', now(), 'original', 'test_bot')",
            )
            .bind(token)
            .bind(index as i64)
            .execute(pool)
            .await
            .unwrap();
        }
        if partial {
            // The transitional schema permits partial encrypted state. Leave
            // row one fully plaintext and give row two only its ciphertext.
            backfill_migrator(&full).run(pool).await.unwrap();
            let encrypted = crate::crypto::encrypt_token(TOKENS[1]);
            sqlx::query("UPDATE services SET token_ciphertext = $1 WHERE token = $2")
                .bind(encrypted.ciphertext)
                .bind(TOKENS[1])
                .execute(pool)
                .await
                .unwrap();
        }
        run_migrations(pool).await;
        assert_upgraded_tokens(pool).await;
    }

    #[sqlx::test(migrations = false)]
    async fn plaintext_schema_upgrades_to_latest(pool: PgPool) {
        upgrade_legacy_fixture(&pool, false).await;
    }

    #[sqlx::test(migrations = false)]
    async fn partially_encrypted_schema_upgrades_to_latest(pool: PgPool) {
        upgrade_legacy_fixture(&pool, true).await;
    }

    #[sqlx::test(migrations = false)]
    async fn latest_schema_migrations_are_noop(pool: PgPool) {
        upgrade_legacy_fixture(&pool, false).await;
        // Includes ciphertext/nonces and migration execution timestamps, so
        // re-encryption or reapplication cannot masquerade as a no-op.
        let snapshot = || async {
            sqlx::query_as::<_, (String, String)>(
                "SELECT (SELECT jsonb_agg(to_jsonb(s) ORDER BY id)::text FROM services s), \
                 (SELECT jsonb_agg(to_jsonb(m) ORDER BY version)::text FROM _sqlx_migrations m)",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        let before = snapshot().await;
        run_migrations(&pool).await;
        assert_eq!(snapshot().await, before);
        assert_upgraded_tokens(&pool).await;
    }

    #[test]
    fn backfill_stage_stops_before_guard_and_preserves_checksums() {
        let full = sqlx::migrate!("./migrations");
        let stage = backfill_migrator(&full);
        assert_eq!(
            stage.iter().last().unwrap().version,
            ENCRYPTED_COLUMNS_VERSION
        );
        assert!(!stage.version_exists(20260806130000));
        assert!(!stage.locking);
        for migration in stage.iter() {
            let original = full
                .iter()
                .find(|m| m.version == migration.version)
                .unwrap();
            assert_eq!(migration.checksum, original.checksum);
        }
    }
}
