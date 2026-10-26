use atalaya_api::ApiError;
use axum::{
    Json,
    extract::{
        FromRequest, FromRequestParts, Path, Request,
        rejection::{JsonRejection, PathRejection},
    },
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

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

/// Como `Json<T>`, pero si el cuerpo no vale responde con un [`AppError`] en
/// español en vez del texto en inglés de axum (#45).
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(AppError::from)
    }
}

/// Como `Path<T>`, con el error en español (#45).
pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(AppError::from)
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        let message = match &rejection {
            JsonRejection::MissingJsonContentType(_) => {
                "Falta la cabecera Content-Type: application/json".to_owned()
            }
            JsonRejection::JsonSyntaxError(_) => {
                format!(
                    "El cuerpo no es JSON válido: {}",
                    detail_es(&rejection.body_text())
                )
            }
            JsonRejection::JsonDataError(_) => {
                format!(
                    "El cuerpo no tiene la forma esperada: {}",
                    detail_es(&rejection.body_text())
                )
            }
            _ => "No se pudo leer el cuerpo de la petición".to_owned(),
        };
        Self::new(rejection.status(), message)
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        Self::new(
            rejection.status(),
            format!(
                "La ruta no es válida: {}",
                detail_es(&rejection.body_text())
            ),
        )
    }
}

/// Respuesta para una ruta que no existe (#45).
pub async fn not_found() -> AppError {
    AppError::not_found("Esa ruta no existe")
}

/// Respuesta para un método que la ruta no admite (#45).
pub async fn method_not_allowed() -> AppError {
    AppError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "La ruta no admite ese método",
    )
}

/// Pasa al español los mensajes más comunes de axum y serde. Lo que no
/// reconoce queda como está: mejor un detalle en inglés que ninguno.
pub fn detail_es(text: &str) -> String {
    const PREFIXES: [&str; 6] = [
        "Failed to deserialize the JSON body into the target type: ",
        "Failed to parse the request body as JSON: ",
        "Failed to deserialize query string: ",
        "Failed to deserialize form: ",
        "Invalid URL: ",
        "Wrong number of path arguments for `Path`. ",
    ];
    let mut s = text.trim();
    for prefix in PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    let mut out = s.to_owned();
    // « at line 1 column 7» → « (línea 1, columna 7)».
    if let Some(i) = out.rfind(" at line ") {
        let place = out[i + " at line ".len()..].to_owned();
        if let Some((line, column)) = place.split_once(" column ") {
            out = format!("{} (línea {line}, columna {column})", &out[..i]);
        }
    }
    let replacements: [(&str, &str); 10] = [
        ("missing field ", "falta el campo "),
        ("unknown field ", "campo desconocido "),
        ("unknown variant ", "valor no permitido "),
        ("invalid type: ", "tipo no válido: "),
        ("invalid value: ", "valor no válido: "),
        (", expected one of ", "; se esperaba uno de "),
        (", expected ", "; se esperaba "),
        ("EOF while parsing", "el JSON se corta antes de terminar"),
        ("trailing characters", "sobran caracteres al final"),
        ("expected value", "se esperaba un valor"),
    ];
    for (en, es) in replacements {
        out = out.replace(en, es);
    }
    // «Cannot parse `abc` to a `i64`» → «no se puede leer `abc` como `i64`».
    if let Some(rest) = out.strip_prefix("Cannot parse ") {
        out = format!("no se puede leer {}", rest.replacen(" to a ", " como ", 1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_messages_in_spanish() {
        assert_eq!(
            detail_es(
                "Failed to deserialize the JSON body into the target type: missing field `title` at line 1 column 2"
            ),
            "falta el campo `title` (línea 1, columna 2)"
        );
        assert_eq!(
            detail_es(
                "Failed to parse the request body as JSON: EOF while parsing an object at line 1 column 9"
            ),
            "el JSON se corta antes de terminar an object (línea 1, columna 9)"
        );
        assert_eq!(
            detail_es("invalid type: string \"x\", expected i64"),
            "tipo no válido: string \"x\"; se esperaba i64"
        );
        assert_eq!(
            detail_es("Invalid URL: Cannot parse `abc` to a `i64`"),
            "no se puede leer `abc` como `i64`"
        );
        assert_eq!(detail_es("algo raro"), "algo raro");
    }
}
