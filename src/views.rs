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
use tower_http::trace::{self, TraceLayer};
use tracing::Level;

use crate::{config::CONFIG, db::get_pg_pool};

pub type Database = Extension<PgPool>;

const BOTS_COUNT_LIMIT: i64 = 5;

#[derive(sqlx::FromRow, Serialize)]
pub struct Service {
    pub id: i32,
    pub token: String,
    pub user: i64,
    pub status: String,
    pub created_time: DateTime<chrono::Local>,
    pub cache: String,
    pub username: String,
}

async fn get_services(db: Database) -> impl IntoResponse {
    let services = sqlx::query_as!(
        Service,
        r#"
        SELECT * FROM services
        "#
    )
    .fetch_all(&db.0)
    .await
    .unwrap();

    Json(services).into_response()
}

async fn get_service(Path(id): Path<i32>, db: Database) -> impl IntoResponse {
    let service = sqlx::query_as!(
        Service,
        r#"
        SELECT * FROM services WHERE id = $1
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
        Service,
        r#"
        DELETE FROM services WHERE id = $1 RETURNING *
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

    let service = sqlx::query_as!(
        Service,
        r#"
        INSERT INTO services (token, "user", status, cache, username) VALUES ($1, $2, $3, $4, $5) RETURNING *
        "#,
        token,
        user,
        status,
        cache,
        username
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
        Service,
        r#"
        UPDATE services SET status = $1 WHERE id = $2 RETURNING *
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
        Service,
        r#"
        UPDATE services SET cache = $1 WHERE id = $2 RETURNING *
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

    if auth_header != CONFIG.api_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

pub async fn get_router() -> Router {
    let client = get_pg_pool().await;

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

    let metric_router =
        Router::new().route("/metrics", get(|| async move { metric_handle.render() }));

    Router::new().merge(app_router).merge(metric_router).layer(
        TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
            .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
    )
}
