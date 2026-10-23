//! Rutas de lectura: menú lateral y lista. El paso 5.2 añade aquí abrir un
//! artículo (`GET /api/entries/{id}`) y marcarlo (`PATCH`).

use atalaya_api::{EntryPage, EntryQuery, Sidebar};

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
}
