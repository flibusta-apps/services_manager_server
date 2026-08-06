use axum::{
    extract::Path,
    http::{self, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Extension, Json, Router,
};
use axum_prometheus::PrometheusMetricLayer;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use tower_http::trace::{self, TraceLayer};
use tracing::Level;

use crate::config::CONFIG;
use crate::error::AppError;

pub type Database = Extension<PgPool>;

const BOTS_COUNT_LIMIT: i64 = 5;

// Mirrors book_bot's registration status (`book_bot/src/bots/registration/register.rs`,
// which only ever sends "approved") and its `BotCache` enum
// (`book_bot/src/bots_manager/bot_manager_client.rs`). The DB columns are
// `status VARCHAR(12)` / `cache VARCHAR(12)`.
const ALLOWED_STATUSES: &[&str] = &["approved"];
const ALLOWED_CACHE_VALUES: &[&str] = &["original", "cache", "no_cache"];

fn mask_token(token: &str) -> String {
    format!("{}…", &token[..token.len().min(8)])
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Validates the Telegram bot token format: `^\d+:[A-Za-z0-9_-]{35}$`.
fn is_valid_telegram_token_format(token: &str) -> bool {
    let Some((id_part, secret_part)) = token.split_once(':') else {
        return false;
    };

    if id_part.is_empty() || !id_part.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }

    if secret_part.len() != 35 {
        return false;
    }

    secret_part
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn validate_create_service(data: &CreateServiceData) -> Result<(), String> {
    if data.token.is_empty()
        || data.token.len() > 128
        || !is_valid_telegram_token_format(&data.token)
    {
        return Err("invalid token format".to_string());
    }

    if !ALLOWED_STATUSES.contains(&data.status.as_str()) {
        return Err("status must be one of: approved".to_string());
    }

    if !ALLOWED_CACHE_VALUES.contains(&data.cache.as_str()) {
        return Err("cache must be one of: original, cache, no_cache".to_string());
    }

    if data.username.is_empty() || data.username.len() > 64 {
        return Err("username must be 1-64 characters".to_string());
    }

    Ok(())
}

#[derive(Serialize)]
pub struct Service {
    pub id: i32,
    pub token: String,
    pub user: i64,
    pub status: String,
    pub created_time: DateTime<chrono::Local>,
    pub cache: String,
    pub username: String,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("id", &self.id)
            .field("token", &mask_token(&self.token))
            .field("user", &self.user)
            .field("status", &self.status)
            .field("created_time", &self.created_time)
            .field("cache", &self.cache)
            .field("username", &self.username)
            .finish()
    }
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ServiceInfo {
    pub id: i32,
    pub user: i64,
    pub status: String,
    pub created_time: DateTime<chrono::Local>,
    pub cache: String,
    pub username: String,
}

#[derive(sqlx::FromRow)]
struct ServiceRow {
    pub id: i32,
    pub token_ciphertext: Vec<u8>,
    pub token_nonce: Vec<u8>,
    pub user: i64,
    pub status: String,
    pub created_time: DateTime<chrono::Local>,
    pub cache: String,
    pub username: String,
}

impl ServiceRow {
    fn try_into_service(self) -> Result<Service, String> {
        let token = crate::crypto::decrypt_token(&self.token_ciphertext, &self.token_nonce)?;
        Ok(Service {
            id: self.id,
            token,
            user: self.user,
            status: self.status,
            created_time: self.created_time,
            cache: self.cache,
            username: self.username,
        })
    }
}

async fn get_services(db: Database) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query_as!(
        ServiceRow,
        r#"
        SELECT id, token_ciphertext AS "token_ciphertext!", token_nonce AS "token_nonce!", "user", status, created_time, cache, username FROM services
        "#
    )
    .fetch_all(&db.0)
    .await?;

    let services: Vec<Service> = rows
        .into_iter()
        .map(|row| row.try_into_service())
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(services).into_response())
}

async fn get_service(Path(id): Path<i32>, db: Database) -> Result<impl IntoResponse, AppError> {
    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        SELECT id, "user", status, created_time, cache, username FROM services WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&db.0)
    .await?;

    Ok(match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

async fn delete_service(Path(id): Path<i32>, db: Database) -> Result<impl IntoResponse, AppError> {
    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        DELETE FROM services WHERE id = $1 RETURNING id, "user", status, created_time, cache, username
        "#,
        id
    )
    .fetch_optional(&db.0)
    .await?;

    Ok(match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

#[derive(Deserialize)]
pub struct CreateServiceData {
    pub token: String,
    pub user: i64,
    pub status: String,
    pub cache: String,
    pub username: String,
}

impl std::fmt::Debug for CreateServiceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateServiceData")
            .field("token", &mask_token(&self.token))
            .field("user", &self.user)
            .field("status", &self.status)
            .field("cache", &self.cache)
            .field("username", &self.username)
            .finish()
    }
}

