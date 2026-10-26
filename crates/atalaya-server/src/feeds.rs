//! Feeds y carpetas (F1–F6, ADR 0007), reenviados a la API de Miniflux.
//!
//! En Miniflux una carpeta es una «categoría».

use std::time::Duration;

use atalaya_api::{
    Category, CategoryInput, DiscoverRequest, DiscoveredFeed, Feed, FeedError, FeedUpdate, Id,
    NewFeed,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, patch, post},
};
use chrono::{DateTime, Datelike, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use crate::{
    AppState,
    error::{ApiJson, ApiPath, AppError, AppResult},
    feed_stats::{EntrySample, FeedStats, SAMPLE},
    miniflux::Miniflux,
    papers::saved::FEED_URL,
    stats_cache::StatsCache,
};

/// Tiempo máximo para descargar un feed encontrado y calcular sus datos.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/feeds/discover", post(discover))
        .route("/feeds", get(list_feeds).post(create_feed))
        .route("/feeds/{id}", patch(update_feed).delete(delete_feed))
        .route("/categories", get(list_categories).post(create_category))
        .route(
            "/categories/{id}",
            patch(rename_category).delete(delete_category),
        )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn discover(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<DiscoverRequest>,
) -> AppResult<Json<Vec<DiscoveredFeed>>> {
    let url = normalize_url(&req.url)?;
    let found = state.miniflux.discover(&url).await.map_err(remote_error)?;

    let mut tasks = JoinSet::new();
    for (i, sub) in found.into_iter().enumerate() {
        let http = state.http.clone();
        tasks.spawn(async move { (i, analyze_remote(&http, sub).await) });
    }
    let mut feeds: Vec<_> = tasks.join_all().await;
    feeds.sort_by_key(|(i, _)| *i);
    Ok(Json(feeds.into_iter().map(|(_, f)| f).collect()))
}

async fn list_feeds(State(state): State<AppState>) -> AppResult<Json<Vec<Feed>>> {
    let feeds = state.miniflux.feeds().await?;
    let mut tasks = JoinSet::new();
    for feed in feeds {
        let miniflux = state.miniflux.clone();
        let cache = state.stats.clone();
        tasks.spawn(async move { miniflux.with_stats(feed, &cache).await });
    }
    let mut feeds: Vec<Feed> = tasks.join_all().await;
    feeds.sort_by_cached_key(|f| (f.category.title.to_lowercase(), f.title.to_lowercase()));
    Ok(Json(feeds))
}

async fn create_feed(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<NewFeed>,
) -> AppResult<(StatusCode, Json<Feed>)> {
    let feed_url = normalize_url(&req.feed_url)?;
    // Miniflux descarga cada artículo antes de responder y puede tardar más de
    // lo que espera la app. Si la app corta, axum suelta este handler: en una
    // tarea aparte, el feed nuevo llega igual a la cola de títulos (#34).
    let creation = tokio::spawn({
        let state = state.clone();
        async move {
            let id = state
                .miniflux
                .create_feed(&feed_url, req.category_id, req.crawler)
                .await
                .map_err(remote_error)?;
            state.stats.clear();
            state.translations.submit_new_feed(id);
            Ok::<_, AppError>(id)
        }
    });
    let id = creation
        .await
        .map_err(|e| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    let feed = state.miniflux.feed(id).await?;
    Ok((
        StatusCode::CREATED,
        Json(state.miniflux.with_stats(feed, &state.stats).await),
    ))
}

async fn update_feed(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Id>,
    ApiJson(req): ApiJson<FeedUpdate>,
) -> AppResult<Json<Feed>> {
    let title = req.title.as_deref().map(non_empty_title).transpose()?;
    let body = MinifluxFeedModification {
        title,
        category_id: req.category_id,
        crawler: req.crawler,
    };
    let feed = state.miniflux.update_feed(id, &body).await?;
    state.stats.clear();
    Ok(Json(state.miniflux.with_stats(feed, &state.stats).await))
}

async fn delete_feed(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Id>,
) -> AppResult<StatusCode> {
    state.miniflux.delete(&format!("/v1/feeds/{id}")).await?;
    state.stats.clear();
    Ok(StatusCode::NO_CONTENT)
}

async fn list_categories(State(state): State<AppState>) -> AppResult<Json<Vec<Category>>> {
    let mut categories: Vec<Category> = state
        .miniflux
        .get::<Vec<MinifluxCategory>>("/v1/categories")
        .await?
        .into_iter()
        .map(Category::from)
        .collect();
    categories.sort_by_cached_key(|c| c.title.to_lowercase());
    Ok(Json(categories))
}

async fn create_category(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CategoryInput>,
) -> AppResult<(StatusCode, Json<Category>)> {
    let body = CategoryInput {
        title: non_empty_title(&req.title)?,
    };
    let category: MinifluxCategory = state
        .miniflux
        .send_json(Method::POST, "/v1/categories", &body)
        .await?;
    Ok((StatusCode::CREATED, Json(category.into())))
}

async fn rename_category(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Id>,
    ApiJson(req): ApiJson<CategoryInput>,
) -> AppResult<Json<Category>> {
    let body = CategoryInput {
        title: non_empty_title(&req.title)?,
    };
    let category: MinifluxCategory = state
        .miniflux
        .send_json(Method::PUT, &format!("/v1/categories/{id}"), &body)
        .await?;
    Ok(Json(category.into()))
}

/// Solo borra carpetas vacías. Miniflux borraría también sus feeds.
async fn delete_category(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Id>,
) -> AppResult<StatusCode> {
    let feeds: Vec<MinifluxFeed> = state
        .miniflux
        .get(&format!("/v1/categories/{id}/feeds"))
        .await?;
    if !feeds.is_empty() {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            format!(
                "la carpeta tiene {} feed(s): muévelos o quítalos antes de borrarla",
                feeds.len()
            ),
        ));
    }
    state
        .miniflux
        .delete(&format!("/v1/categories/{id}"))
        .await
        .map(|()| StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Ayudas
// ---------------------------------------------------------------------------

/// Añade `https://` si falta. Miniflux rechaza una dirección sin esquema.
pub fn normalize_url(url: &str) -> AppResult<String> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::bad_request("falta la dirección"));
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(url.to_owned())
    } else if url.contains("://") {
        Err(AppError::bad_request("la dirección debe ser http o https"))
    } else {
        Ok(format!("https://{url}"))
    }
}

