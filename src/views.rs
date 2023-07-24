use axum::{Router, response::{Response, IntoResponse}, http::{StatusCode, self, Request}, middleware::{Next, self}, Extension, routing::{get, delete, post, patch}, Json, extract::{Path, self}};
use axum_prometheus::PrometheusMetricLayer;
use serde::Deserialize;
use tower_http::trace::{TraceLayer, self};
use tracing::Level;
use std::sync::Arc;

use crate::{config::CONFIG, db::get_prisma_client, prisma::{PrismaClient, service}};


pub type Database = Extension<Arc<PrismaClient>>;


//

async fn get_services(
    db: Database
) -> impl IntoResponse {
    let services = db.service()
        .find_many(vec![])
        .order_by(service::id::order(prisma_client_rust::Direction::Asc))
        .exec()
        .await
        .unwrap();

    Json(services).into_response()
}

async fn get_service(
    Path(id): Path<i32>,
    db: Database
) -> impl IntoResponse {
    let service = db.service()
        .find_unique(service::id::equals(id))
        .exec()
        .await
        .unwrap();

    match service {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn delete_service(
    Path(id): Path<i32>,
    db: Database
) -> impl IntoResponse {
    let service = db.service()
        .find_unique(service::id::equals(id))
        .exec()
        .await
        .unwrap();

    match service {
        Some(v) => {
            let _ = db.service()
                .delete(service::id::equals(id))
                .exec()
                .await;

            Json(v).into_response()
        },
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateServiceData {
    #[serde(rename = "token")]
    pub token: String,
    #[serde(rename = "user")]
    pub user: i64,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "cache")]
    pub cache: String,
    #[serde(rename = "username")]
    pub username: String,
}

async fn create_service(
    db: Database,
    extract::Json(data): extract::Json<CreateServiceData>,
) -> impl IntoResponse {
    let CreateServiceData { token, user, status, cache, username } = data;

    let service = db.service()
        .create(
            token,
            user,
            status,
            chrono::offset::Local::now().into(),
            cache,
            username,
            vec![]
        )
        .exec()
        .await
        .unwrap();

    Json(service).into_response()
}

async fn update_state(
    Path(id): Path<i32>,
    db: Database,
    extract::Json(state): extract::Json<String>
) -> impl IntoResponse {
    let service = db.service()
        .update(
            service::id::equals(id),
            vec![
                service::status::set(state)
            ]
        )
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
    extract::Json(cache): extract::Json<String>
) -> impl IntoResponse {
    let service = db.service()
        .update(
            service::id::equals(id),
            vec![
                service::cache::set(cache)
            ]
        )
        .exec()
        .await;

    match service {
        Ok(v) => Json(v).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

//


async fn auth<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let auth_header = req.headers()
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

    let metric_router = Router::new()
        .route("/metrics", get(|| async move { metric_handle.render() }));

    Router::new()
        .nest("/", app_router)
        .nest("/", metric_router)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new()
                    .level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new()
                    .level(Level::INFO)),
        )
}
