use axum::{
    extract::Path,
    http::{self, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Extension, Json, Router,
};
use axum_prometheus::PrometheusMetricLayer;
use serde::Deserialize;
use std::sync::Arc;
use tower_http::trace::{self, TraceLayer};
use tracing::Level;

use crate::{
    config::CONFIG,
    db::get_prisma_client,
    prisma::{service, PrismaClient},
};

pub type Database = Extension<Arc<PrismaClient>>;

const BOTS_COUNT_LIMIT: i64 = 5;

async fn get_services(db: Database) -> impl IntoResponse {
    let services = db
        .service()
        .find_many(vec![])
        .order_by(service::id::order(prisma_client_rust::Direction::Asc))
        .exec()
        .await
        .unwrap();

    Json(services).into_response()
}

async fn get_service(Path(id): Path<i32>, db: Database) -> impl IntoResponse {
    let service = db
        .service()
        .find_unique(service::id::equals(id))
        .exec()
        .await
        .unwrap();

    match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn delete_service(Path(id): Path<i32>, db: Database) -> impl IntoResponse {
    let service = db
        .service()
        .find_unique(service::id::equals(id))
        .exec()
        .await
        .unwrap();

    match service {
        Some(v) => {
            let _ = db.service().delete(service::id::equals(id)).exec().await;

            Json(v).into_response()
        }
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

    let exist_count = db
        .service()
        .count(vec![service::user::equals(user)])
        .exec()
        .await
        .unwrap();

    if exist_count >= BOTS_COUNT_LIMIT {
        return StatusCode::PAYMENT_REQUIRED.into_response();
    };

    let service = db
        .service()
        .create(
            token,
            user,
            status,
            chrono::offset::Local::now().into(),
            cache,
            username,
            vec![],
        )
        .exec()
        .await
        .unwrap();

    Json(service).into_response()
}

async fn update_state(
    Path(id): Path<i32>,
    db: Database,
    Json(state): Json<String>,
) -> impl IntoResponse {
    let service = db
        .service()
        .update(service::id::equals(id), vec![service::status::set(state)])
        .exec()
        .await;

    match service {
        Ok(v) => Json(v).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn update_cache(
    Path(id): Path<i32>,
    db: Database,
    Json(cache): Json<String>,
) -> impl IntoResponse {
    let service = db
        .service()
        .update(service::id::equals(id), vec![service::cache::set(cache)])
        .exec()
        .await;

    match service {
        Ok(v) => Json(v).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
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
    let client = Arc::new(get_prisma_client().await);

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let app_router = Router::new()
        .route("/", get(get_services))
        .route("/:id/", get(get_service))
        .route("/:id/", delete(delete_service))
        .route("/", post(create_service))
        .route("/:id/update_status", patch(update_state))
        .route("/:id/update_cache", patch(update_cache))
        .layer(middleware::from_fn(auth))
        .layer(Extension(client))
        .layer(prometheus_layer);

    let metric_router =
        Router::new().route("/metrics", get(|| async move { metric_handle.render() }));

    Router::new()
        .nest("/", app_router)
        .nest("/", metric_router)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        )
}