/// Miniflux responde 500 cuando no puede descargar la web o el feed. No es
/// un fallo del servicio, sino de la web de fuera: se responde 502.
fn remote_error(e: AppError) -> AppError {
    if e.status == StatusCode::INTERNAL_SERVER_ERROR {
        AppError::bad_gateway(e.message)
    } else {
        e
    }
}

fn non_empty_title(title: &str) -> AppResult<String> {
    let title = title.trim();
    if title.is_empty() {
        Err(AppError::bad_request("el nombre no puede estar vacío"))
    } else {
        Ok(title.to_owned())
    }
}

/// Descarga un feed encontrado y calcula su idioma y artículos al día. Si no
/// se puede, lo devuelve igual, sin esos datos.
async fn analyze_remote(http: &reqwest::Client, sub: MinifluxSubscription) -> DiscoveredFeed {
    let parsed = match fetch_feed(http, &sub.url).await {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(url = %sub.url, "no se pudo analizar el feed: {e}");
            None
        }
    };
    discovered_feed(sub, parsed.as_ref(), Utc::now())
}

async fn fetch_feed(http: &reqwest::Client, url: &str) -> Result<feed_rs::model::Feed, String> {
    let resp = http
        .get(url)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    feed_rs::parser::parse(bytes.as_ref()).map_err(|e| e.to_string())
}

