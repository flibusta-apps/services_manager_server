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

pub type Database = Extension<PgPool>;

const BOTS_COUNT_LIMIT: i64 = 5;

fn mask_token(token: &str) -> String {
    format!("{}…", &token[..token.len().min(8)])
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

async fn get_services(db: Database) -> impl IntoResponse {
    let rows = sqlx::query_as!(
        ServiceRow,
        r#"
        SELECT id, token_ciphertext AS "token_ciphertext!", token_nonce AS "token_nonce!", "user", status, created_time, cache, username FROM services
        "#
    )
    .fetch_all(&db.0)
    .await
    .unwrap();

    let services: Vec<Service> = rows
        .into_iter()
        .map(|row| row.try_into_service().expect("Failed to decrypt token"))
        .collect();

    Json(services).into_response()
}

async fn get_service(Path(id): Path<i32>, db: Database) -> impl IntoResponse {
    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        SELECT id, "user", status, created_time, cache, username FROM services WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&db.0)
    .await
    .unwrap();

    match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn delete_service(Path(id): Path<i32>, db: Database) -> impl IntoResponse {
    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        DELETE FROM services WHERE id = $1 RETURNING id, "user", status, created_time, cache, username
        "#,
        id
    )
    .fetch_optional(&db.0)
    .await
    .unwrap();

    match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
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

async fn create_service(db: Database, Json(data): Json<CreateServiceData>) -> impl IntoResponse {
    let CreateServiceData {
        token,
        user,
        status,
        cache,
        username,
    } = data;

    let exist_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM services WHERE "user" = $1
        "#,
        user
    )
    .fetch_one(&db.0)
    .await
    .unwrap_or(Some(0))
    .unwrap();

    if exist_count >= BOTS_COUNT_LIMIT {
        return StatusCode::PAYMENT_REQUIRED.into_response();
    };

    let token_hmac = crate::crypto::hmac_token(&token);

    let token_exists = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(SELECT 1 FROM services WHERE token_hmac = $1)
        "#,
        token_hmac
    )
    .fetch_one(&db.0)
    .await
    .unwrap_or(Some(false))
    .unwrap();

    if token_exists {
        return StatusCode::CONFLICT.into_response();
    }

    let encrypted = crate::crypto::encrypt_token(&token);

    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        INSERT INTO services (token_ciphertext, token_nonce, token_hmac, "user", status, cache, username, created_time)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, "user", status, created_time, cache, username
        "#,
        encrypted.ciphertext,
        encrypted.nonce,
        token_hmac,
        user,
        status,
        cache,
        username,
        chrono::Local::now()
    )
        .fetch_one(&db.0)
        .await
        .unwrap();

    Json(service).into_response()
}

async fn update_state(
    Path(id): Path<i32>,
    db: Database,
    Json(state): Json<String>,
) -> impl IntoResponse {
    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        UPDATE services SET status = $1 WHERE id = $2 RETURNING id, "user", status, created_time, cache, username
        "#,
        state,
        id
    )
    .fetch_optional(&db.0)
    .await
    .unwrap();

    match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn update_cache(
    Path(id): Path<i32>,
    db: Database,
    Json(cache): Json<String>,
) -> impl IntoResponse {
    let service = sqlx::query_as!(
        ServiceInfo,
        r#"
        UPDATE services SET cache = $1 WHERE id = $2 RETURNING id, "user", status, created_time, cache, username
        "#,
        cache,
        id
    )
    .fetch_optional(&db.0)
    .await
    .unwrap();

    match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
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

pub async fn get_router(client: PgPool) -> Router {
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let app_router = Router::new()
        .route("/", get(get_services))
        .route("/{id}/", get(get_service))
        .route("/{id}/", delete(delete_service))
        .route("/", post(create_service))
        .route("/{id}/update_status", patch(update_state))
        .route("/{id}/update_cache", patch(update_cache))
        .layer(middleware::from_fn(auth))
        .layer(Extension(client))
        .layer(prometheus_layer);

    let health_router = Router::new().route("/health", get(health_check));

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
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        )
}
