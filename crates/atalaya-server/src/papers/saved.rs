//! Papers guardados (paso 4.2, P5, ADR 0005).
//!
//! Un paper guardado es un artículo de Miniflux en el feed «Papers guardados»,
//! dentro de la carpeta «Papers», marcado como guardado. Así sale en
//! Guardados y se sincroniza como cualquier artículo.
//!
//! # El feed
//!
//! Miniflux no deja crear un feed sin descargar su dirección. El servicio lo
//! crea con una consulta a arXiv que nunca trae nada ([`FEED_URL`]: pide un
//! identificador que no puede existir, porque los nuevos empiezan en `0704`)
//! y después lo desactiva, para que Miniflux no vuelva a
//! descargarlo. Así no entra ningún artículo ajeno y no puede fallar al
//! refrescar. Lleva activado «artículo completo»: si alguna vez hay que
//! traducir el texto por el camino normal (`content`), se traduce el resumen
//! guardado y no la web de arXiv. El feed se busca por su dirección, así que
//! Rubén puede cambiarle el nombre o la carpeta.
//!
//! # Guardar
//!
//! Con «Import Entry» de Miniflux (desde 2.2.16). Si el paper ya está en el
//! feed, solo se vuelve a marcar como guardado. El título y el resumen en
//! español se escriben en la base del servicio con el hash del texto tal como
//! lo guardó Miniflux, para que en Guardados salgan traducidos sin esperar.

use std::collections::HashMap;

use atalaya_api::Id;
use chrono::Utc;
use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

use super::{
    Papers,
    arxiv::{self, ArxivPaper},
};
use crate::{
    error::{AppError, AppResult},
    miniflux::Miniflux,
    store::{self, FIELD_CONTENT, FIELD_TITLE, SeenEntry},
};

pub const CATEGORY_TITLE: &str = "Papers";
pub const FEED_TITLE: &str = "Papers guardados";

/// Una consulta válida a arXiv que siempre responde un Atom sin entradas.
/// Con `max_results=0`, arXiv responde 500.
pub const FEED_URL: &str = "https://export.arxiv.org/api/query?id_list=0000.00000";

/// Artículos por página al leer el feed.
const PAGE_SIZE: usize = 250;

/// Un artículo del feed de papers.
#[derive(Debug, Clone, Deserialize)]
pub struct SavedEntry {
    pub id: Id,
    pub url: String,
    pub starred: bool,
}

/// El identificador de arXiv de una dirección `https://arxiv.org/abs/…`.
pub fn arxiv_id_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://arxiv.org/abs/")
        .or_else(|| url.strip_prefix("http://arxiv.org/abs/"))?;
    let id = arxiv::strip_version(rest.trim_end_matches('/'));
    arxiv::is_valid_id(id).then(|| id.to_owned())
}

