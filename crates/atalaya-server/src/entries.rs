//! Lista de artículos, artículo abierto, marcar leído y guardar (paso 2.2).
//!
//! Todo se reenvía a Miniflux, para que el móvil y el PC vean lo mismo (S1).
//! Los títulos traducidos y el texto en español salen de [`Translations`].
//!
//! [`Translations`]: crate::translations::Translations

use atalaya_api::{Entry, EntryPage, EntryQuery, EntrySummary, EntryUpdate, Id, View};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    routing::get,
};
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;

use crate::{
    AppState,
    error::{AppError, AppResult},
    miniflux::Miniflux,
    translations::TitleInfo,
};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/entries", get(list))
        .route("/entries/{id}", get(open).patch(update))
}

async fn list(
    State(state): State<AppState>,
    query: Result<Query<EntryQuery>, QueryRejection>,
) -> AppResult<Json<EntryPage>> {
    let Query(query) = query.map_err(|e| invalid("parámetros no válidos", e.body_text()))?;
    let page = state.miniflux.entries(&query).await?;
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
    Ok(Json(Entry {
        summary: summary(&entry, title.as_ref()),
        author: Some(entry.author).filter(|a| !a.trim().is_empty()),
        content: entry.content,
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
    }
    if change.starred == Some(true) {
        // Guardar pone en marcha la traducción del texto completo (T2).
        state.translations.content(id).await;
    }

    let title = title_of(&state, id).await;
    Ok(Json(summary(&entry, title.as_ref())))
}

async fn title_of(state: &AppState, id: Id) -> Option<TitleInfo> {
    state.translations.titles(&[id]).await.remove(&id)
}

/// Une un artículo de Miniflux con lo que se sabe de su título.
fn summary(entry: &MinifluxEntry, title: Option<&TitleInfo>) -> EntrySummary {
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
fn entries_request(query: &EntryQuery) -> (String, Vec<(&'static str, String)>) {
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
    pub(crate) async fn entries(&self, query: &EntryQuery) -> AppResult<MinifluxEntryPage> {
        let (path, params) = entries_request(query);
        Self::send(self.request(Method::GET, &path).query(&params)).await
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

    pub(crate) async fn toggle_starred(&self, id: Id) -> AppResult<()> {
        Self::send_empty(self.request(Method::PUT, &format!("/v1/entries/{id}/bookmark"))).await?;
        Ok(())
    }

    /// Total de una vista, sin traer sus artículos.
    pub(crate) async fn count(&self, view: View) -> AppResult<u64> {
        let query = EntryQuery {
            view,
            limit: Some(1),
            ..Default::default()
        };
        Ok(self.entries(&query).await?.total)
    }
}

/// Petición mal formada, como [`atalaya_api::ApiError`] y no como texto suelto.
fn invalid(what: &str, detail: String) -> AppError {
    AppError::bad_request(format!("{what}: {detail}"))
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
        let (path, params) = entries_request(&EntryQuery::default());
        let p = params_map(&params);
        assert_eq!(path, "/v1/entries");
        assert_eq!(p["status"], ["unread", "read"]);
        assert_eq!(p["limit"], ["50"]);
        assert_eq!(p["offset"], ["0"]);
        assert_eq!(p["order"], ["published_at"]);
        assert_eq!(p["direction"], ["desc"]);
        assert!(!p.contains_key("starred"));
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
        let (path, params) = entries_request(&query);
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
        let (path, params) = entries_request(&query);
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
