//! Lista de artículos, artículo abierto, marcar leído y guardar (paso 2.2).
//!
//! Todo se reenvía a Miniflux, para que el móvil y el PC vean lo mismo (S1).
//! Los títulos traducidos y el texto en español salen de [`Translations`].
//!
//! [`Translations`]: crate::translations::Translations

use atalaya_api::{
    Entry, EntryPage, EntryQuery, EntrySummary, EntryUpdate, Id, MarkAllReadQuery, View,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;

use crate::{
    AppState,
    error::{AppError, AppResult},
    miniflux::Miniflux,
    papers::saved::PapersPlace,
    translations::TitleInfo,
};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/entries", get(list))
        .route("/entries/{id}", get(open).patch(update))
        .route("/entries/mark-all-as-read", post(mark_all_read))
}

async fn list(
    State(state): State<AppState>,
    query: Result<Query<EntryQuery>, QueryRejection>,
) -> AppResult<Json<EntryPage>> {
    let Query(query) = query.map_err(|e| invalid("parámetros no válidos", e.body_text()))?;
    let place = state.papers.place().await;
    let page = state.miniflux.entries(&query, place).await?;
    let ids: Vec<Id> = page.entries.iter().map(|e| e.id).collect();
    let titles = state.translations.titles(&ids).await;
    Ok(Json(EntryPage {
        total: page.total,
        entries: page
            .entries
            .iter()
            .map(|e| summary(e, titles.get(&e.id)))
            .collect(),
    }))
}

async fn open(
    State(state): State<AppState>,
    id: Result<Path<Id>, PathRejection>,
) -> AppResult<Json<Entry>> {
    let Path(id) = id.map_err(|e| invalid("id no válido", e.body_text()))?;
    let entry = state.miniflux.entry(id).await?;
    let title = title_of(&state, id).await;
    let (translation, translated_content) = state.translations.content(id).await;
    // Si el feed solo trae un resumen, el artículo completo (L7).
    let content = state
        .translations
        .original_content(id, &entry.content)
        .await
        .unwrap_or_else(|| entry.content.clone());
    Ok(Json(Entry {
        summary: summary(&entry, title.as_ref()),
        author: Some(entry.author).filter(|a| !a.trim().is_empty()),
        content,
        translation,
        translated_content,
    }))
}

async fn update(
    State(state): State<AppState>,
    id: Result<Path<Id>, PathRejection>,
    change: Result<Json<EntryUpdate>, JsonRejection>,
) -> AppResult<Json<EntrySummary>> {
    let Path(id) = id.map_err(|e| invalid("id no válido", e.body_text()))?;
    let Json(change) = change.map_err(|e| invalid("cuerpo no válido", e.body_text()))?;
    // Se pide antes para responder 404 si no existe: Miniflux acepta marcar
    // un id que no existe sin quejarse.
    let mut entry = state.miniflux.entry(id).await?;

    if let Some(read) = change.read {
        state.miniflux.set_read(id, read).await?;
        entry.status = if read { "read" } else { "unread" }.into();
    }
    if let Some(starred) = change.starred
        && starred != entry.starred
    {
        // Miniflux solo sabe alternar: se llama solo si hay que cambiarlo.
        state.miniflux.toggle_starred(id).await?;
        entry.starred = starred;
        // Un paper que deja de estar guardado se oculta (ver
        // `entries_request`); leído, tampoco suma en los contadores.
        if !starred
            && change.read.is_none()
            && entry.status != "read"
            && state.papers.saved_feed(false).await.ok().flatten() == Some(entry.feed_id)
        {
            state.miniflux.set_read(id, true).await?;
            entry.status = "read".into();
        }
    }
    if change.starred == Some(true) {
        // Guardar pone en marcha la traducción del texto completo (T2).
        state.translations.content(id).await;
    }

    let title = title_of(&state, id).await;
    Ok(Json(summary(&entry, title.as_ref())))
}

