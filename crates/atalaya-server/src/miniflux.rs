//! Cliente de la API de Miniflux.
//!
//! Aquí solo está lo común. Cada módulo añade los métodos que necesita en su
//! propio bloque `impl Miniflux`, para que los pasos en paralelo no toquen
//! este archivo.

use reqwest::{Method, RequestBuilder};
use serde::{Serialize, de::DeserializeOwned};

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct Miniflux {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl Miniflux {
    pub fn new(http: reqwest::Client, base_url: &str, api_key: &str) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key: api_key.to_owned(),
        }
    }

    /// Petición a `path`, que empieza por `/v1/`, ya con la clave puesta.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base_url))
            .header("X-Auth-Token", &self.api_key)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, AppError> {
        Self::send(self.request(Method::GET, path)).await
    }

    pub async fn send_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<T, AppError> {
        Self::send(self.request(method, path).json(body)).await
    }

    /// Manda la petición y lee la respuesta como JSON. Si Miniflux responde
    /// con error, devuelve el mismo código con su mensaje.
    pub async fn send<T: DeserializeOwned>(req: RequestBuilder) -> Result<T, AppError> {
        let resp = Self::send_empty(req).await?;
        resp.json()
            .await
            .map_err(|e| AppError::bad_gateway(format!("respuesta de Miniflux no válida: {e}")))
    }

    /// Como [`Self::send`], para respuestas sin cuerpo (204).
    pub async fn send_empty(req: RequestBuilder) -> Result<reqwest::Response, AppError> {
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::bad_gateway(format!("no se pudo hablar con Miniflux: {e}")))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        #[derive(serde::Deserialize)]
        struct MinifluxError {
            error_message: String,
        }
        let message = match resp.json::<MinifluxError>().await {
            Ok(e) => format!("Miniflux: {}", e.error_message),
            Err(_) => format!("Miniflux respondió {status}"),
        };
        let status = axum::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
        Err(AppError::new(status, message))
    }
}
