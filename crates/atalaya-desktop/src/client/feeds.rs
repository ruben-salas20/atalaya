//! Rutas de feeds y carpetas (ADR 0007, F1–F6). El servicio las reenvía a
//! Miniflux.

use atalaya_api::{
    Category, CategoryInput, DiscoverRequest, DiscoveredFeed, Feed, FeedUpdate, Id, NewFeed,
};
use std::time::Duration;

use reqwest::Method;

use super::{ApiClient, Result};

/// Cuánto se espera a que el servicio cree un feed (#48). Las demás peticiones
/// cortan a los 20 s.
pub const ADD_FEED_TIMEOUT: Duration = Duration::from_secs(120);

impl ApiClient {
    /// `POST /api/feeds/discover`: busca los feeds de una web.
    pub fn discover(&self, url: &str) -> Result<Vec<DiscoveredFeed>> {
        let body = DiscoverRequest {
            url: url.to_owned(),
        };
        self.send_json(Method::POST, "/api/feeds/discover", &body)
    }

    /// `GET /api/feeds`, ordenados por carpeta y nombre.
    pub fn feeds(&self) -> Result<Vec<Feed>> {
        self.get("/api/feeds")
    }

    /// `POST /api/feeds`. Con «artículo completo», Miniflux descarga cada artículo
    /// antes de responder y puede tardar minutos (#48). Se espera hasta
    /// [`ADD_FEED_TIMEOUT`]; si no llega la respuesta, `Ok(None)`: el servicio
    /// termina de crear el feed igualmente (#34).
    pub fn add_feed(&self, feed: &NewFeed) -> Result<Option<Feed>> {
        self.add_feed_within(feed, ADD_FEED_TIMEOUT)
    }

    fn add_feed_within(&self, feed: &NewFeed, timeout: Duration) -> Result<Option<Feed>> {
        let sent = self
            .request(Method::POST, "/api/feeds")
            .timeout(timeout)
            .json(feed)
            .send();
        match sent {
            Err(e) if e.is_timeout() => Ok(None),
            sent => Ok(Some(super::read_json(sent)?)),
        }
    }

    /// `PATCH /api/feeds/{id}`.
    pub fn update_feed(&self, id: Id, update: &FeedUpdate) -> Result<Feed> {
        self.send_json(Method::PATCH, &format!("/api/feeds/{id}"), update)
    }

    /// `DELETE /api/feeds/{id}`.
    pub fn delete_feed(&self, id: Id) -> Result<()> {
        self.send_empty(Method::DELETE, &format!("/api/feeds/{id}"))
    }

    /// `GET /api/categories`, en orden alfabético.
    pub fn categories(&self) -> Result<Vec<Category>> {
        self.get("/api/categories")
    }

    /// `POST /api/categories`: crea una carpeta.
    pub fn create_category(&self, title: &str) -> Result<Category> {
        let body = CategoryInput {
            title: title.to_owned(),
        };
        self.send_json(Method::POST, "/api/categories", &body)
    }

    /// `PATCH /api/categories/{id}`: renombra una carpeta.
    pub fn rename_category(&self, id: Id, title: &str) -> Result<Category> {
        let body = CategoryInput {
            title: title.to_owned(),
        };
        self.send_json(Method::PATCH, &format!("/api/categories/{id}"), &body)
    }

    /// `DELETE /api/categories/{id}`. El servicio responde 409 si la carpeta
    /// aún tiene feeds.
    pub fn delete_category(&self, id: Id) -> Result<()> {
        self.send_empty(Method::DELETE, &format!("/api/categories/{id}"))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;

    use super::*;

    #[test]
    fn slow_add_feed_is_not_an_error() {
        // #48: el servicio tarda más que la espera; la app no debe dar error.
        let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir el servidor");
        let port = listener.local_addr().expect("puerto del servidor").port();
        let service = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("conexión de prueba");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            std::thread::sleep(Duration::from_secs(3));
        });
        let client = ApiClient {
            http: reqwest::blocking::Client::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            api_key: String::new(),
            pending: None,
            sending: Mutex::new(()),
            cache: None,
            offline: AtomicBool::new(false),
            caching_starred: AtomicBool::new(false),
        };
        let feed = NewFeed {
            feed_url: "https://example.com/feed".into(),
            category_id: 1,
            crawler: true,
        };

        let added = client.add_feed_within(&feed, Duration::from_secs(1));

        assert_eq!(added, Ok(None));
        service.join().expect("el servicio de prueba falló");
    }
}