/// HTML del artículo que se guarda en Miniflux: el resumen y los enlaces a la
/// página de arXiv y al PDF (P4). Con el resumen traducido sale la versión en
/// español, con los mismos enlaces.
pub fn entry_html(arxiv_id: &str, abstract_text: &str) -> String {
    let mut html = String::new();
    for p in abstract_text.split("\n\n").filter(|p| !p.trim().is_empty()) {
        html.push_str("<p>");
        html.push_str(&escape(p.trim()));
        html.push_str("</p>");
    }
    html.push_str(&format!(
        "<p><a href=\"{}\">Ver en arXiv</a> · <a href=\"{}\">Abrir PDF</a></p>",
        arxiv::abs_url(arxiv_id),
        arxiv::pdf_url(arxiv_id)
    ));
    html
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Qué pasó al guardar.
pub struct Saved {
    pub entry_id: Id,
    /// `false` si ya estaba en el feed.
    pub created: bool,
}

impl Papers {
    /// El feed «Papers guardados». Con `create`, lo crea si no existe.
    pub async fn saved_feed(&self, create: bool) -> AppResult<Option<Id>> {
        let mut cached = self.saved_feed_cache().lock().await;
        if let Some(id) = *cached {
            return Ok(Some(id));
        }
        let miniflux = self.translations().miniflux();
        let found = miniflux.feed_by_url(FEED_URL).await?;
        let id = match found {
            Some(id) => id,
            None if create => miniflux.create_papers_feed().await?,
            None => return Ok(None),
        };
        *cached = Some(id);
        Ok(Some(id))
    }

    /// Papers guardados (marcados como guardados en Miniflux), por
    /// identificador de arXiv. Si Miniflux falla, vacío: la búsqueda sigue.
    pub async fn saved_entries(&self) -> HashMap<String, Id> {
        let feed = match self.saved_feed(false).await {
            Ok(Some(feed)) => feed,
            Ok(None) => return HashMap::new(),
            Err(e) => {
                tracing::warn!("no se pudo buscar el feed de papers: {}", e.message);
                return HashMap::new();
            }
        };
        match self
            .translations()
            .miniflux()
            .papers_feed_entries(feed)
            .await
        {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| e.starred)
                .filter_map(|e| Some((arxiv_id_of(&e.url)?, e.id)))
                .collect(),
            Err(e) => {
                if e.status == axum::http::StatusCode::NOT_FOUND {
                    // Rubén borró el feed: se creará otra vez al guardar.
                    *self.saved_feed_cache().lock().await = None;
                }
                tracing::warn!("no se pudieron leer los papers guardados: {}", e.message);
                HashMap::new()
            }
        }
    }

    /// Guarda el paper en Miniflux, o lo vuelve a marcar como guardado si ya
    /// estaba, y escribe sus traducciones en la base.
    pub async fn save(&self, paper: &ArxivPaper) -> AppResult<Saved> {
        let feed = self
            .saved_feed(true)
            .await?
            .expect("con create, el feed existe");
        let miniflux = self.translations().miniflux();
        let entries = match miniflux.papers_feed_entries(feed).await {
            Ok(e) => e,
            Err(e) if e.status == axum::http::StatusCode::NOT_FOUND => {
                *self.saved_feed_cache().lock().await = None;
                return Err(AppError::new(
                    axum::http::StatusCode::CONFLICT,
                    "el feed «Papers guardados» ya no existe; vuelve a intentarlo",
                ));
            }
            Err(e) => return Err(e),
        };
        let existing = entries
            .iter()
            .find(|e| arxiv_id_of(&e.url).as_deref() == Some(paper.arxiv_id.as_str()));

        let saved = match existing {
            Some(e) => {
                if !e.starred {
                    miniflux.toggle_starred(e.id).await?;
                }
                Saved {
                    entry_id: e.id,
                    created: false,
                }
            }
            None => {
                let (entry_id, created) = miniflux.import_paper(feed, paper).await?;
                Saved { entry_id, created }
            }
        };
        self.write_translations(saved.entry_id, paper).await;
        Ok(saved)
    }

    /// Escribe el título y el resumen en español del artículo de Miniflux.
    /// Si el resumen aún se está traduciendo, lo escribe al terminar; mientras,
    /// el artículo sale «Traduciendo…» y abrirlo no empieza otra traducción.
    async fn write_translations(&self, entry_id: Id, paper: &ArxivPaper) {
        if super::abstracts::is_spanish(paper) {
            return;
        }
        let entry = match self.translations().miniflux().entry(entry_id).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(entry = entry_id, "paper guardado sin leer: {}", e.message);
                return;
            }
        };
        let feed_id = entry.feed_id;
        let entry_title = entry.title.clone();
        let entry_content = entry.content.clone();

        let title = self
            .titles(vec![super::to_translate(paper)])
            .await
            .remove(&paper.arxiv_id);
        let model = self.llama().model().to_owned();
        let seen = SeenEntry {
            entry_id,
            feed_id,
            detected_lang: Some("en".into()),
        };
        let m = model.clone();
        let saved = self
            .store()
            .call(move |c| -> rusqlite::Result<()> {
                store::record_langs(c, &[seen])?;
                if let Some(t) = title {
                    let hash = store::source_hash(&entry_title);
                    store::save_translation(c, entry_id, FIELD_TITLE, &hash, &m, &Ok(t))?;
                }
                Ok(())
            })
            .await;
        if let Err(e) = saved {
            tracing::error!(
                entry = entry_id,
                "no se pudo guardar el título del paper: {e}"
            );
        }

        let Some(guard) = self.translations().hold_in_progress(entry_id) else {
            return;
        };
        let this = self.clone();
        let paper = paper.clone();
        let task = async move {
            let _guard = guard;
            let Some(text) = this.wait_abstract(&paper).await else {
                return;
            };
            let html = entry_html(&paper.arxiv_id, &text);
            let hash = store::source_hash(&entry_content);
            let saved = this
                .store()
                .call(move |c| {
                    store::save_translation(c, entry_id, FIELD_CONTENT, &hash, &model, &Ok(html))
                })
                .await;
            match saved {
                Ok(()) => tracing::info!(entry = entry_id, "resumen del paper guardado en español"),
                Err(e) => tracing::error!(entry = entry_id, "no se pudo guardar el resumen: {e}"),
            }
        };
        // Casi siempre el resumen ya está traducido, porque Rubén abrió el
        // paper antes de guardarlo: entonces se escribe antes de responder.
        let handle = tokio::spawn(task);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
    }
}

impl Miniflux {
    async fn feed_by_url(&self, url: &str) -> AppResult<Option<Id>> {
        #[derive(Deserialize)]
        struct Feed {
            id: Id,
            feed_url: String,
        }
        let feeds: Vec<Feed> = self.get("/v1/feeds").await?;
        Ok(feeds.into_iter().find(|f| f.feed_url == url).map(|f| f.id))
    }

