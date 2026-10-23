//! Cliente HTTP del servicio de Atalaya, con los tipos de `atalaya-api`.
//!
//! Es bloqueante: cada llamada va en un hilo de fondo con
//! `cx.background_spawn`, nunca en el hilo de la interfaz.
//!
//! Este archivo tiene lo común. Las rutas van en un archivo por tema, para
//! que cada paso añada las suyas sin tocar las de otro:
//! - `entries.rs`: lista, menú lateral y artículos (pasos 5.1 y 5.2).
//! - `feeds.rs`: feeds y carpetas (paso 5.4).
//! - `papers.rs`: búsqueda, apertura y guardado de papers (paso 5.3).

mod entries;
mod feeds;
mod papers;

use std::time::Duration;

use atalaya_api::{ApiError, Status};
use reqwest::Method;
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::settings::Settings;

/// Error listo para mostrar en la interfaz, en español.
pub type ClientError = String;
pub type Result<T> = std::result::Result<T, ClientError>;

pub struct ApiClient {
    http: Client,
    base_url: String,
    api_key: String,
}

impl ApiClient {
    pub fn new(settings: &Settings) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| format!("No se pudo preparar el cliente HTTP: {e}"))?;
        Ok(Self {
            http,
            base_url: settings.service_url.trim_end_matches('/').to_owned(),
            api_key: settings.api_key.clone(),
        })
    }

    /// `GET /api/status`: el botón «Probar» de los ajustes.
    pub fn status(&self) -> Result<Status> {
        self.get("/api/status")
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.api_key)
    }

    pub(crate) fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        read_json(self.request(Method::GET, path).send())
    }

    pub(crate) fn get_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        read_json(self.request(Method::GET, path).query(query).send())
    }

    /// `POST` o `PATCH` con cuerpo JSON y respuesta JSON.
    #[allow(dead_code)] // Lo usan los pasos 5.2 y 5.4.
    pub(crate) fn send_json<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<T> {
        read_json(self.request(method, path).json(body).send())
    }

    /// `DELETE` u otra petición sin cuerpo en la respuesta (204).
    pub(crate) fn send_empty(&self, method: Method, path: &str) -> Result<()> {
        check(self.request(method, path).send()).map(|_| ())
    }
}

fn read_json<T: DeserializeOwned>(sent: reqwest::Result<Response>) -> Result<T> {
    check(sent)?
        .json()
        .map_err(|e| format!("El servicio respondió algo inesperado: {e}"))
}

/// Convierte un fallo de red o un código de error en un mensaje.
fn check(sent: reqwest::Result<Response>) -> Result<Response> {
    let response = sent.map_err(|e| {
        if e.is_timeout() {
            "El servicio tarda demasiado en responder".to_owned()
        } else {
            format!("No se pudo conectar con el servicio: {e}")
        }
    })?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    // El servicio explica el error en un ApiError; si no, se da el código.
    match response.json::<ApiError>() {
        Ok(body) => Err(body.error),
        Err(_) if status == reqwest::StatusCode::UNAUTHORIZED => {
            Err("El servicio rechaza la clave de los ajustes".into())
        }
        Err(_) => Err(format!("El servicio respondió con el error {status}")),
    }
}
