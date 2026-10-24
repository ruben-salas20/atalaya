//! Cliente HTTP con el que GPUI baja las imágenes de los artículos (#18).
//!
//! Sin él, GPUI usa uno vacío y cada imagen falla con «No HttpClient
//! available». Usa el mismo `reqwest` bloqueante que el cliente del
//! servicio: cada descarga va en un hilo propio y el resultado vuelve por un
//! canal, para no frenar la interfaz.

use std::time::Duration;

use futures::channel::oneshot;
use futures::future::BoxFuture;
use gpui::http_client::http::HeaderValue;
use gpui::http_client::{self, AsyncBody, HttpClient, Request, Response, Url};

pub struct ImageClient {
    http: reqwest::blocking::Client,
}

impl ImageClient {
    pub fn new() -> Option<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            // Algunos servidores de imágenes (Wikimedia) rechazan las
            // peticiones sin User-Agent.
            .user_agent(concat!("Atalaya/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()?;
        Some(Self { http })
    }
}

impl HttpClient for ImageClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    /// Solo se usa para bajar imágenes, así que el cuerpo de la petición
    /// no se manda.
    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, http_client::Result<Response<AsyncBody>>> {
        let http = self.http.clone();
        let (parts, _) = request.into_parts();
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let result = (|| -> http_client::Result<Response<AsyncBody>> {
                let response = http
                    .request(parts.method, parts.uri.to_string())
                    .headers(parts.headers)
                    .send()?;
                let mut builder = Response::builder().status(response.status());
                for (name, value) in response.headers() {
                    builder = builder.header(name, value);
                }
                Ok(builder.body(AsyncBody::from(response.bytes()?.to_vec()))?)
            })();
            tx.send(result).ok();
        });
        Box::pin(async move { rx.await? })
    }
}