    /// Crea la carpeta «Papers» si no existe y, dentro, el feed.
    async fn create_papers_feed(&self) -> AppResult<Id> {
        #[derive(Deserialize)]
        struct Category {
            id: Id,
            title: String,
        }
        #[derive(Deserialize)]
        struct Created {
            feed_id: Id,
        }
        let categories: Vec<Category> = self.get("/v1/categories").await?;
        let category = match categories
            .into_iter()
            .find(|c| c.title.eq_ignore_ascii_case(CATEGORY_TITLE))
        {
            Some(c) => c.id,
            None => {
                let c: Category = self
                    .send_json(
                        Method::POST,
                        "/v1/categories",
                        &json!({ "title": CATEGORY_TITLE }),
                    )
                    .await?;
                c.id
            }
        };
        let created: Created = self
            .send_json(
                Method::POST,
                "/v1/feeds",
                &json!({ "feed_url": FEED_URL, "category_id": category, "crawler": true }),
            )
            .await?;
        // Sin `site_url`, Miniflux pone la dirección de la consulta.
        let settings = json!({
            "title": FEED_TITLE,
            "site_url": "https://arxiv.org/",
            "disabled": true,
            "crawler": true,
        });
        let _: serde_json::Value = self
            .send_json(
                Method::PUT,
                &format!("/v1/feeds/{}", created.feed_id),
                &settings,
            )
            .await?;
        tracing::info!(feed = created.feed_id, "creado el feed «{FEED_TITLE}»");
        Ok(created.feed_id)
    }

    /// Todos los artículos del feed de papers, leídos y sin leer.
    async fn papers_feed_entries(&self, feed: Id) -> AppResult<Vec<SavedEntry>> {
        #[derive(Deserialize)]
        struct Page {
            entries: Vec<SavedEntry>,
        }
        let mut out = Vec::new();
        loop {
            let path = format!(
                "/v1/feeds/{feed}/entries?status=unread&status=read&order=id&direction=asc&limit={PAGE_SIZE}&offset={}",
                out.len()
            );
            let page: Page = self.get(&path).await?;
            let got = page.entries.len();
            out.extend(page.entries);
            if got < PAGE_SIZE {
                return Ok(out);
            }
        }
    }

    /// Importa el paper como artículo guardado y sin leer. Devuelve su id y
    /// si es nuevo.
    async fn import_paper(&self, feed: Id, paper: &ArxivPaper) -> AppResult<(Id, bool)> {
        let mut body = json!({
            "url": arxiv::abs_url(&paper.arxiv_id),
            "title": paper.title,
            "content": entry_html(&paper.arxiv_id, &paper.abstract_text),
            "author": paper.authors.join(", "),
            "published_at": paper.published_at.timestamp(),
            "status": "unread",
            "starred": true,
            "external_id": paper.arxiv_id,
        });
        match self.import_entry(feed, &body).await {
            // Miniflux no deja volver a crear un artículo que borró (por
            // ejemplo, al limpiar los viejos): se importa con otro id externo.
            Err(e) if e.message.contains("tombstoned") => {
                body["external_id"] =
                    json!(format!("{}@{}", paper.arxiv_id, Utc::now().timestamp()));
                self.import_entry(feed, &body).await
            }
            other => other,
        }
    }

    async fn import_entry(&self, feed: Id, body: &serde_json::Value) -> AppResult<(Id, bool)> {
        #[derive(Deserialize)]
        struct Imported {
            id: Id,
        }
        let resp = Self::send_empty(
            self.request(Method::POST, &format!("/v1/feeds/{feed}/entries/import"))
                .json(body),
        )
        .await?;
        let created = resp.status() == reqwest::StatusCode::CREATED;
        let imported: Imported = resp
            .json()
            .await
            .map_err(|e| AppError::bad_gateway(format!("respuesta de Miniflux no válida: {e}")))?;
        Ok((imported.id, created))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_arxiv_ids_from_urls() {
        assert_eq!(
            arxiv_id_of("https://arxiv.org/abs/2610.08812").as_deref(),
            Some("2610.08812")
        );
        assert_eq!(
            arxiv_id_of("http://arxiv.org/abs/hep-th/9901001v2").as_deref(),
            Some("hep-th/9901001")
        );
        assert_eq!(arxiv_id_of("https://example.com/abs/2610.08812"), None);
    }

    #[test]
    fn entry_html_has_abstract_and_links() {
        let html = entry_html("2610.08812", "Uno & <dos>.\n\nTres.");
        assert_eq!(
            html,
            "<p>Uno &amp; &lt;dos&gt;.</p><p>Tres.</p>\
             <p><a href=\"https://arxiv.org/abs/2610.08812\">Ver en arXiv</a> · \
             <a href=\"https://arxiv.org/pdf/2610.08812\">Abrir PDF</a></p>"
        );
    }
}
