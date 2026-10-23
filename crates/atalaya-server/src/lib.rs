pub mod auth;
pub mod config;
pub mod entries;
pub mod error;
pub mod miniflux;
pub mod sidebar;
pub mod translations;

use atalaya_api::{Health, Status};
use axum::{Json, Router, extract::State, middleware, routing::get};

use crate::{config::Config, miniflux::Miniflux, translations::Translations};

/// Lo que comparten todos los handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub http: reqwest::Client,
    pub miniflux: Miniflux,
    pub translations: Translations,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Atalaya/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("no se pudo crear el cliente HTTP");
        let miniflux = Miniflux::new(http.clone(), &config.miniflux_url, &config.miniflux_api_key);
        Self {
            config,
            http,
            miniflux,
            translations: Translations::new(),
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
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_api_key,
        ));

    Router::new()
        .route("/health", get(health))
        .nest("/api", api)
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
