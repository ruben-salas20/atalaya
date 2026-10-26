//! Rutas de lectura: menú lateral, lista, abrir un artículo y marcarlo.
//!
//! El menú, las listas y los artículos se guardan en disco al llegar y, sin
//! red, salen de ahí (L8). Encima de esa copia se aplican los cambios que
//! esperan en la cola, para que la lista y las cifras muestren lo último
//! que pidió el usuario.

use std::collections::{HashMap, HashSet};
use std::sync::PoisonError;
use std::sync::atomic::Ordering;

use atalaya_api::{
    Entry, EntryPage, EntryQuery, EntrySummary, EntryUpdate, Id, MarkAllReadQuery, Sidebar,
    TranslationState, View, fold,
};
use reqwest::Method;

use super::{ApiClient, Result, check, read_json};

pub(super) fn entry_key(id: Id) -> String {
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

/// El id de arXiv sin versión de una dirección `arxiv.org/abs|pdf|html/…`,
/// como `arxivIdFromUrl` de Android. `None` si no es de arXiv.
pub fn arxiv_id_from_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let rest = rest
        .strip_prefix("www.")
        .or_else(|| rest.strip_prefix("export."))
        .unwrap_or(rest);
    let rest = rest.strip_prefix("arxiv.org/")?;
    let rest = ["abs/", "pdf/", "html/"]
        .iter()
        .find_map(|kind| rest.strip_prefix(kind))?;
    let path = &rest[..rest.find(['?', '#']).unwrap_or(rest.len())];
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".pdf").unwrap_or(path);
    let id = without_version(path);
    (is_new_id(id) || is_old_id(id)).then(|| id.to_owned())
}

/// Quita la versión final (`v2`), si la hay.
fn without_version(id: &str) -> &str {
    match id.rfind('v') {
        Some(at) if at + 1 < id.len() && id[at + 1..].bytes().all(|b| b.is_ascii_digit()) => {
            &id[..at]
        }
        _ => id,
    }
}

/// `2610.08812`: año y mes de cuatro cifras, y un número de cuatro o cinco.
fn is_new_id(id: &str) -> bool {
    let Some((yymm, number)) = id.split_once('.') else {
        return false;
    };
    yymm.len() == 4
        && yymm.bytes().all(|b| b.is_ascii_digit())
        && (4..=5).contains(&number.len())
        && number.bytes().all(|b| b.is_ascii_digit())
}

/// `hep-th/9901001` o `math.AG/0601001`: archivo, clase opcional y siete cifras.
fn is_old_id(id: &str) -> bool {
    let Some((archive, number)) = id.split_once('/') else {
        return false;
    };
    let (name, class) = match archive.split_once('.') {
        Some((name, class)) => (name, Some(class)),
        None => (archive, None),
    };
    let name_ok = !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
    let class_ok = class.is_none_or(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase()));
    name_ok && class_ok && number.len() == 7 && number.bytes().all(|b| b.is_ascii_digit())
}

/// El texto de búsqueda, sin espacios y plegado, o `None` si no hay.
fn search_text(query: &EntryQuery) -> Option<String> {
    let needle = fold(query.q.as_deref()?.trim());
    (!needle.is_empty()).then_some(needle)
}

impl ApiClient {
    /// `GET /api/sidebar`. Sin red, suma o resta los cambios de la cola.
    pub fn sidebar(&self) -> Result<Sidebar> {
        self.get_cached(
            "sidebar",
            "/api/sidebar",
            &(),
            |_| true,
            |sidebar| self.with_queued_counts(sidebar),
        )
    }

