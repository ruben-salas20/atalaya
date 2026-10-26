//! Las traducciones guardadas en el servicio (#91): volver a traducir un
//! artículo o un paper, y borrar las de lo ya leído. Nunca tocan Miniflux ni
//! Guardados: solo la caché de traducciones de Atalaya, en el servicio y en
//! las copias de este PC.

use atalaya_api::{Cleanup, Entry, Id};
use reqwest::Method;

use super::entries::entry_key;
use super::papers::path_id;
use super::{ApiClient, ClientError, Result};

/// Lo que pasó al volver a traducir: qué se borró y el aviso si algo falló.
/// Si falla la traducción del artículo, no se pide la del paper.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Forgotten {
    /// Se borró la traducción del artículo, y su copia en disco.
    pub entry: bool,
    /// Se borró la traducción del paper.
    pub paper: bool,
    /// Lo que dijo el servicio, en español (un 409 si se está traduciendo).
    pub error: Option<ClientError>,
}

impl ApiClient {
    /// `DELETE /api/entries/{id}/translation`: el artículo se vuelve a traducir
    /// la próxima vez que se abra. 409 si se está traduciendo.
    pub fn delete_entry_translation(&self, id: Id) -> Result<()> {
        self.send_empty(Method::DELETE, &format!("/api/entries/{id}/translation"))
    }

    /// `DELETE /api/papers/{arxiv_id}/translation`, con el texto completo y
    /// sus fórmulas. 409 si se está traduciendo.
    pub fn delete_paper_translation(&self, arxiv_id: &str) -> Result<()> {
        let path = format!("/api/papers/{}/translation", path_id(arxiv_id));
        self.send_empty(Method::DELETE, &path)
    }

    /// Vuelve a traducir: borra la traducción de un artículo (`entry`) y la de
    /// un paper (`paper`), y la copia en disco del artículo. Lo borrado queda
    /// en la respuesta aunque algo más falle.
    pub fn forget_translations(&self, entry: Option<Id>, paper: Option<&str>) -> Forgotten {
        let mut forgotten = Forgotten::default();
        if let Some(id) = entry {
            if let Err(error) = self.delete_entry_translation(id) {
                forgotten.error = Some(error);
                return forgotten;
            }
            forgotten.entry = true;
            if let Some(cache) = &self.cache {
                cache.remove(&entry_key(id));
            }
        }
        if let Some(arxiv_id) = paper {
            match self.delete_paper_translation(arxiv_id) {
                Ok(()) => forgotten.paper = true,
                Err(error) => forgotten.error = Some(error),
            }
        }
        forgotten
    }

    /// `GET /api/translations/cleanup`: lo que borraría «Borrar traducciones
    /// de lo leído», sin borrar nada.
    pub fn translation_cleanup_preview(&self) -> Result<Cleanup> {
        self.get("/api/translations/cleanup")
    }

