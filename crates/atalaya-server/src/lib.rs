pub mod auth;
pub mod config;
pub mod content;
pub mod entries;
pub mod error;
pub mod feed_stats;
pub mod feeds;
pub mod lang;
pub mod llama;
pub mod miniflux;
pub mod papers;
pub mod sidebar;
pub mod stats_cache;
pub mod store;
pub mod titles;
pub mod translations;
pub mod webhook;

use atalaya_api::{Health, Status};
use axum::{Json, Router, extract::State, middleware, routing::get};

use crate::{
    config::Config, llama::Llama, miniflux::Miniflux, papers::Papers, stats_cache::StatsCache,
    store::Store, translations::Translations,
};

/// Lo que comparten todos los handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub http: reqwest::Client,
    pub miniflux: Miniflux,
    pub translations: Translations,
    pub papers: Papers,
    pub stats: StatsCache,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Atalaya/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("no se pudo crear el cliente HTTP");
        let miniflux = Miniflux::new(http.clone(), &config.miniflux_url, &config.miniflux_api_key);
        let store = Store::open(&config.data_dir).unwrap_or_else(|e| panic!("{e}"));
        let llama = Llama::new(http.clone(), &config.llama_url, &config.llama_model);
        let translations = Translations::new(store, llama, miniflux.clone());
        let papers = Papers::new(http.clone(), translations.clone());
        Self {
            config,
            http,
            miniflux,
            translations,
            papers,
            stats: StatsCache::default(),
        }
    }
}

/// Todas las rutas. Cada paso añade las suyas a `api` con `.merge(...)`,
/// antes de `.layer(...)`: la capa solo protege lo que ya está dentro.
pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/status", get(status))
        .merge(sidebar::routes())
        .merge(entries::routes())
        .merge(feeds::routes())
        .merge(papers::routes())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_api_key,
        ));

    Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .merge(webhook::routes())
        .route("/formulas/{arxiv_id}/{file}", get(papers::formula_svg))
        .fallback(error::not_found)
        .method_not_allowed_fallback(error::method_not_allowed)
        .with_state(state)
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

async fn status(State(state): State<AppState>) -> Json<Status> {
    #[derive(serde::Deserialize)]
    struct Me {}
    let miniflux = state.miniflux.get::<Me>("/v1/me").await.is_ok();
    Json(Status {
        version: env!("CARGO_PKG_VERSION").into(),
        miniflux,
    })
}