    /// `GET /api/entries`. Con `q`, busca en el servicio; sin red, busca en
    /// la copia. Sin red, la lista se arma con los cambios de la cola.
    pub fn entries(&self, query: &EntryQuery) -> Result<EntryPage> {
        let Some(needle) = search_text(query) else {
            return self.get_cached(
                &list_key(query),
                "/api/entries",
                query,
                |_| true,
                |page| self.with_queued_page(query, page),
            );
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

    /// Busca `needle` en la copia de la misma carpeta, como hace el
    /// servicio, con los cambios de la cola aplicados. `None` si la vista no
    /// tiene ninguna página guardada.
    fn cached_search(&self, query: &EntryQuery, needle: &str) -> Option<EntryPage> {
        let cache = self.cache.as_ref()?;
        cache.load::<EntryPage>(&list_key(&EntryQuery {
            offset: Some(0),
            q: None,
            ..query.clone()
        }))?;
        let queue = self.queue();
        let mut found: Vec<EntrySummary> = self
            .copied_entries(query, Vec::new())
            .iter()
            .map(|entry| applied_by(entry, queue.get(&entry.id)))
            .filter(|e| {
                in_view(query.view, e)
                    && (fold(&e.title).contains(needle) || fold(&e.original_title).contains(needle))
            })
            .collect();
        newest_first(&mut found);
        let total = found.len() as u64;
        self.offline.store(true, Ordering::Relaxed);
        Some(page_of(found, total, query))
    }

    /// `GET /api/entries/{id}`. Pone en marcha la traducción si hace falta.
    pub fn entry(&self, id: Id) -> Result<Entry> {
        // A medio traducir no se guarda: sin red se vería en inglés.
        self.get_cached(
            &entry_key(id),
            &format!("/api/entries/{id}"),
            &(),
            |e: &Entry| e.translation != TranslationState::Pending,
            |mut entry: Entry| {
                let queue = self.queue();
                entry.summary = applied_by(&entry.summary, queue.get(&id));
                // Una copia de antes de #90 no trae el id de arXiv: sale de la dirección.
                entry.arxiv_id = entry
                    .arxiv_id
                    .or_else(|| arxiv_id_from_url(&entry.summary.url));
                entry
            },
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

    /// Anota en la cola un cambio de leído o guardado. Va antes de
    /// [`ApiClient::update_entry`], en el hilo de la interfaz y en el mismo
    /// orden en que el usuario pide los cambios: así la cola los funde en
    /// ese orden, y el envío manda siempre lo último pedido. Sin copia en
    /// disco no hace nada.
    pub fn queue_change(&self, current: &EntrySummary, update: &EntryUpdate) {
        if let Some(pending) = &self.pending {
            pending.merge(current, update);
        }
    }

    /// Manda a `PATCH /api/entries/{id}` lo último pedido para el artículo,
    /// que ya está anotado con [`ApiClient::queue_change`]. Sin red, el
    /// cambio se queda en la cola y se responde con `current` ya cambiado;
    /// se manda en la próxima lectura con red. Un error del servicio sí
    /// devuelve `Err`. Si el artículo ya no está en cola, otra tarea lo
    /// mandó y aquí no se manda nada.
    pub fn update_entry(
        &self,
        current: &EntrySummary,
        update: &EntryUpdate,
    ) -> Result<EntrySummary> {
        let id = current.id;
        // Mientras se envía, nadie más manda la cola: así no se cruzan.
        let _turn = self.sending.lock().unwrap_or_else(PoisonError::into_inner);
        let sent = match &self.pending {
            Some(pending) => match pending.latest(id) {
                Some(sent) => sent,
                None => return Ok(applied(current, update)),
            },
            None => update.clone(),
        };
        let result = self.try_send_json(Method::PATCH, &format!("/api/entries/{id}"), &sent);
        match result {
            Ok(summary) => {
                self.offline.store(false, Ordering::Relaxed);
                self.forget(id, &sent);
                Ok(summary)
            }
            Err(failure) if failure.retry && self.pending.is_some() => {
                if failure.network {
                    self.offline.store(true, Ordering::Relaxed);
                }
                Ok(applied(current, &sent))
            }
            Err(failure) => {
                self.forget(id, &sent);
                Err(failure.into())
            }
        }
    }

    fn forget(&self, id: Id, sent: &EntryUpdate) {
        if let Some(pending) = &self.pending {
            pending.forget(id, sent);
        }
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

    /// Los cambios que esperan a la red, por id de artículo.
    fn queue(&self) -> HashMap<Id, EntryUpdate> {
        self.pending
            .as_ref()
            .map(|pending| {
                pending
                    .all()
                    .into_iter()
                    .map(|(id, queued)| (id, queued.update))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// La página guardada con los cambios de la cola. Sin cambios, sale tal
    /// cual. Con cambios, la vista se arma con los artículos de la copia de
    /// la misma carpeta: se aplica la cola, se filtra por la vista y se
    /// ordena del más nuevo al más viejo. El total suma o resta lo que los
    /// cambios mueven dentro de la vista.
    fn with_queued_page(&self, query: &EntryQuery, exact: EntryPage) -> EntryPage {
        let queue = self.queue();
        if queue.is_empty() {
            return exact;
        }
        let mut delta: i64 = 0;
        let mut shown = Vec::new();
        for entry in self.copied_entries(query, exact.entries) {
            let now = applied_by(&entry, queue.get(&entry.id));
            delta += i64::from(in_view(query.view, &now)) - i64::from(in_view(query.view, &entry));
            if in_view(query.view, &now) {
                shown.push(now);
            }
        }
        newest_first(&mut shown);
        let total = shift(exact.total, delta);
        page_of(shown, total, query)
    }

    /// El menú con los cambios de la cola: suma o resta los sin leer y los
    /// guardados. Solo cuentan los cambios que guardaron cómo estaba el
    /// artículo.
    fn with_queued_counts(&self, mut sidebar: Sidebar) -> Sidebar {
        let Some(pending) = &self.pending else {
            return sidebar;
        };
        for (_, queued) in pending.all() {
            let Some(before) = queued.before else {
                continue;
            };
            let read = queued.update.read.unwrap_or(before.read);
            let starred = queued.update.starred.unwrap_or(before.starred);
            let unread = i64::from(!read) - i64::from(!before.read);
            if unread != 0 {
                sidebar.unread = shift(sidebar.unread, unread);
                for category in &mut sidebar.categories {
                    for feed in category.feeds.iter_mut().filter(|f| f.id == before.feed_id) {
                        feed.unread = shift(feed.unread, unread);
                        category.unread = shift(category.unread, unread);
                    }
                }
            }
            if starred != before.starred {
                let delta = i64::from(starred) - i64::from(before.starred);
                sidebar.starred = shift(sidebar.starred, delta);
            }
        }
        sidebar
    }

    /// Los artículos de la copia de la misma carpeta (feed o categoría, y
    /// tamaño de página) que `query`, de las tres vistas, sin repetir. `seed`
    /// va primero. Cada vista se lee desde la primera página hasta la
    /// primera que falte.
    fn copied_entries(&self, query: &EntryQuery, seed: Vec<EntrySummary>) -> Vec<EntrySummary> {
        let mut pages = vec![seed];
        if let Some(cache) = &self.cache {
            for view in [View::All, View::Unread, View::Starred] {
                let mut offset = 0;
                while let Some(page) = cache.load::<EntryPage>(&list_key(&EntryQuery {
                    view,
                    offset: Some(offset),
                    q: None,
                    ..query.clone()
                })) {
                    if page.entries.is_empty() {
                        break;
                    }
                    offset += page.entries.len() as u32;
                    pages.push(page.entries);
                }
            }
        }
        let mut seen = HashSet::new();
        pages
            .into_iter()
            .flatten()
            .filter(|entry| seen.insert(entry.id))
            .collect()
    }
}

/// `summary` con los cambios de `update`.
fn applied(summary: &EntrySummary, update: &EntryUpdate) -> EntrySummary {
    let mut next = summary.clone();
    if let Some(read) = update.read {
        next.read = read;
    }
    if let Some(starred) = update.starred {
        next.starred = starred;
    }
    next
}

/// `summary` con los cambios de la cola, si los hay.
fn applied_by(summary: &EntrySummary, queued: Option<&EntryUpdate>) -> EntrySummary {
    match queued {
        Some(update) => applied(summary, update),
        None => summary.clone(),
    }
}

/// Si el artículo sale en la vista (`Todos`, `Sin leer` o `Guardados`).
fn in_view(view: View, entry: &EntrySummary) -> bool {
    match view {
        View::All => true,
        View::Unread => !entry.read,
        View::Starred => entry.starred,
    }
}

/// Ordena del más nuevo al más viejo, como el servicio.
fn newest_first(entries: &mut [EntrySummary]) {
    entries.sort_by(|a, b| {
        b.published_at
            .cmp(&a.published_at)
            .then_with(|| b.id.cmp(&a.id))
    });
}

/// La página de `query` dentro de `found`, ya ordenada.
fn page_of(found: Vec<EntrySummary>, total: u64, query: &EntryQuery) -> EntryPage {
    let start = (query.offset.unwrap_or(0) as usize).min(found.len());
    let limit = query.limit.unwrap_or(50).clamp(1, 200) as usize;
    EntryPage {
        total,
        entries: found.into_iter().skip(start).take(limit).collect(),
    }
}

/// `n` más `delta`, sin bajar de cero.
fn shift(n: u64, delta: i64) -> u64 {
    (n as i64 + delta).max(0) as u64
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use atalaya_api::{EntryUpdate, Sidebar, SidebarCategory, SidebarFeed};

    use super::*;
    use crate::client::pending::Pending;
    use crate::client::pending::tests::{TempDir, article, updates};

    fn summary(id: Id) -> EntrySummary {
        article(id, 1, false, false)
    }

    /// Cliente cuyo servicio no escucha: cada petición falla por red.
    fn offline_client(dir: &TempDir) -> ApiClient {
        // Se reserva un puerto y se suelta: nadie escucha en él.
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("no se pudo reservar un puerto")
            .port();
        client_at(dir, port)
    }

    fn client_at(dir: &TempDir, port: u16) -> ApiClient {
        let cache = dir.cache();
        ApiClient {
            http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("cliente HTTP de prueba"),
            base_url: format!("http://127.0.0.1:{port}"),
            api_key: String::new(),
            pending: Some(Pending::new(cache.clone())),
            sending: Mutex::new(()),
            cache: Some(cache),
            offline: AtomicBool::new(false),
            caching_starred: AtomicBool::new(false),
        }
    }

    /// Guarda en la copia una página de la vista de `query`.
    fn seed_page(dir: &TempDir, query: &EntryQuery, entries: Vec<EntrySummary>, total: u64) {
        dir.cache()
            .store(&list_key(query), &EntryPage { total, entries });
    }

    /// Anota en la cola, como lo hace la interfaz antes de mandar.
    fn queue(dir: &TempDir, current: &EntrySummary, update: EntryUpdate) {
        Pending::new(dir.cache()).merge(current, &update);
    }

    fn read(value: bool) -> EntryUpdate {
        EntryUpdate {
            read: Some(value),
            starred: None,
        }
    }

    fn starred(value: bool) -> EntryUpdate {
        EntryUpdate {
            read: None,
            starred: Some(value),
        }
    }

    fn ids(page: &EntryPage) -> Vec<Id> {
        page.entries.iter().map(|e| e.id).collect()
    }

    #[test]
    fn offline_change_is_queued_and_applied() {
        let dir = TempDir::new();
        let client = offline_client(&dir);
        let update = read(true);

        client.queue_change(&summary(7), &update);
        let result = client
            .update_entry(&summary(7), &update)
            .expect("sin red debe responder Ok");

        assert!(result.read);
        assert!(client.is_offline());
        assert_eq!(updates(&Pending::new(dir.cache())), vec![(7, update)]);
    }

    #[test]
    fn offline_send_keeps_the_queue() {
        let dir = TempDir::new();
        let client = offline_client(&dir);
        let update = starred(true);
        queue(&dir, &summary(4), starred(true));

        client.send_pending();

        assert_eq!(updates(&Pending::new(dir.cache())), vec![(4, update)]);
    }

    #[test]
    fn offline_unread_list_hides_what_was_read_offline() {
        let dir = TempDir::new();
        let unread = EntryQuery {
            view: View::Unread,
            ..Default::default()
        };
        seed_page(&dir, &unread, vec![summary(2), summary(1)], 2);
        queue(&dir, &summary(1), read(true));

        let page = offline_client(&dir)
            .entries(&unread)
            .expect("sin red debe salir de la copia");

        assert_eq!(page.total, 1);
        assert_eq!(ids(&page), vec![2]);
    }

    #[test]
    fn offline_all_list_shows_the_queued_state_and_keeps_the_rows() {
        let dir = TempDir::new();
        seed_page(
            &dir,
            &EntryQuery::default(),
            vec![summary(2), summary(1)],
            2,
        );
        queue(&dir, &summary(1), read(true));
        queue(&dir, &summary(2), starred(true));

        let page = offline_client(&dir)
            .entries(&EntryQuery::default())
            .expect("sin red debe salir de la copia");

        assert_eq!(page.total, 2);
        assert_eq!(ids(&page), vec![2, 1]);
        assert!(page.entries[0].starred && !page.entries[0].read);
        assert!(page.entries[1].read);
    }

    #[test]
    fn offline_starred_list_gets_queued_stars_from_the_copy() {
        let dir = TempDir::new();
        let starred_view = EntryQuery {
            view: View::Starred,
            ..Default::default()
        };
        seed_page(&dir, &starred_view, vec![article(3, 1, false, true)], 1);
        seed_page(
            &dir,
            &EntryQuery::default(),
            vec![article(3, 1, false, true), summary(2), summary(1)],
            3,
        );
        queue(&dir, &summary(1), starred(true));

        let page = offline_client(&dir)
            .entries(&starred_view)
            .expect("sin red debe salir de la copia");

        assert_eq!(page.total, 2);
        assert_eq!(ids(&page), vec![3, 1]);
    }

    #[test]
    fn queued_changes_survive_a_restart() {
        let dir = TempDir::new();
        let starred_view = EntryQuery {
            view: View::Starred,
            ..Default::default()
        };
        seed_page(&dir, &starred_view, vec![article(3, 1, false, true)], 1);
        seed_page(
            &dir,
            &EntryQuery::default(),
            vec![article(3, 1, false, true), summary(1)],
            2,
        );
        queue(&dir, &summary(1), starred(true));

        let before_restart = offline_client(&dir)
            .entries(&starred_view)
            .expect("sin red debe salir de la copia");
        // Una app nueva, sin nada en memoria, lee la misma copia y la cola.
        let after_restart = offline_client(&dir)
            .entries(&starred_view)
            .expect("tras reiniciar, sin red, también");

        assert_eq!(before_restart, after_restart);
        assert_eq!(ids(&after_restart), vec![3, 1]);
    }

    #[test]
    fn offline_sidebar_counts_include_the_queue() {
        let dir = TempDir::new();
        let copy = Sidebar {
            total: 10,
            unread: 4,
            starred: 2,
            categories: vec![SidebarCategory {
                id: 1,
                title: "Noticias".into(),
                unread: 4,
                feeds: vec![
                    SidebarFeed {
                        id: 5,
                        title: "A".into(),
                        unread: 3,
                    },
                    SidebarFeed {
                        id: 6,
                        title: "B".into(),
                        unread: 1,
                    },
                ],
            }],
        };
        dir.cache().store("sidebar", &copy);
        // Del feed 5: un sin leer pasa a leído.
        queue(&dir, &article(1, 5, false, false), read(true));
        // Del feed 6: un leído pasa a guardado, sin tocar los sin leer.
        queue(&dir, &article(2, 6, true, false), starred(true));

        let sidebar = offline_client(&dir)
            .sidebar()
            .expect("sin red debe salir de la copia");

        assert_eq!(sidebar.total, 10);
        assert_eq!(sidebar.unread, 3);
        assert_eq!(sidebar.starred, 3);
        assert_eq!(sidebar.categories[0].unread, 3);
        assert_eq!(sidebar.categories[0].feeds[0].unread, 2);
        assert_eq!(sidebar.categories[0].feeds[1].unread, 1);
    }

    #[test]
    fn offline_entry_shows_its_queued_state() {
        let dir = TempDir::new();
        let entry: Entry = serde_json::from_value(serde_json::json!({
            "id": 7,
            "feed_id": 1,
            "feed_title": "Feed",
            "title": "Título",
            "original_title": "Title",
            "title_translated": false,
            "lang": "en",
            "url": "https://example.com/a",
            "published_at": "2026-01-07T00:00:00Z",
            "read": false,
            "starred": false,
            "author": null,
            "content": "",
            "translation": "not_needed",
            "translated_content": null,
        }))
        .expect("artículo de prueba mal formado");
        dir.cache().store(&entry_key(7), &entry);
        queue(&dir, &summary(7), read(true));

        let copy = offline_client(&dir).entry(7).expect("sin red, de la copia");

        assert!(copy.summary.read);
    }

    #[test]
    fn arxiv_id_comes_from_the_address() {
        assert_eq!(
            arxiv_id_from_url("https://arxiv.org/abs/2610.08812v2").as_deref(),
            Some("2610.08812")
        );
        assert_eq!(
            arxiv_id_from_url("http://arxiv.org/pdf/2610.08812.pdf").as_deref(),
            Some("2610.08812")
        );
        assert_eq!(
            arxiv_id_from_url("https://arxiv.org/html/1706.03762v7/").as_deref(),
            Some("1706.03762")
        );
        assert_eq!(
            arxiv_id_from_url("https://www.arxiv.org/abs/2610.08812?context=cs#abstract")
                .as_deref(),
            Some("2610.08812")
        );
        assert_eq!(
            arxiv_id_from_url("https://arxiv.org/abs/hep-th/9901001").as_deref(),
            Some("hep-th/9901001")
        );
        assert_eq!(
            arxiv_id_from_url("https://arxiv.org/abs/math.AG/0601001v3").as_deref(),
            Some("math.AG/0601001")
        );
        assert_eq!(
            arxiv_id_from_url("https://example.com/abs/2610.08812"),
            None
        );
        assert_eq!(arxiv_id_from_url("https://arxiv.org/list/cs.CL/new"), None);
        assert_eq!(arxiv_id_from_url("https://arxiv.org/abs/2610"), None);
        assert_eq!(arxiv_id_from_url("https://arxiv.org/abs/"), None);
    }

    /// Un artículo como lo da el servicio, con la traducción dada.
    fn entry_body(translation: &str, translated: Option<&str>) -> String {
        serde_json::json!({
            "id": 7,
            "feed_id": 1,
            "feed_title": "Feed",
            "title": "Título",
            "original_title": "Title",
            "title_translated": false,
            "lang": "en",
            "url": "https://example.com/a",
            "published_at": "2026-01-07T00:00:00Z",
            "read": false,
            "starred": false,
            "author": null,
            "content": "<p>One.</p><p>Two.</p>",
            "translation": translation,
            "translated_content": translated,
            "progress": if translation == "pending" {
                serde_json::json!({ "done": 1, "total": 2 })
            } else {
                serde_json::Value::Null
            },
        })
        .to_string()
    }

    /// Hace de servicio: responde `body` a la primera petición.
    fn serve_once(body: String) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir el servidor");
        let port = listener.local_addr().expect("puerto del servidor").port();
        let service = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("conexión de prueba");
            read_request(&mut stream);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("respuesta de prueba");
        });
        (port, service)
    }

    #[test]
    fn partial_translation_is_not_copied_to_disk() {
        let dir = TempDir::new();
        let partial = "<p>Uno.</p><p><span class=\"atalaya-pendiente\">Two.</span></p>";
        let (port, service) = serve_once(entry_body("pending", Some(partial)));

        let entry = client_at(&dir, port)
            .entry(7)
            .expect("el servicio responde");
        service.join().expect("el servicio de prueba falló");

        // Lo que se ve en pantalla sí llega; el disco no lo guarda.
        assert_eq!(entry.translation, TranslationState::Pending);
        assert!(entry.translated_content.is_some());
        assert!(!dir.cache().contains(&entry_key(7)));
    }

    #[test]
    fn finished_translation_is_copied_to_disk() {
        let dir = TempDir::new();
        let (port, service) = serve_once(entry_body("done", Some("<p>Uno.</p><p>Dos.</p>")));

        client_at(&dir, port)
            .entry(7)
            .expect("el servicio responde");
        service.join().expect("el servicio de prueba falló");

        assert!(dir.cache().contains(&entry_key(7)));
    }

    #[test]
    fn stale_send_sends_the_latest_queued_state() {
        let dir = TempDir::new();
        let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir el servidor");
        let port = listener.local_addr().expect("puerto del servidor").port();
        // Hace de servicio: responde una sola petición y devuelve lo que recibió.
        let service = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("conexión de prueba");
            let request = read_request(&mut stream);
            let body = serde_json::to_string(&summary(7)).expect("cuerpo de prueba");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("respuesta de prueba");
            request
        });
        let client = client_at(&dir, port);

        // Dos acciones seguidas: leído y luego sin leer. La tarea de la
        // primera llega la última y debe mandar lo último pedido.
        client.queue_change(&summary(7), &read(true));
        client.queue_change(&summary(7), &read(false));
        client
            .update_entry(&summary(7), &read(true))
            .expect("el servicio responde");

        let request = service.join().expect("el servicio de prueba falló");
        assert!(request.starts_with("PATCH /api/entries/7 "), "{request}");
        assert!(request.contains(r#""read":false"#), "{request}");
        assert!(Pending::new(dir.cache()).all().is_empty());
    }

    #[test]
    fn already_sent_change_is_not_sent_again() {
        let dir = TempDir::new();
        let client = offline_client(&dir);

        // Nada en cola: la tarea no toca la red.
        client
            .update_entry(&summary(7), &read(true))
            .expect("sin nada en cola no hay error");

        assert!(!client.is_offline());
    }

    /// Lee una petición HTTP completa y devuelve el texto entero.
    fn read_request(stream: &mut TcpStream) -> String {
        let mut data = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).expect("lectura de prueba");
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&data).into_owned();
            if let Some(head_end) = text.find("\r\n\r\n") {
                let length: usize = text[..head_end]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .map(str::to_owned)
                    })
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if data.len() >= head_end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    #[test]
    fn server_error_keeps_the_queue() {
        let dir = TempDir::new();
        let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir el servidor");
        let port = listener.local_addr().expect("puerto del servidor").port();
        // Hace de Caddy mientras el servicio se reinicia.
        let service = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("conexión de prueba");
            read_request(&mut stream);
            stream
                .write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("respuesta de prueba");
        });
        let client = client_at(&dir, port);
        let update = read(true);
        queue(&dir, &summary(7), read(true));

        client.send_pending();

        service.join().expect("el servicio de prueba falló");
        assert_eq!(updates(&Pending::new(dir.cache())), vec![(7, update)]);
    }

    #[test]
    fn online_send_empties_the_queue() {
        let dir = TempDir::new();
        let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir el servidor");
        let port = listener.local_addr().expect("puerto del servidor").port();
        // Hace de servicio: responde una sola petición y devuelve lo que recibió.
        let service = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("conexión de prueba");
            let request = read_request(&mut stream);
            let body = serde_json::to_string(&summary(7)).expect("cuerpo de prueba");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("respuesta de prueba");
            request
        });
        let client = client_at(&dir, port);
        queue(&dir, &summary(7), read(true));

        client.send_pending();

        let request = service.join().expect("el servicio de prueba falló");
        assert!(request.starts_with("PATCH /api/entries/7 "), "{request}");
        assert!(request.contains(r#""read":true"#), "{request}");
        assert!(Pending::new(dir.cache()).all().is_empty());
    }
}
