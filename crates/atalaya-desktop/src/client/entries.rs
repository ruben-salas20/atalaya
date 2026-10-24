//! Rutas de lectura: menú lateral, lista, abrir un artículo y marcarlo.

//!
//! El menú, las listas y los artículos se guardan en disco al llegar y, sin
//! red, salen de ahí (L8).

use std::sync::atomic::Ordering;

use atalaya_api::{
    Entry, EntryPage, EntryQuery, EntrySummary, EntryUpdate, Id, Sidebar, TranslationState, View,
};
use reqwest::Method;

use super::{ApiClient, Result};

fn entry_key(id: Id) -> String {
    format!("entries/{id}")
}

impl ApiClient {
    /// `GET /api/sidebar`.
    pub fn sidebar(&self) -> Result<Sidebar> {
        self.get_cached("sidebar", "/api/sidebar", &(), |_| true)
    }

    /// `GET /api/entries`.
    pub fn entries(&self, query: &EntryQuery) -> Result<EntryPage> {
        let key = format!(
            "list-{:?}-{}-{}-{}-{}",
            query.view,
            query.feed_id.unwrap_or(0),
            query.category_id.unwrap_or(0),
            query.offset.unwrap_or(0),
            query.limit.unwrap_or(0),
        )
        .to_lowercase();
        self.get_cached(&key, "/api/entries", query, |_| true)
    }

    /// `GET /api/entries/{id}`. Pone en marcha la traducción si hace falta.
    pub fn entry(&self, id: Id) -> Result<Entry> {
        // A medio traducir no se guarda: sin red se vería en inglés.
        self.get_cached(
            &entry_key(id),
            &format!("/api/entries/{id}"),
            &(),
            |e: &Entry| e.translation != TranslationState::Pending,
        )
    }

    /// Baja a disco el texto de los guardados que aún no están, como hace
    /// el móvil, y borra los artículos viejos que ya no hacen falta. Pedir
    /// un guardado sin traducir pone en marcha su traducción; se guarda la
    /// próxima vez que se llame, ya traducido.
    pub fn cache_starred(&self) {
        let Some(cache) = &self.cache else {
            return;
        };
        if self.caching_starred.swap(true, Ordering::Relaxed) {
            return;
        }
        // La misma consulta que la lista de Guardados: así su primera página
        // también queda en disco aunque no se haya abierto en el PC.
        let query = EntryQuery {
            view: View::Starred,
            offset: Some(0),
            ..Default::default()
        };
        if let Some(page) = self.entries(&query).ok().filter(|_| !self.is_offline()) {
            let keys: Vec<String> = page.entries.iter().map(|e| entry_key(e.id)).collect();
            for (entry, key) in page.entries.iter().zip(&keys) {
                if !cache.contains(key) && self.entry(entry.id).is_err() {
                    break;
                }
            }
            cache.prune_entries(&keys);
        }
        self.caching_starred.store(false, Ordering::Relaxed);
    }

    /// `PATCH /api/entries/{id}`: leído o guardado.
    pub fn update_entry(&self, id: Id, update: &EntryUpdate) -> Result<EntrySummary> {
        self.send_json(Method::PATCH, &format!("/api/entries/{id}"), update)
    }
}