/// POST / — create a service (bot registration).
///
/// Status code contract (relied on by the `book_bot` consumer, see
/// `book_bot/src/bots/registration/register.rs`):
/// - `200` — created.
/// - `402 Payment Required` — the per-user bot count limit (`BOTS_COUNT_LIMIT`)
///   was reached. Not a real payment flow; this is an established, if
///   non-standard, "limit reached" signal that `book_bot` already parses.
/// - `409 Conflict` — the token is already registered (unique constraint on
///   `token_hmac`).
/// - `422 Unprocessable Entity` — the request body failed validation (invalid
///   token format, disallowed `status`/`cache` value, or `username` out of
///   range).
async fn create_service(
    db: Database,
    Json(data): Json<CreateServiceData>,
) -> Result<impl IntoResponse, AppError> {
    if let Err(msg) = validate_create_service(&data) {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse { error: msg }),
        )
            .into_response());
    }

    let CreateServiceData {
        token,
        user,
        status,
        cache,
        username,
    } = data;

    let mut tx = db.0.begin().await?;

    // Serialize concurrent bot-count-limit checks and inserts for the same
    // user, regardless of connection-pool size.
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", user)
        .execute(&mut *tx)
        .await?;

    let exist_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM services WHERE "user" = $1
        "#,
        user
    )
    .fetch_one(&mut *tx)
    .await?
    .unwrap_or(0);

    if exist_count >= BOTS_COUNT_LIMIT {
        return Ok(StatusCode::PAYMENT_REQUIRED.into_response());
    };

    let token_hmac = crate::crypto::hmac_token(&token);

    let encrypted = crate::crypto::encrypt_token(&token);

    let insert_result = sqlx::query_as!(
        ServiceInfo,
        r#"
        INSERT INTO services (token_ciphertext, token_nonce, token_hmac, "user", status, cache, username, created_time)
        VALUES ($1, $2, $3, $4, $5, $6, $7, now())
        RETURNING id, "user", status, created_time, cache, username
        "#,
        encrypted.ciphertext,
        encrypted.nonce,
        token_hmac,
        user,
        status,
        cache,
        username
    )
    .fetch_one(&mut *tx)
    .await;

    let service = match insert_result {
        Ok(v) => v,
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            return Ok(StatusCode::CONFLICT.into_response());
        }
        Err(e) => return Err(AppError::from(e)),
    };

    tx.commit().await?;

    Ok(Json(service).into_response())
}

async fn update_status(
    Path(id): Path<i32>,
    db: Database,
    Json(status): Json<String>,
) -> Result<impl IntoResponse, AppError> {
    if !ALLOWED_STATUSES.contains(&status.as_str()) {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "status must be one of: approved".to_string(),
            }),
        )
            .into_response());
    }

    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        UPDATE services SET status = $1 WHERE id = $2 RETURNING id, "user", status, created_time, cache, username
        "#,
        status,
        id
    )
    .fetch_optional(&db.0)
    .await?;

    Ok(match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

async fn update_cache(
    Path(id): Path<i32>,
    db: Database,
    Json(cache): Json<String>,
) -> Result<impl IntoResponse, AppError> {
    if !ALLOWED_CACHE_VALUES.contains(&cache.as_str()) {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "cache must be one of: original, cache, no_cache".to_string(),
            }),
        )
            .into_response());
    }

    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        UPDATE services SET cache = $1 WHERE id = $2 RETURNING id, "user", status, created_time, cache, username
        "#,
        cache,
        id
    )
    .fetch_optional(&db.0)
    .await?;

    Ok(match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

async fn ready_check(db: Database) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&db.0).await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::error!("Readiness check failed: {}", e);
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

//

