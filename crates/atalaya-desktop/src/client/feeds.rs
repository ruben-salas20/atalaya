//! Rutas de feeds y carpetas (ADR 0007, F1–F6). El servicio las reenvía a
//! Miniflux.

use atalaya_api::{
    Category, CategoryInput, DiscoverRequest, DiscoveredFeed, Feed, FeedUpdate, Id, NewFeed,
};
use reqwest::Method;

use super::{ApiClient, Result};

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

    /// `POST /api/feeds`.
    pub fn add_feed(&self, feed: &NewFeed) -> Result<Feed> {
        self.send_json(Method::POST, "/api/feeds", feed)
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
