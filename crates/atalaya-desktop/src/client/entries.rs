//! Rutas de lectura: menú lateral, lista, abrir un artículo y marcarlo.

//!
//! El menú, las listas y los artículos se guardan en disco al llegar y, sin
//! red, salen de ahí (L8).

use std::sync::PoisonError;
use std::sync::atomic::Ordering;

use atalaya_api::{
    Entry, EntryPage, EntryQuery, EntrySummary, EntryUpdate, Id, Sidebar, TranslationState, View,
};
use reqwest::Method;

use super::{ApiClient, Result};

fn entry_key(id: Id) -> String {
    format!("entries/{id}")
}

impl ApiClient {
    /// `GET /api/sidebar`.
    pub fn sidebar(&self) -> Result<Sidebar> {
        self.get_cached("sidebar", "/api/sidebar", &(), |_| true)
    }

    /// `GET /api/entries`.
    pub fn entries(&self, query: &EntryQuery) -> Result<EntryPage> {
        let key = format!(
            "list-{:?}-{}-{}-{}-{}",
            query.view,
            query.feed_id.unwrap_or(0),
            query.category_id.unwrap_or(0),
            query.offset.unwrap_or(0),
            query.limit.unwrap_or(0),
        )
        .to_lowercase();
        self.get_cached(&key, "/api/entries", query, |_| true)
    }

    /// `GET /api/entries/{id}`. Pone en marcha la traducción si hace falta.
    pub fn entry(&self, id: Id) -> Result<Entry> {
        // A medio traducir no se guarda: sin red se vería en inglés.
        self.get_cached(
            &entry_key(id),
            &format!("/api/entries/{id}"),
            &(),
            |e: &Entry| e.translation != TranslationState::Pending,
        )
    }

    /// Baja a disco el texto de los guardados que aún no están, como hace
    /// el móvil, y borra los artículos viejos que ya no hacen falta. Pedir
    /// un guardado sin traducir pone en marcha su traducción; se guarda la
    /// próxima vez que se llame, ya traducido.
    pub fn cache_starred(&self) {
        let Some(cache) = &self.cache else {
            return;
        };
        if self.caching_starred.swap(true, Ordering::Relaxed) {
            return;
        }
        // La misma consulta que la lista de Guardados: así su primera página
        // también queda en disco aunque no se haya abierto en el PC.
        let query = EntryQuery {
            view: View::Starred,
            offset: Some(0),
            ..Default::default()
        };
        if let Some(page) = self.entries(&query).ok().filter(|_| !self.is_offline()) {
            let keys: Vec<String> = page.entries.iter().map(|e| entry_key(e.id)).collect();
            for (entry, key) in page.entries.iter().zip(&keys) {
                if !cache.contains(key) && self.entry(entry.id).is_err() {
                    break;
                }
            }
            cache.prune_entries(&keys);
        }
        self.caching_starred.store(false, Ordering::Relaxed);
    }

    /// Marca leído o guardado con `PATCH /api/entries/{id}`. Sin red, el
    /// cambio se guarda en la cola y se responde con `current` ya cambiado;
    /// se manda en la próxima lectura con red. Un error del servicio sí
    /// devuelve `Err`.
    pub fn update_entry(
        &self,
        current: &EntrySummary,
        update: &EntryUpdate,
    ) -> Result<EntrySummary> {
        let id = current.id;
        // Mientras se envía, nadie más manda la cola: así no se cruzan.
        let _turn = self.sending.lock().unwrap_or_else(PoisonError::into_inner);
        let sent = match &self.pending {
            Some(pending) => pending.merge(id, update),
            None => update.clone(),
        };
        let result = self.try_send_json(Method::PATCH, &format!("/api/entries/{id}"), &sent);
        match result {
            Ok(summary) => {
                self.offline.store(false, Ordering::Relaxed);
                self.forget(id, &sent);
                Ok(summary)
            }
            Err(failure) if failure.network && self.pending.is_some() => {
                self.offline.store(true, Ordering::Relaxed);
                Ok(applied(current, update))
            }
            Err(failure) => {
                self.forget(id, &sent);
                Err(failure.into())
            }
        }
    }