async fn auth(req: Request<axum::body::Body>, next: Next) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let auth_header = if let Some(auth_header) = auth_header {
        auth_header
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let token = if let Some(token) = auth_header.strip_prefix("Bearer ") {
        token
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let token_bytes = token.as_bytes();
    let api_key_bytes = CONFIG.api_key.as_bytes();

    let is_valid =
        token_bytes.len() == api_key_bytes.len() && bool::from(token_bytes.ct_eq(api_key_bytes));

    if !is_valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

pub fn get_router(client: PgPool) -> Router {
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let health_router = Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(ready_check))
        .layer(Extension(client.clone()));

    let app_router = Router::new()
        .route("/", get(get_services))
        .route("/{id}/", get(get_service))
        .route("/{id}/", delete(delete_service))
        .route("/", post(create_service))
        .route("/{id}/update_status", patch(update_status))
        .route("/{id}/update_cache", patch(update_cache))
        .layer(middleware::from_fn(auth))
        .layer(Extension(client))
        .layer(prometheus_layer);

    let metric_router = Router::new()
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .layer(middleware::from_fn(auth));

    Router::new()
        .merge(app_router)
        .merge(metric_router)
        .merge(health_router)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::DEBUG))
                .on_failure(trace::DefaultOnFailure::new().level(Level::ERROR)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_data() -> CreateServiceData {
        CreateServiceData {
            token: "123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            user: 1,
            status: "approved".to_string(),
            cache: "original".to_string(),
            username: "some_username".to_string(),
        }
    }

    #[test]
    fn telegram_token_format_valid() {
        assert!(is_valid_telegram_token_format(
            "123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
        // 11-digit id part is also allowed.
        assert!(is_valid_telegram_token_format(
            "12345678901:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
    }

    #[test]
    fn telegram_token_format_missing_colon() {
        assert!(!is_valid_telegram_token_format(
            "123456789AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
    }

    #[test]
    fn telegram_token_format_non_numeric_id() {
        assert!(!is_valid_telegram_token_format(
            "12345678a:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
    }

    #[test]
    fn telegram_token_format_secret_too_short() {
        // 34 chars instead of 35.
        assert!(!is_valid_telegram_token_format(
            "123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
    }

    #[test]
    fn telegram_token_format_secret_too_long() {
        // 36 chars instead of 35.
        assert!(!is_valid_telegram_token_format(
            "123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
    }

    #[test]
    fn telegram_token_format_disallowed_chars_in_secret() {
        assert!(!is_valid_telegram_token_format(
            "123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA!!"
        ));
    }

    #[test]
    fn telegram_token_format_empty() {
        assert!(!is_valid_telegram_token_format(""));
    }

    #[test]
    fn validate_create_service_valid() {
        assert!(validate_create_service(&make_valid_data()).is_ok());

        // Other allowed cache values.
        let mut data = make_valid_data();
        data.cache = "cache".to_string();
        assert!(validate_create_service(&data).is_ok());

        let mut data = make_valid_data();
        data.cache = "no_cache".to_string();
        assert!(validate_create_service(&data).is_ok());
    }

    #[test]
    fn validate_create_service_bad_token_format() {
        let mut data = make_valid_data();
        data.token = "not-a-valid-token".to_string();
        assert!(validate_create_service(&data).is_err());
    }

    #[test]
    fn validate_create_service_disallowed_status() {
        let mut data = make_valid_data();
        data.status = "pending".to_string();
        assert!(validate_create_service(&data).is_err());
    }

    #[test]
    fn validate_create_service_disallowed_cache() {
        let mut data = make_valid_data();
        data.cache = "invalid_cache".to_string();
        assert!(validate_create_service(&data).is_err());
    }

    #[test]
    fn validate_create_service_empty_username() {
        let mut data = make_valid_data();
        data.username = "".to_string();
        assert!(validate_create_service(&data).is_err());
    }

    #[test]
    fn validate_create_service_username_too_long() {
        let mut data = make_valid_data();
        data.username = "a".repeat(65);
        assert!(validate_create_service(&data).is_err());
    }

    #[test]
    fn validate_create_service_token_too_long() {
        let mut data = make_valid_data();
        data.token = format!("123456789:{}", "A".repeat(200));
        assert!(validate_create_service(&data).is_err());
    }

    #[tokio::test]
    async fn health_check_returns_ok() {
        let response = health_check().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_check_returns_ok_when_db_is_up() {
        let user = match std::env::var("POSTGRES_USER") {
            Ok(v) => v,
            Err(_) => return,
        };
        let password = match std::env::var("POSTGRES_PASSWORD") {
            Ok(v) => v,
            Err(_) => return,
        };
        let host = match std::env::var("POSTGRES_HOST") {
            Ok(v) => v,
            Err(_) => return,
        };
        let port = match std::env::var("POSTGRES_PORT") {
            Ok(v) => v,
            Err(_) => return,
        };
        let db = match std::env::var("POSTGRES_DB") {
            Ok(v) => v,
            Err(_) => return,
        };

        let database_url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            user, password, host, port, db
        );

        let pool = match sqlx::postgres::PgPoolOptions::new()
            .connect(&database_url)
            .await
        {
            Ok(p) => p,
            Err(_) => return,
        };

        let response = ready_check(Extension(pool)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
