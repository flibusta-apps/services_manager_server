use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub enum AppError {
    Db(sqlx::Error),
    Decrypt(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            AppError::Db(err) => tracing::error!(error = ?err, "database request failed"),
            AppError::Decrypt(err) => tracing::error!(error = ?err, "token decryption failed"),
        }

        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Db(err)
    }
}

impl From<String> for AppError {
    fn from(err: String) -> Self {
        AppError::Decrypt(err)
    }
}
