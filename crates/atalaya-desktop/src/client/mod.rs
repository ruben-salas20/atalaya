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

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use atalaya_api::{ApiError, Status};
use reqwest::Method;
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::cache::Cache;
use crate::settings::Settings;

/// Error listo para mostrar en la interfaz, en español.
pub type ClientError = String;
pub type Result<T> = std::result::Result<T, ClientError>;

pub struct ApiClient {
    http: Client,
    base_url: String,
    api_key: String,
    /// `None` si no se pudo crear la carpeta: la app funciona igual, sin
    /// copia en disco.
    cache: Option<Cache>,
    /// La última petición no llegó al servicio y se respondió con la copia.
    offline: AtomicBool,
    /// Ya hay un hilo bajando los guardados.
    caching_starred: AtomicBool,
}

/// Por qué falló una petición: sin red se puede tirar de la copia en disco;
/// con un error del servicio, no.
struct Failure {
    network: bool,
    message: ClientError,
}

impl From<Failure> for ClientError {
    fn from(failure: Failure) -> Self {
        failure.message
    }
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
            cache: Cache::open(),
            offline: AtomicBool::new(false),
            caching_starred: AtomicBool::new(false),
        })
    }

    /// La última petición se respondió con la copia en disco.
    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::Relaxed)
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
        Ok(read_json(self.request(Method::GET, path).send())?)
    }

    pub(crate) fn get_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        Ok(read_json(
            self.request(Method::GET, path).query(query).send(),
        )?)
    }

    /// `GET` que guarda la respuesta en disco con el nombre `key` si
    /// `keep(&respuesta)`. Sin red, responde con lo guardado.
    pub(crate) fn get_cached<T, Q>(
        &self,
        key: &str,
        path: &str,
        query: &Q,
        keep: impl Fn(&T) -> bool,
    ) -> Result<T>
    where
        T: DeserializeOwned + Serialize,
        Q: Serialize,
    {
        match read_json(self.request(Method::GET, path).query(query).send()) {
            Ok(value) => {
                self.offline.store(false, Ordering::Relaxed);
                if let Some(cache) = self.cache.as_ref().filter(|_| keep(&value)) {
                    cache.store(key, &value);
                }
                Ok(value)
            }
            Err(failure) if failure.network => {
                match self.cache.as_ref().and_then(|c| c.load(key)) {
                    Some(value) => {
                        self.offline.store(true, Ordering::Relaxed);
                        Ok(value)
                    }
                    None => Err(failure.message),
                }
            }
            Err(failure) => Err(failure.message),
        }
    }

    /// `POST` o `PATCH` con cuerpo JSON y respuesta JSON.
    #[allow(dead_code)] // Lo usan los pasos 5.2 y 5.4.
    pub(crate) fn send_json<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<T> {
        Ok(read_json(self.request(method, path).json(body).send())?)
    }

    /// `DELETE` u otra petición sin cuerpo en la respuesta (204).
    pub(crate) fn send_empty(&self, method: Method, path: &str) -> Result<()> {
        check(self.request(method, path).send())?;
        Ok(())
    }
}

fn read_json<T: DeserializeOwned>(
    sent: reqwest::Result<Response>,
) -> std::result::Result<T, Failure> {
    check(sent)?.json().map_err(|e| Failure {
        network: false,
        message: format!("El servicio respondió algo inesperado: {e}"),
    })
}

/// Convierte un fallo de red o un código de error en un mensaje.
fn check(sent: reqwest::Result<Response>) -> std::result::Result<Response, Failure> {
    let response = sent.map_err(|e| Failure {
        network: true,
        message: if e.is_timeout() {
            "El servicio tarda demasiado en responder".to_owned()
        } else {
            format!("No se pudo conectar con el servicio: {e}")
        },
    })?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    // El servicio explica el error en un ApiError; si no, se da el código.
    let message = match response.json::<ApiError>() {
        Ok(body) => body.error,
        Err(_) if status == reqwest::StatusCode::UNAUTHORIZED => {
            "El servicio rechaza la clave de los ajustes".into()
        }
        Err(_) => format!("El servicio respondió con el error {status}"),
    };
    Err(Failure {
        network: false,
        message,
    })
}
