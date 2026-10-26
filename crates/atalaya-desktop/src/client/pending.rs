//! Cambios de leído y guardado hechos sin red, a la espera de la red (L8).
//!
//! Se guardan en `pending.json`, dentro de la copia en disco. Por artículo
//! solo cuenta lo último pedido para cada campo: dos cambios seguidos se
//! funden en uno. Cada artículo guarda también cómo estaba antes de su
//! primer cambio, para que el menú pueda sumar o restar sin red.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use atalaya_api::{EntrySummary, EntryUpdate, Id};
use serde::{Deserialize, Serialize};

use crate::cache::Cache;

/// Nombre del archivo en la copia en disco: `pending.json`.
const KEY: &str = "pending";

/// Cómo estaba el artículo en el servicio antes de su primer cambio en cola.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Before {
    pub feed_id: Id,
    pub read: bool,
    pub starred: bool,
}

/// Un artículo con cambios en cola.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Queued {
    /// Lo último pedido para cada campo.
    #[serde(flatten)]
    pub update: EntryUpdate,
    /// `None` en las colas de versiones anteriores: esos cambios no mueven
    /// las cifras del menú.
    pub before: Option<Before>,
}

pub struct Pending {
    cache: Cache,
    /// Un solo hilo lee o escribe el archivo a la vez.
    lock: Mutex<()>,
}

impl Pending {
    pub fn new(cache: Cache) -> Self {
        Self {
            cache,
            lock: Mutex::new(()),
        }
    }

    /// Funde `update` en lo pendiente del artículo y devuelve el resultado.
    /// `current` es el artículo tal como estaba antes de este cambio; solo
    /// se usa si es el primer cambio en cola.
    pub fn merge(&self, current: &EntrySummary, update: &EntryUpdate) -> EntryUpdate {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut queue = self.load();
        let change = queue.entry(current.id).or_insert_with(|| Queued {
            update: EntryUpdate::default(),
            before: Some(Before {
                feed_id: current.feed_id,
                read: current.read,
                starred: current.starred,
            }),
        });
        if update.read.is_some() {
            change.update.read = update.read;
        }
        if update.starred.is_some() {
            change.update.starred = update.starred;
        }
        let merged = change.update.clone();
        self.save(&queue);
        merged
    }

    /// Lo último pedido para el artículo, si espera a la red.
    pub fn latest(&self, id: Id) -> Option<EntryUpdate> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.load().get(&id).map(|queued| queued.update.clone())
    }

    /// Todo lo que espera, por id de artículo.
    pub fn all(&self) -> Vec<(Id, Queued)> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.load().into_iter().collect()
    }

    /// Quita el cambio que ya salió hacia el servicio. Si mientras tanto se
    /// pidió otro cambio para el mismo artículo, no lo quita: ese se manda
    /// después.
    pub fn forget(&self, id: Id, sent: &EntryUpdate) {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut queue = self.load();
        if queue.get(&id).map(|queued| &queued.update) == Some(sent) {
            queue.remove(&id);
            self.save(&queue);
        }
    }

    fn load(&self) -> BTreeMap<Id, Queued> {
        self.cache.load(KEY).unwrap_or_default()
    }

    fn save(&self, queue: &BTreeMap<Id, Queued>) {
        self.cache.store(KEY, queue);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    /// Carpeta temporal que se borra al salir de la prueba, aunque falle.
    pub(crate) struct TempDir(pub(crate) PathBuf);

    impl TempDir {
        pub(crate) fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            Self(std::env::temp_dir().join(format!("atalaya-pending-{}-{nanos}", process::id())))
        }

        pub(crate) fn cache(&self) -> Cache {
            Cache::at(self.0.clone()).expect("no se pudo crear la carpeta de prueba")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    /// Un artículo de prueba, con su carpeta y su estado.
    pub(crate) fn article(id: Id, feed_id: Id, read: bool, starred: bool) -> EntrySummary {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "feed_id": feed_id,
            "feed_title": "Feed",
            "title": "Título",
            "original_title": "Title",
            "title_translated": false,
            "lang": "en",
            "url": "https://example.com/a",
            "published_at": format!("2026-01-{id:02}T00:00:00Z"),
            "read": read,
            "starred": starred,
        }))
        .expect("resumen de prueba mal formado")
    }

    /// Lo que espera en la cola, sin la carpeta de cada artículo.
    pub(crate) fn updates(pending: &Pending) -> Vec<(Id, EntryUpdate)> {
        pending
            .all()
            .into_iter()
            .map(|(id, queued)| (id, queued.update))
            .collect()
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

    #[test]
    fn merges_two_changes_to_one_entry() {
        let dir = TempDir::new();
        let pending = Pending::new(dir.cache());
        let current = article(7, 1, false, false);

        pending.merge(&current, &read(true));
        pending.merge(&current, &read(false));
        let merged = pending.merge(&current, &starred(true));

        assert_eq!(
            merged,
            EntryUpdate {
                read: Some(false),
                starred: Some(true),
            }
        );
        assert_eq!(pending.latest(7), Some(merged.clone()));
        assert_eq!(updates(&pending), vec![(7, merged)]);
    }

    #[test]
    fn change_survives_on_disk() {
        let dir = TempDir::new();
        Pending::new(dir.cache()).merge(&article(3, 1, false, false), &read(true));
        Pending::new(dir.cache()).merge(&article(5, 1, false, false), &starred(true));

        let reopened = Pending::new(dir.cache());
        assert_eq!(
            updates(&reopened),
            vec![(3, read(true)), (5, starred(true))]
        );
        assert!(dir.0.join("pending.json").exists());
    }

    #[test]
    fn forget_removes_only_what_was_sent() {
        let dir = TempDir::new();
        let pending = Pending::new(dir.cache());
        let current = article(9, 1, false, false);
        pending.merge(&current, &read(true));

        // Llega otro cambio mientras se enviaba el anterior.
        pending.merge(&current, &read(false));
        pending.forget(9, &read(true));
        assert_eq!(updates(&pending), vec![(9, read(false))]);

        pending.forget(9, &read(false));
        assert!(pending.all().is_empty());
    }

    #[test]
    fn keeps_the_state_from_before_the_first_change() {
        let dir = TempDir::new();
        let pending = Pending::new(dir.cache());
        pending.merge(&article(9, 4, false, false), &read(true));
        // La app ya muestra el cambio: el segundo cambio no toca lo guardado.
        pending.merge(&article(9, 4, true, false), &read(false));

        assert_eq!(
            pending.all()[0].1.before,
            Some(Before {
                feed_id: 4,
                read: false,
                starred: false,
            })
        );
    }

    #[test]
    fn reads_a_queue_from_an_older_version() {
        let dir = TempDir::new();
        let old: BTreeMap<Id, EntryUpdate> = BTreeMap::from([(7, read(true))]);
        dir.cache().store(KEY, &old);

        let pending = Pending::new(dir.cache());
        assert_eq!(updates(&pending), vec![(7, read(true))]);
        assert_eq!(pending.all()[0].1.before, None);
    }
}
