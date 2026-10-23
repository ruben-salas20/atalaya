use atalaya_api::ApiError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Error de un handler: un código HTTP y un mensaje en español.
#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub message: String,
}

impl AppError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// Fallo de Miniflux o de llama-swap.
    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(status = %self.status, "{}", self.message);
        }
        (
            self.status,
            Json(ApiError {
                error: self.message,
            }),
        )
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