    fn forget(&self, id: Id, sent: &EntryUpdate) {
        if let Some(pending) = &self.pending {
            pending.forget(id, sent);
        }
    }
}

/// `summary` con los cambios de `update`.
fn applied(summary: &EntrySummary, update: &EntryUpdate) -> EntrySummary {
    let mut next = summary.clone();
    if let Some(read) = update.read {
        next.read = read;
    }
    if let Some(starred) = update.starred {
        next.starred = starred;
    }
    next
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use atalaya_api::EntryUpdate;

    use super::*;
    use crate::client::pending::Pending;
    use crate::client::pending::tests::TempDir;

    fn summary(id: Id) -> EntrySummary {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "feed_id": 1,
            "feed_title": "Feed",
            "title": "Título",
            "original_title": "Title",
            "title_translated": false,
            "lang": "en",
            "url": "https://example.com/a",
            "published_at": "2026-01-01T00:00:00Z",
            "read": false,
            "starred": false,
        }))
        .expect("resumen de prueba mal formado")
    }

    /// Cliente cuyo servicio no escucha: cada petición falla por red.
    fn offline_client(dir: &TempDir) -> ApiClient {
        // Se reserva un puerto y se suelta: nadie escucha en él.
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("no se pudo reservar un puerto")
            .port();
        client_at(dir, port)
    }

    fn client_at(dir: &TempDir, port: u16) -> ApiClient {
        let cache = dir.cache();
        ApiClient {
            http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("cliente HTTP de prueba"),
            base_url: format!("http://127.0.0.1:{port}"),
            api_key: String::new(),
            pending: Some(Pending::new(cache.clone())),
            sending: Mutex::new(()),
            cache: Some(cache),
            offline: AtomicBool::new(false),
            caching_starred: AtomicBool::new(false),
        }
    }

    #[test]
    fn offline_change_is_queued_and_applied() {
        let dir = TempDir::new();
        let client = offline_client(&dir);
        let update = EntryUpdate {
            read: Some(true),
            starred: None,
        };

        let result = client
            .update_entry(&summary(7), &update)
            .expect("sin red debe responder Ok");

        assert!(result.read);
        assert!(client.is_offline());
        assert_eq!(Pending::new(dir.cache()).all(), vec![(7, update)]);
    }

    #[test]
    fn offline_send_keeps_the_queue() {
        let dir = TempDir::new();
        let client = offline_client(&dir);
        let update = EntryUpdate {
            read: None,
            starred: Some(true),
        };
        Pending::new(dir.cache()).merge(4, &update);

        client.send_pending();

        assert_eq!(Pending::new(dir.cache()).all(), vec![(4, update)]);
    }

    /// Lee una petición HTTP completa y devuelve el texto entero.
    fn read_request(stream: &mut TcpStream) -> String {
        let mut data = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).expect("lectura de prueba");
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&data).into_owned();
            if let Some(head_end) = text.find("\r\n\r\n") {
                let length: usize = text[..head_end]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .map(str::to_owned)
                    })
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if data.len() >= head_end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    #[test]
    fn online_send_empties_the_queue() {
        let dir = TempDir::new();
        let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir el servidor");
        let port = listener.local_addr().expect("puerto del servidor").port();
        // Hace de servicio: responde una sola petición y devuelve lo que recibió.
        let service = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("conexión de prueba");
            let request = read_request(&mut stream);
            let body = serde_json::to_string(&summary(7)).expect("cuerpo de prueba");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("respuesta de prueba");
            request
        });
        let client = client_at(&dir, port);
        Pending::new(dir.cache()).merge(
            7,
            &EntryUpdate {
                read: Some(true),
                starred: None,
            },
        );

        client.send_pending();

        let request = service.join().expect("el servicio de prueba falló");
        assert!(request.starts_with("PATCH /api/entries/7 "), "{request}");
        assert!(request.contains(r#""read":true"#), "{request}");
        assert!(Pending::new(dir.cache()).all().is_empty());
    }
}
