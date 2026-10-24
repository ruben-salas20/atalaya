//! Rutas de lectura: menú lateral, lista, abrir un artículo y marcarlo.

//!
//! El menú, las listas y los artículos se guardan en disco al llegar y, sin
//! red, salen de ahí (L8).

use std::sync::atomic::Ordering;

use atalaya_api::{
    Entry, EntryPage, EntryQuery, EntrySummary, EntryUpdate, Id, MarkAllReadQuery, Sidebar,
    TranslationState, View, fold,
};
use reqwest::Method;

use super::{ApiClient, Result, check, read_json};

fn entry_key(id: Id) -> String {
    format!("entries/{id}")
}

/// Nombre en disco de una página de lista. Las búsquedas no se guardan.
fn list_key(query: &EntryQuery) -> String {
    format!(
        "list-{:?}-{}-{}-{}-{}",
        query.view,
        query.feed_id.unwrap_or(0),
        query.category_id.unwrap_or(0),
        query.offset.unwrap_or(0),
        query.limit.unwrap_or(0),
    )
    .to_lowercase()
}

/// El texto de búsqueda, sin espacios y plegado, o `None` si no hay.
fn search_text(query: &EntryQuery) -> Option<String> {
    let needle = fold(query.q.as_deref()?.trim());
    (!needle.is_empty()).then_some(needle)
}

impl ApiClient {
    /// `GET /api/sidebar`.
    pub fn sidebar(&self) -> Result<Sidebar> {
        self.get_cached("sidebar", "/api/sidebar", &(), |_| true)
    }

    /// `GET /api/entries`. Con `q`, busca en el servicio; sin red, filtra
    /// las listas guardadas de esta misma vista.
    pub fn entries(&self, query: &EntryQuery) -> Result<EntryPage> {
        let Some(needle) = search_text(query) else {
            return self.get_cached(&list_key(query), "/api/entries", query, |_| true);
        };
        match read_json(
            self.request(Method::GET, "/api/entries")
                .query(query)
                .send(),
        ) {
            Ok(page) => {
                self.offline.store(false, Ordering::Relaxed);
                Ok(page)
            }
            Err(failure) if failure.network => {
                self.cached_search(query, &needle).ok_or(failure.message)
            }
            Err(failure) => Err(failure.message),
        }
    }

    /// Busca `needle` en las páginas guardadas de la vista de `query`, como
    /// hace el servicio. `None` si no hay ninguna página guardada.
    fn cached_search(&self, query: &EntryQuery, needle: &str) -> Option<EntryPage> {
        let cache = self.cache.as_ref()?;
        let mut found: Vec<EntrySummary> = Vec::new();
        let mut offset = 0;
        let mut any = false;
        while let Some(page) = cache.load::<EntryPage>(&list_key(&EntryQuery {
            offset: Some(offset),
            ..query.clone()
        })) {
            any = true;
            if page.entries.is_empty() {
                break;
            }
            offset += page.entries.len() as u32;
            found.extend(page.entries.into_iter().filter(|e| {
                fold(&e.title).contains(needle) || fold(&e.original_title).contains(needle)
            }));
        }
        if !any {
            return None;
        }
        let total = found.len() as u64;
        let start = (query.offset.unwrap_or(0) as usize).min(found.len());
        let limit = query.limit.unwrap_or(50).clamp(1, 200) as usize;
        let entries = found.into_iter().skip(start).take(limit).collect();
        self.offline.store(true, Ordering::Relaxed);
        Some(EntryPage { total, entries })
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

    /// `POST /api/entries/mark-all-as-read` con el alcance en la URL: un feed,
    /// una carpeta o, sin nada, todos. Responde 204 sin cuerpo.
    pub fn mark_all_read(&self, scope: &MarkAllReadQuery) -> Result<()> {
        check(
            self.request(Method::POST, "/api/entries/mark-all-as-read")
                .query(scope)
                .send(),
        )?;
        Ok(())
    }
}