fn discovered_feed(
    sub: MinifluxSubscription,
    parsed: Option<&feed_rs::model::Feed>,
    now: DateTime<Utc>,
) -> DiscoveredFeed {
    let stats = parsed
        .map(|feed| {
            let samples: Vec<_> = feed.entries.iter().map(sample_from_parsed).collect();
            FeedStats::compute(&samples, feed.language.as_deref(), now)
        })
        .unwrap_or_default();
    let title = Some(sub.title.trim().to_owned())
        .filter(|t| !t.is_empty())
        .or_else(|| parsed.and_then(|f| f.title.as_ref().map(|t| t.content.trim().to_owned())))
        .unwrap_or_else(|| sub.url.clone());
    DiscoveredFeed {
        feed_url: sub.url,
        title,
        lang: stats.lang,
        articles_per_day: stats.articles_per_day,
    }
}

fn sample_from_parsed(entry: &feed_rs::model::Entry) -> EntrySample {
    let content = entry
        .content
        .as_ref()
        .and_then(|c| c.body.clone())
        .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
        .unwrap_or_default();
    EntrySample {
        title: entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default(),
        content,
        published_at: entry.published.or(entry.updated),
    }
}

/// Convierte un feed de Miniflux al de la API, con sus datos calculados.
fn feed_from_miniflux(feed: MinifluxFeed, stats: FeedStats) -> Feed {
    let error = (feed.parsing_error_count > 0).then(|| FeedError {
        message: feed.parsing_error_message.clone(),
        count: feed.parsing_error_count,
    });
    let papers = feed.feed_url == FEED_URL;
    Feed {
        id: feed.id,
        title: feed.title,
        site_url: feed.site_url,
        feed_url: feed.feed_url,
        category: feed.category.into(),
        crawler: feed.crawler,
        lang: stats.lang,
        articles_per_day: stats.articles_per_day,
        // Miniflux pone el año 1 a un feed que aún no ha comprobado.
        checked_at: feed.checked_at.filter(|d| d.year() > 1),
        error,
        papers,
    }
}

