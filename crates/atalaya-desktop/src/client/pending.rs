//! Cambios de leído y guardado hechos sin red, a la espera de la red (L8).
//!
//! Se guardan en `pending.json`, dentro de la copia en disco. Por artículo
//! solo cuenta lo último pedido para cada campo: dos cambios seguidos se
//! funden en uno.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use atalaya_api::{EntryUpdate, Id};

use crate::cache::Cache;

/// Nombre del archivo en la copia en disco: `pending.json`.
const KEY: &str = "pending";

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
    pub fn merge(&self, id: Id, update: &EntryUpdate) -> EntryUpdate {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut queue = self.load();
        let change = queue.entry(id).or_default();
        if update.read.is_some() {
            change.read = update.read;
        }
        if update.starred.is_some() {
            change.starred = update.starred;
        }
        let merged = change.clone();
        self.save(&queue);
        merged
    }

    /// Todo lo que espera, por id de artículo.
    pub fn all(&self) -> Vec<(Id, EntryUpdate)> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.load().into_iter().collect()
    }

    /// Quita el cambio que ya salió hacia el servicio. Si mientras tanto se
    /// pidió otro cambio para el mismo artículo, no lo quita: ese se manda
    /// después.
    pub fn forget(&self, id: Id, sent: &EntryUpdate) {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut queue = self.load();
        if queue.get(&id) == Some(sent) {
            queue.remove(&id);
            self.save(&queue);
        }
    }

    fn load(&self) -> BTreeMap<Id, EntryUpdate> {
        self.cache.load(KEY).unwrap_or_default()
    }

    fn save(&self, queue: &BTreeMap<Id, EntryUpdate>) {
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

        pending.merge(7, &read(true));
        pending.merge(7, &read(false));
        let merged = pending.merge(7, &starred(true));

        assert_eq!(
            merged,
            EntryUpdate {
                read: Some(false),
                starred: Some(true),
            }
        );
        assert_eq!(pending.all(), vec![(7, merged)]);
    }

    #[test]
    fn change_survives_on_disk() {
        let dir = TempDir::new();
        Pending::new(dir.cache()).merge(3, &read(true));
        Pending::new(dir.cache()).merge(5, &starred(true));

        let reopened = Pending::new(dir.cache());
        assert_eq!(reopened.all(), vec![(3, read(true)), (5, starred(true))]);
        assert!(dir.0.join("pending.json").exists());
    }

    #[test]
    fn forget_removes_only_what_was_sent() {
        let dir = TempDir::new();
        let pending = Pending::new(dir.cache());
        pending.merge(9, &read(true));

        // Llega otro cambio mientras se enviaba el anterior.
        pending.merge(9, &read(false));
        pending.forget(9, &read(true));
        assert_eq!(pending.all(), vec![(9, read(false))]);

        pending.forget(9, &read(false));
        assert!(pending.all().is_empty());
    }
}
