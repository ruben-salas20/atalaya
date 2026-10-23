use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

use crate::{AppState, error::AppError};

/// Exige `Authorization: Bearer <ATALAYA_API_KEY>`.
pub async fn require_api_key(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let given = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    // Comparación en tiempo constante, para no dar pistas de la clave.
    if bool::from(given.as_bytes().ct_eq(state.config.api_key.as_bytes())) {
        Ok(next.run(req).await)
    } else {
        Err(AppError::new(
            StatusCode::UNAUTHORIZED,
            "falta la clave o no es correcta",
        ))
    }
}