// ---------------------------------------------------------------------------
// Miniflux
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct MinifluxSubscription {
    title: String,
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MinifluxCategory {
    id: Id,
    title: String,
}

impl From<MinifluxCategory> for Category {
    fn from(c: MinifluxCategory) -> Self {
        Self {
            id: c.id,
            title: c.title,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MinifluxFeed {
    id: Id,
    title: String,
    site_url: String,
    feed_url: String,
    category: MinifluxCategory,
    crawler: bool,
    #[serde(default)]
    language: String,
    checked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    parsing_error_message: String,
    #[serde(default)]
    parsing_error_count: u32,
}

#[derive(Debug, Deserialize)]
struct MinifluxEntries {
    entries: Vec<MinifluxEntry>,
}

#[derive(Debug, Deserialize)]
struct MinifluxEntry {
    title: String,
    #[serde(default)]
    content: String,
    published_at: Option<DateTime<Utc>>,
}

impl From<MinifluxEntry> for EntrySample {
    fn from(e: MinifluxEntry) -> Self {
        Self {
            title: e.title,
            content: e.content,
            published_at: e.published_at,
        }
    }
}

/// Cuerpo de `PUT /v1/feeds/{id}`: Miniflux solo cambia lo que llega.
#[derive(Debug, Serialize)]
struct MinifluxFeedModification {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    crawler: Option<bool>,
}

impl Miniflux {
    async fn discover(&self, url: &str) -> AppResult<Vec<MinifluxSubscription>> {
        #[derive(Serialize)]
        struct Body<'a> {
            url: &'a str,
        }
        self.send_json(Method::POST, "/v1/discover", &Body { url })
            .await
    }

    async fn feeds(&self) -> AppResult<Vec<MinifluxFeed>> {
        self.get("/v1/feeds").await
    }

    async fn feed(&self, id: Id) -> AppResult<MinifluxFeed> {
        self.get(&format!("/v1/feeds/{id}")).await
    }

    async fn create_feed(&self, feed_url: &str, category_id: Id, crawler: bool) -> AppResult<Id> {
        #[derive(Serialize)]
        struct Body<'a> {
            feed_url: &'a str,
            category_id: Id,
            crawler: bool,
        }
        #[derive(Deserialize)]
        struct Created {
            feed_id: Id,
        }
        let created: Created = self
            .send_json(
                Method::POST,
                "/v1/feeds",
                &Body {
                    feed_url,
                    category_id,
                    crawler,
                },
            )
            .await?;
        Ok(created.feed_id)
    }

    async fn update_feed(
        &self,
        id: Id,
        body: &MinifluxFeedModification,
    ) -> AppResult<MinifluxFeed> {
        self.send_json(Method::PUT, &format!("/v1/feeds/{id}"), body)
            .await
    }

    async fn delete(&self, path: &str) -> AppResult<()> {
        Self::send_empty(self.request(Method::DELETE, path))
            .await
            .map(drop)
    }

    /// Sus últimos artículos en Miniflux, del más nuevo al más viejo.
    async fn recent_entries(&self, feed_id: Id) -> AppResult<Vec<MinifluxEntry>> {
        let path =
            format!("/v1/feeds/{feed_id}/entries?limit={SAMPLE}&order=published_at&direction=desc");
        Ok(self.get::<MinifluxEntries>(&path).await?.entries)
    }

    /// El feed de la API con su idioma y artículos al día. Si no se pueden
    /// leer sus artículos, lo devuelve sin esos datos.
    /// El feed con sus estadísticas. Si la caché tiene unas recientes, no lee
    /// los artículos otra vez. Un fallo no se guarda: se reintenta en la
    /// siguiente carga.
    async fn with_stats(&self, feed: MinifluxFeed, cache: &StatsCache) -> Feed {
        if let Some(stats) = cache.get(feed.id) {
            return feed_from_miniflux(feed, stats);
        }
        let stats = match self.recent_entries(feed.id).await {
            Ok(entries) => {
                let samples: Vec<EntrySample> = entries.into_iter().map(Into::into).collect();
                let declared = Some(feed.language.as_str()).filter(|l| !l.is_empty());
                let stats = FeedStats::compute(&samples, declared, Utc::now());
                cache.put(feed.id, stats.clone());
                stats
            }
            Err(e) => {
                tracing::warn!(
                    feed_id = feed.id,
                    "no se pudieron leer sus artículos: {}",
                    e.message
                );
                FeedStats::default()
            }
        };
        feed_from_miniflux(feed, stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_https_when_missing() {
        assert_eq!(
            normalize_url(" theverge.com ").unwrap(),
            "https://theverge.com"
        );
        assert_eq!(
            normalize_url("http://example.com/rss").unwrap(),
            "http://example.com/rss"
        );
        assert!(normalize_url("").is_err());
        assert!(normalize_url("ftp://example.com").is_err());
    }

    #[test]
    fn rejects_empty_titles() {
        assert_eq!(non_empty_title("  Tecnología ").unwrap(), "Tecnología");
        assert!(non_empty_title("   ").is_err());
    }

    /// Respuesta real de Miniflux 2.3.4, recortada.
    const MINIFLUX_FEED: &str = r#"{
        "id": 1, "user_id": 1,
        "feed_url": "http://www.aaronsw.com/2002/feeds/pgessays.rss",
        "site_url": "http://www.paulgraham.com/",
        "title": "Paul Graham: Essays",
        "language": "",
        "checked_at": "2026-10-23T17:39:36.4477Z",
        "parsing_error_message": "", "parsing_error_count": 0,
        "crawler": true,
        "category": {"id": 1, "title": "All", "user_id": 1, "hide_globally": false},
        "icon": {"feed_id": 1, "icon_id": 1}
    }"#;

    #[test]
    fn converts_miniflux_feed() {
        let mf: MinifluxFeed = serde_json::from_str(MINIFLUX_FEED).unwrap();
        let stats = FeedStats {
            lang: Some("en".into()),
            articles_per_day: Some(0.2),
        };
        let feed = feed_from_miniflux(mf, stats);
        assert_eq!(feed.id, 1);
        assert_eq!(feed.title, "Paul Graham: Essays");
        assert_eq!(
            feed.category,
            Category {
                id: 1,
                title: "All".into()
            }
        );
        assert!(feed.crawler);
        assert_eq!(feed.lang.as_deref(), Some("en"));
        assert_eq!(feed.articles_per_day, Some(0.2));
        assert_eq!(
            feed.checked_at,
            Some("2026-10-23T17:39:36.4477Z".parse().unwrap())
        );
        assert_eq!(feed.error, None);
    }

    #[test]
    fn reports_feed_errors_and_unchecked_feeds() {
        let json = MINIFLUX_FEED
            .replace(
                r#""parsing_error_message": """#,
                r#""parsing_error_message": "404 Not Found""#,
            )
            .replace(r#""parsing_error_count": 0"#, r#""parsing_error_count": 3"#)
            .replace("2026-10-23T17:39:36.4477Z", "0001-01-01T00:00:00Z");
        let mf: MinifluxFeed = serde_json::from_str(&json).unwrap();
        let feed = feed_from_miniflux(mf, FeedStats::default());
        assert_eq!(
            feed.error,
            Some(FeedError {
                message: "404 Not Found".into(),
                count: 3
            })
        );
        assert_eq!(feed.checked_at, None);
    }

    #[test]
    fn marks_only_the_papers_feed() {
        let mf: MinifluxFeed = serde_json::from_str(MINIFLUX_FEED).unwrap();
        assert!(!feed_from_miniflux(mf, FeedStats::default()).papers);
        let json =
            MINIFLUX_FEED.replace("http://www.aaronsw.com/2002/feeds/pgessays.rss", FEED_URL);
        let mf: MinifluxFeed = serde_json::from_str(&json).unwrap();
        assert!(feed_from_miniflux(mf, FeedStats::default()).papers);
    }

    #[test]
    fn feed_update_sends_only_given_fields() {
        let body = MinifluxFeedModification {
            title: None,
            category_id: Some(4),
            crawler: Some(false),
        };
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            serde_json::json!({"category_id": 4, "crawler": false})
        );
    }

    #[test]
    fn analyzes_a_downloaded_feed() {
        let rss = r#"<?xml version="1.0"?>
            <rss version="2.0"><channel>
              <title>Ejemplo</title><language>en-us</language>
              <item><title>First post about computers</title>
                <description>This is a long enough English sentence about the new computers that shipped today.</description>
                <pubDate>Thu, 22 Oct 2026 12:00:00 GMT</pubDate></item>
              <item><title>Second post about phones</title>
                <description>Another English paragraph, this time about phones and the people who use them.</description>
                <pubDate>Wed, 21 Oct 2026 12:00:00 GMT</pubDate></item>
              <item><title>Third post about the weather</title>
                <description>It rained all day and the streets were flooded, which nobody expected.</description>
                <pubDate>Tue, 20 Oct 2026 12:00:00 GMT</pubDate></item>
            </channel></rss>"#;
        let parsed = feed_rs::parser::parse(rss.as_bytes()).unwrap();
        let sub = MinifluxSubscription {
            title: String::new(),
            url: "https://example.com/rss".into(),
        };
        let now = "2026-10-23T12:00:00Z".parse().unwrap();
        let found = discovered_feed(sub, Some(&parsed), now);
        assert_eq!(found.title, "Ejemplo");
        assert_eq!(found.lang.as_deref(), Some("en"));
        // Tres artículos: dos intervalos en tres días.
        assert_eq!(found.articles_per_day, Some(0.7));
    }

    #[test]
    fn discovered_feed_without_download() {
        let sub = MinifluxSubscription {
            title: "The Verge".into(),
            url: "https://theverge.com/rss/index.xml".into(),
        };
        let found = discovered_feed(sub, None, Utc::now());
        assert_eq!(found.title, "The Verge");
        assert_eq!(found.lang, None);
        assert_eq!(found.articles_per_day, None);
    }
}
