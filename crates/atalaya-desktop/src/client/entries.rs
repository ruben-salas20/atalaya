//! Rutas de lectura: menú lateral, lista, abrir un artículo y marcarlo.

use atalaya_api::{Entry, EntryPage, EntryQuery, EntrySummary, EntryUpdate, Id, Sidebar};
use reqwest::Method;

use super::{ApiClient, Result};

impl ApiClient {
    /// `GET /api/sidebar`.
    pub fn sidebar(&self) -> Result<Sidebar> {
        self.get("/api/sidebar")
    }

    /// `GET /api/entries`.
    pub fn entries(&self, query: &EntryQuery) -> Result<EntryPage> {
        self.get_query("/api/entries", query)
    }

    /// `GET /api/entries/{id}`. Pone en marcha la traducción si hace falta.
    pub fn entry(&self, id: Id) -> Result<Entry> {
        self.get(&format!("/api/entries/{id}"))
    }

    /// `PATCH /api/entries/{id}`: leído o guardado.
    pub fn update_entry(&self, id: Id, update: &EntryUpdate) -> Result<EntrySummary> {
        self.send_json(Method::PATCH, &format!("/api/entries/{id}"), update)
    }
}