    /// `POST /api/translations/cleanup`: borra las traducciones de lo leído y
    /// responde lo borrado. Las copias en disco de esos artículos (leídos y
    /// no guardados) se quitan también, como en Android.
    pub fn cleanup_translations(&self) -> Result<Cleanup> {
        let done: Cleanup = super::read_json(
            self.request(Method::POST, "/api/translations/cleanup")
                .send(),
        )?;
        if let Some(cache) = &self.cache {
            for key in cache.entry_keys() {
                let read_and_unstarred = cache
                    .load::<Entry>(&key)
                    .is_some_and(|entry| entry.summary.read && !entry.summary.starred);
                if read_and_unstarred {
                    cache.remove(&key);
                }
            }
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use super::*;
    use crate::client::pending::Pending;
    use crate::client::pending::tests::TempDir;

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

    /// Hace de servicio: responde a cada petición con su `(estado, cuerpo)`, en
    /// orden, y devuelve la línea de cada petición que recibió.
    fn serve(replies: Vec<(&'static str, String)>) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir el servidor");
        let port = listener.local_addr().expect("puerto del servidor").port();
        let service = std::thread::spawn(move || {
            replies
                .into_iter()
                .map(|(status, body)| {
                    let (mut stream, _) = listener.accept().expect("conexión de prueba");
                    let line = request_line(&mut stream);
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("respuesta de prueba");
                    line
                })
                .collect()
        });
        (port, service)
    }

    /// Lee la cabecera de una petición (sin cuerpo) y da su primera línea.
    fn request_line(stream: &mut TcpStream) -> String {
        let mut data = Vec::new();
        let mut chunk = [0u8; 512];
        while !data.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut chunk).expect("lectura de prueba");
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
        }
        String::from_utf8_lossy(&data)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    /// Una copia de un artículo en disco, como la guarda `entry()`.
    fn copy(id: Id, read: bool, starred: bool) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "feed_id": 1,
            "feed_title": "Feed",
            "title": "Título",
            "original_title": "Title",
            "title_translated": false,
            "lang": "en",
            "url": "https://example.com/a",
            "published_at": "2026-01-07T00:00:00Z",
            "read": read,
            "starred": starred,
            "author": null,
            "content": "<p>One.</p>",
            "translation": "done",
            "translated_content": "<p>Uno.</p>",
        })
    }

    #[test]
    fn deleting_an_entry_translation_calls_its_route() {
        let dir = TempDir::new();
        let (port, service) = serve(vec![("204 No Content", String::new())]);

        client_at(&dir, port)
            .delete_entry_translation(7)
            .expect("el servicio borra la traducción");

        assert_eq!(
            service.join().expect("servicio de prueba"),
            ["DELETE /api/entries/7/translation HTTP/1.1"]
        );
    }

    #[test]
    fn an_old_paper_id_is_escaped_in_the_route() {
        let dir = TempDir::new();
        let (port, service) = serve(vec![("204 No Content", String::new())]);

        client_at(&dir, port)
            .delete_paper_translation("hep-th/9901001")
            .expect("el servicio borra la traducción");

        assert_eq!(
            service.join().expect("servicio de prueba"),
            ["DELETE /api/papers/hep-th%2F9901001/translation HTTP/1.1"]
        );
    }

    #[test]
    fn a_409_shows_the_services_message() {
        let dir = TempDir::new();
        let body = r#"{"error":"Se está traduciendo; espera a que acabe."}"#.to_owned();
        let (port, service) = serve(vec![("409 Conflict", body)]);

        let error = client_at(&dir, port)
            .delete_entry_translation(7)
            .expect_err("con 409 no se borra");

        service.join().expect("servicio de prueba");
        assert_eq!(error, "Se está traduciendo; espera a que acabe.");
    }

    #[test]
    fn a_refused_entry_deletion_keeps_the_copy_and_skips_the_paper() {
        let dir = TempDir::new();
        let cache = dir.cache();
        cache.store(&entry_key(7), &copy(7, true, false));
        let (port, service) = serve(vec![(
            "409 Conflict",
            r#"{"error":"Se está traduciendo."}"#.to_owned(),
        )]);

        let forgotten = client_at(&dir, port).forget_translations(Some(7), Some("2610.08812"));

        // Solo se pidió la del artículo: la del paper no se toca.
        assert_eq!(service.join().expect("servicio de prueba").len(), 1);
        assert_eq!(
            forgotten,
            Forgotten {
                entry: false,
                paper: false,
                error: Some("Se está traduciendo.".into()),
            }
        );
        assert!(cache.contains(&entry_key(7)));
    }

    #[test]
    fn forgetting_an_article_that_is_a_paper_drops_both_and_its_copy() {
        let dir = TempDir::new();
        let cache = dir.cache();
        cache.store(&entry_key(7), &copy(7, true, false));
        let (port, service) = serve(vec![
            ("204 No Content", String::new()),
            ("204 No Content", String::new()),
        ]);

        let forgotten = client_at(&dir, port).forget_translations(Some(7), Some("2610.08812"));

        assert_eq!(
            service.join().expect("servicio de prueba"),
            [
                "DELETE /api/entries/7/translation HTTP/1.1",
                "DELETE /api/papers/2610.08812/translation HTTP/1.1",
            ]
        );
        assert_eq!(
            forgotten,
            Forgotten {
                entry: true,
                paper: true,
                error: None,
            }
        );
        assert!(!cache.contains(&entry_key(7)));
    }

    #[test]
    fn the_preview_reads_the_counts_without_deleting() {
        let dir = TempDir::new();
        let body = r#"{"entries":2,"papers":1,"bytes":2500000}"#.to_owned();
        let (port, service) = serve(vec![("200 OK", body)]);

        let preview = client_at(&dir, port)
            .translation_cleanup_preview()
            .expect("el servicio cuenta");

        assert_eq!(
            service.join().expect("servicio de prueba"),
            ["GET /api/translations/cleanup HTTP/1.1"]
        );
        assert_eq!(
            preview,
            Cleanup {
                entries: 2,
                papers: 1,
                bytes: 2_500_000,
            }
        );
    }

    #[test]
    fn the_cleanup_drops_only_the_read_copies_that_are_not_starred() {
        let dir = TempDir::new();
        let cache = dir.cache();
        cache.store(&entry_key(1), &copy(1, true, false));
        cache.store(&entry_key(2), &copy(2, false, false));
        cache.store(&entry_key(3), &copy(3, true, true));
        let body = r#"{"entries":1,"papers":0,"bytes":10}"#.to_owned();
        let (port, service) = serve(vec![("200 OK", body)]);

        let done = client_at(&dir, port)
            .cleanup_translations()
            .expect("el servicio borra");

        assert_eq!(
            service.join().expect("servicio de prueba"),
            ["POST /api/translations/cleanup HTTP/1.1"]
        );
        assert_eq!(done.entries, 1);
        assert!(!cache.contains(&entry_key(1)));
        assert!(cache.contains(&entry_key(2)));
        assert!(cache.contains(&entry_key(3)));
    }
}