/// Marca como leído lo de un feed, lo de una carpeta o, sin filtro, «Todos».
async fn mark_all_read(
    State(state): State<AppState>,
    query: Result<Query<MarkAllReadQuery>, QueryRejection>,
) -> AppResult<StatusCode> {
    let Query(query) = query.map_err(|e| invalid("parámetros no válidos", e.body_text()))?;
    match (query.feed_id, query.category_id) {
        (Some(feed), _) => state.miniflux.mark_feed_read(feed).await?,
        (None, Some(category)) => state.miniflux.mark_category_read(category).await?,
        (None, None) => {
            // Sin saber qué feed es el de papers, no se marca nada: podría ser ese.
            let papers = state.papers.saved_feed(false).await?;
            for feed in state.miniflux.feed_ids().await? {
                if Some(feed) != papers {
                    state.miniflux.mark_feed_read(feed).await?;
                }
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn title_of(state: &AppState, id: Id) -> Option<TitleInfo> {
    state.translations.titles(&[id]).await.remove(&id)
}

/// Une un artículo de Miniflux con lo que se sabe de su título.
pub(crate) fn summary(entry: &MinifluxEntry, title: Option<&TitleInfo>) -> EntrySummary {
    let translated = title
        .and_then(|t| t.title.as_deref())
        .filter(|t| !t.trim().is_empty());
    EntrySummary {
        id: entry.id,
        feed_id: entry.feed_id,
        feed_title: entry.feed.title.clone(),
        title: translated.unwrap_or(&entry.title).to_owned(),
        original_title: entry.title.clone(),
        title_translated: translated.is_some(),
        lang: title.and_then(|t| t.lang.clone()),
        url: entry.url.clone(),
        published_at: entry.published_at,
        // Miniflux tiene un tercer estado, «removed», que no sale en las
        // listas; si llega, cuenta como leído.
        read: entry.status != "unread",
        starred: entry.starred,
    }
}

/// Ruta de Miniflux y parámetros para una vista de la lista.
/// La petición a Miniflux para una vista.
///
/// Los papers solo se ven mientras están guardados: en su feed y en su carpeta
/// se piden solo los guardados, y en «Todos» y «Sin leer» no salen, porque su
/// feed está oculto en las listas generales. Miniflux no deja borrar un
/// artículo suelto, así que los que se dejan de guardar se quedan ocultos.
fn entries_request(
    query: &EntryQuery,
    papers: Option<PapersPlace>,
) -> (String, Vec<(&'static str, String)>) {
    let path = match (query.feed_id, query.category_id) {
        (Some(feed), _) => format!("/v1/feeds/{feed}/entries"),
        (None, Some(category)) => format!("/v1/categories/{category}/entries"),
        (None, None) => "/v1/entries".to_owned(),
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut params = vec![
        ("order", "published_at".to_owned()),
        ("direction", "desc".to_owned()),
        ("limit", limit.to_string()),
        ("offset", query.offset.unwrap_or(0).to_string()),
    ];
    match query.view {
        // Sin `status`, Miniflux también devuelve los borrados.
        View::All => {
            params.push(("status", "unread".into()));
            params.push(("status", "read".into()));
        }
        View::Unread => params.push(("status", "unread".into())),
        View::Starred => {
            params.push(("starred", "true".into()));
            params.push(("status", "unread".into()));
            params.push(("status", "read".into()));
        }
    }
    if query.view != View::Starred {
        let in_papers = papers.is_some_and(|p| match (query.feed_id, query.category_id) {
            (Some(feed), _) => feed == p.feed,
            (None, Some(category)) => category == p.category,
            (None, None) => false,
        });
        if in_papers {
            params.push(("starred", "true".into()));
        } else if query.feed_id.is_none() && query.category_id.is_none() {
            params.push(("globally_visible", "true".into()));
        }
    }
    (path, params)
}

#[derive(Debug, Deserialize)]
pub(crate) struct MinifluxEntryPage {
    pub total: u64,
    pub entries: Vec<MinifluxEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MinifluxEntry {
    pub id: Id,
    pub feed_id: Id,
    /// `unread`, `read` o `removed`.
    pub status: String,
    pub title: String,
    pub url: String,
    pub published_at: DateTime<Utc>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub content: String,
    pub starred: bool,
    pub feed: MinifluxEntryFeed,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MinifluxEntryFeed {
    pub title: String,
}

impl Miniflux {
    pub(crate) async fn entries(
        &self,
        query: &EntryQuery,
        papers: Option<PapersPlace>,
    ) -> AppResult<MinifluxEntryPage> {
        let (path, params) = entries_request(query, papers);
        let mut page: MinifluxEntryPage =
            Self::send(self.request(Method::GET, &path).query(&params)).await?;
        sort_newest_first(&mut page.entries);
        Ok(page)
    }

    pub(crate) async fn entry(&self, id: Id) -> AppResult<MinifluxEntry> {
        self.get(&format!("/v1/entries/{id}"))
            .await
            .map_err(|e| not_found_in_spanish(e, id))
    }

    pub(crate) async fn set_read(&self, id: Id, read: bool) -> AppResult<()> {
        let body = serde_json::json!({
            "entry_ids": [id],
            "status": if read { "read" } else { "unread" },
        });
        Self::send_empty(self.request(Method::PUT, "/v1/entries").json(&body)).await?;
        Ok(())
    }

    pub(crate) async fn mark_feed_read(&self, feed: Id) -> AppResult<()> {
        let path = format!("/v1/feeds/{feed}/mark-all-as-read");
        Self::send_empty(self.request(Method::PUT, &path)).await?;
        Ok(())
    }

    pub(crate) async fn mark_category_read(&self, category: Id) -> AppResult<()> {
        let path = format!("/v1/categories/{category}/mark-all-as-read");
        Self::send_empty(self.request(Method::PUT, &path)).await?;
        Ok(())
    }

    /// Los ids de todos los feeds, también el oculto de papers.
    pub(crate) async fn feed_ids(&self) -> AppResult<Vec<Id>> {
        #[derive(Deserialize)]
        struct FeedId {
            id: Id,
        }
        let feeds: Vec<FeedId> = self.get("/v1/feeds").await?;
        Ok(feeds.into_iter().map(|f| f.id).collect())
    }

    pub(crate) async fn toggle_starred(&self, id: Id) -> AppResult<()> {
        Self::send_empty(self.request(Method::PUT, &format!("/v1/entries/{id}/bookmark"))).await?;
        Ok(())
    }

    /// Total de una vista, sin traer sus artículos.
    pub(crate) async fn count(&self, view: View, papers: Option<PapersPlace>) -> AppResult<u64> {
        let query = EntryQuery {
            view,
            limit: Some(1),
            ..Default::default()
        };
        Ok(self.entries(&query, papers).await?.total)
    }
}

/// Petición mal formada, como [`atalaya_api::ApiError`] y no como texto suelto.
fn invalid(what: &str, detail: String) -> AppError {
    AppError::bad_request(format!("{what}: {detail}"))
}

/// Del más nuevo al más viejo. Con la misma fecha, el id más alto primero.
/// Miniflux solo acepta un orden, así que el desempate se hace aquí, dentro
/// de la página. Entre páginas, los empates pueden repetirse o saltarse.
pub(crate) fn sort_newest_first(entries: &mut [MinifluxEntry]) {
    entries.sort_by(|a, b| b.published_at.cmp(&a.published_at).then(b.id.cmp(&a.id)));
}

fn not_found_in_spanish(e: AppError, id: Id) -> AppError {
    if e.status == axum::http::StatusCode::NOT_FOUND {
        AppError::not_found(format!("no existe el artículo {id}"))
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Para que las pruebas lean los parámetros como un mapa.
    fn params_map(params: &[(&'static str, String)]) -> HashMap<&'static str, Vec<String>> {
        let mut map: HashMap<_, Vec<String>> = HashMap::new();
        for (k, v) in params {
            map.entry(*k).or_default().push(v.clone());
        }
        map
    }

    const MINIFLUX_ENTRY: &str = r#"{
        "id": 7, "user_id": 1, "feed_id": 1, "status": "read",
        "hash": "x", "title": "How to Do Great Work",
        "url": "http://www.paulgraham.com/greatwork.html", "comments_url": "",
        "language": "", "published_at": "2023-07-01T10:00:00-04:00",
        "created_at": "2026-10-23T16:31:09.83048Z", "changed_at": "2026-10-23T16:31:09.83048Z",
        "content": "<p>Hello</p>", "author": "", "share_code": "", "starred": true,
        "reading_time": 1, "enclosures": [], "tags": [],
        "feed": {"id": 1, "title": "Paul Graham: Essays", "category": {"id": 1, "title": "All"}}
    }"#;

    fn entry() -> MinifluxEntry {
        serde_json::from_str(MINIFLUX_ENTRY).unwrap()
    }

    #[test]
    fn ties_in_date_go_by_newest_id() {
        let at = |id: Id, date: &str| MinifluxEntry {
            id,
            published_at: date.parse().unwrap(),
            ..entry()
        };
        let mut entries = vec![
            at(5, "2023-07-01T10:00:00Z"),
            at(9, "2023-07-01T10:00:00Z"),
            at(7, "2023-07-02T10:00:00Z"),
        ];
        sort_newest_first(&mut entries);
        let ids: Vec<Id> = entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, [7, 9, 5]);
    }

    #[test]
    fn untranslated_title_falls_back_to_original() {
        let s = summary(&entry(), None);
        assert_eq!(s.id, 7);
        assert_eq!(s.feed_title, "Paul Graham: Essays");
        assert_eq!(s.title, "How to Do Great Work");
        assert_eq!(s.original_title, "How to Do Great Work");
        assert!(!s.title_translated);
        assert_eq!(s.lang, None);
        assert!(s.read);
        assert!(s.starred);
        assert_eq!(s.published_at.to_rfc3339(), "2023-07-01T14:00:00+00:00");
    }

    #[test]
    fn translated_title_wins() {
        let info = TitleInfo {
            title: Some("Cómo hacer un gran trabajo".into()),
            lang: Some("en".into()),
        };
        let s = summary(&entry(), Some(&info));
        assert_eq!(s.title, "Cómo hacer un gran trabajo");
        assert_eq!(s.original_title, "How to Do Great Work");
        assert!(s.title_translated);
        assert_eq!(s.lang.as_deref(), Some("en"));
    }

    #[test]
    fn language_without_translation_keeps_original() {
        let info = TitleInfo {
            title: None,
            lang: Some("es".into()),
        };
        let s = summary(&entry(), Some(&info));
        assert_eq!(s.title, "How to Do Great Work");
        assert!(!s.title_translated);
        assert_eq!(s.lang.as_deref(), Some("es"));
    }

    #[test]
    fn unread_and_removed_status() {
        let mut e = entry();
        e.status = "unread".into();
        assert!(!summary(&e, None).read);
        e.status = "removed".into();
        assert!(summary(&e, None).read);
    }

    #[test]
    fn all_view_excludes_removed() {
        let (path, params) = entries_request(&EntryQuery::default(), None);
        let p = params_map(&params);
        assert_eq!(path, "/v1/entries");
        assert_eq!(p["status"], ["unread", "read"]);
        assert_eq!(p["limit"], ["50"]);
        assert_eq!(p["offset"], ["0"]);
        assert_eq!(p["order"], ["published_at"]);
        assert_eq!(p["direction"], ["desc"]);
        assert!(!p.contains_key("starred"));
        assert_eq!(p["globally_visible"], ["true"]);
    }

    #[test]
    fn papers_show_only_while_saved() {
        let papers = Some(PapersPlace {
            feed: 16,
            category: 9,
        });
        let request = |view, feed_id, category_id| {
            let query = EntryQuery {
                view,
                feed_id,
                category_id,
                ..Default::default()
            };
            params_map(&entries_request(&query, papers).1)
        };
        // Su feed y su carpeta: solo los guardados.
        assert_eq!(request(View::All, Some(16), None)["starred"], ["true"]);
        assert_eq!(request(View::Unread, None, Some(9))["starred"], ["true"]);
        // Otro feed u otra carpeta: como siempre.
        assert!(!request(View::All, Some(3), None).contains_key("starred"));
        assert!(!request(View::All, None, Some(2)).contains_key("starred"));
        assert!(!request(View::All, None, Some(2)).contains_key("globally_visible"));
        // «Todos» y «Sin leer» sin el feed oculto; Guardados, con todo.
        let unread = request(View::Unread, None, None);
        assert_eq!(unread["globally_visible"], ["true"]);
        assert!(!unread.contains_key("starred"));
        let starred = request(View::Starred, None, None);
        assert!(!starred.contains_key("globally_visible"));
        assert_eq!(starred["starred"], ["true"]);
    }

    #[test]
    fn unread_view_of_a_category() {
        let query = EntryQuery {
            view: View::Unread,
            category_id: Some(3),
            limit: Some(500),
            offset: Some(100),
            ..Default::default()
        };
        let (path, params) = entries_request(&query, None);
        let p = params_map(&params);
        assert_eq!(path, "/v1/categories/3/entries");
        assert_eq!(p["status"], ["unread"]);
        assert_eq!(p["limit"], ["200"]);
        assert_eq!(p["offset"], ["100"]);
    }

    #[test]
    fn feed_wins_over_category() {
        let query = EntryQuery {
            view: View::Starred,
            feed_id: Some(1),
            category_id: Some(3),
            limit: Some(0),
            ..Default::default()
        };
        let (path, params) = entries_request(&query, None);
        let p = params_map(&params);
        assert_eq!(path, "/v1/feeds/1/entries");
        assert_eq!(p["starred"], ["true"]);
        assert_eq!(p["limit"], ["1"]);
    }

    #[test]
    fn query_string_parses() {
        let uri: axum::http::Uri = "/api/entries?view=starred&feed_id=1&limit=10"
            .parse()
            .unwrap();
        let Query(q) = Query::<EntryQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(q.view, View::Starred);
        assert_eq!(q.feed_id, Some(1));
        assert_eq!(q.limit, Some(10));
        assert_eq!(q.category_id, None);
    }

    #[tokio::test]
    async fn routes_need_the_key() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let dir = tempfile::tempdir().unwrap();
        let app = crate::app(AppState::new(crate::config::Config {
            api_key: "secreta".into(),
            bind: "127.0.0.1:0".parse().unwrap(),
            data_dir: dir.path().to_owned(),
            miniflux_url: "http://127.0.0.1:9".into(),
            miniflux_api_key: "x".into(),
            miniflux_webhook_secret: String::new(),
            llama_url: "http://127.0.0.1:9".into(),
            llama_model: "x".into(),
        }));
        for (method, uri) in [
            ("GET", "/api/sidebar"),
            ("GET", "/api/entries"),
            ("GET", "/api/entries/1"),
            ("PATCH", "/api/entries/1"),
            ("POST", "/api/entries/mark-all-as-read"),
        ] {
            let req = Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap();
            let status = app.clone().oneshot(req).await.unwrap().status();
            assert_eq!(
                status,
                axum::http::StatusCode::UNAUTHORIZED,
                "{method} {uri}"
            );
        }
    }
}
