//! Copia en disco de lo que llega del servicio, para leer sin red (L8).
//!
//! Vive en `$XDG_CACHE_HOME/atalaya`, o `~/.cache/atalaya`. Cada respuesta
//! va en un JSON con nombre fijo: si se pierde, el servicio la vuelve a dar.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Un artículo abierto que no está en Guardados se borra pasado este tiempo.
const ENTRY_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn open() -> Option<Self> {
        let base = match std::env::var_os("XDG_CACHE_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => PathBuf::from(std::env::var_os("HOME")?).join(".cache"),
        };
        let dir = base.join("atalaya");
        fs::create_dir_all(dir.join("entries")).ok()?;
        Some(Self { dir })
    }

    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let text = fs::read(self.path(key)).ok()?;
        serde_json::from_slice(&text).ok()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.path(key).exists()
    }

    /// Escribe en un archivo aparte y lo renombra, para que un corte a
    /// medias no deje un JSON roto.
    pub fn store<T: Serialize>(&self, key: &str, value: &T) {
        let Ok(text) = serde_json::to_vec(value) else {
            return;
        };
        let path = self.path(key);
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, &path).is_err() {
            fs::remove_file(&tmp).ok();
        }
    }

    /// Borra los artículos viejos, salvo los de `keep` (los guardados).
    pub fn prune_entries(&self, keep: &[String]) {
        let Ok(dir) = fs::read_dir(self.dir.join("entries")) else {
            return;
        };
        let now = SystemTime::now();
        for file in dir.flatten() {
            let path = file.path();
            let key = format!("entries/{}", file.file_name().to_string_lossy());
            let key = key.trim_end_matches(".json");
            let old = file
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| now.duration_since(t).unwrap_or_default() > ENTRY_MAX_AGE);
            if old && !keep.iter().any(|k| k == key) {
                fs::remove_file(path).ok();
            }
        }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }
}
