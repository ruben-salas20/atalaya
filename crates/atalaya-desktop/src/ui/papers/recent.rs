//! Búsquedas recientes de papers (P6), guardadas solo en este PC, en
//! `$XDG_STATE_HOME/atalaya/busquedas.toml` (o `~/.local/state/…`). Como en
//! Android, no se sincronizan entre aparatos.

use std::path::{Path, PathBuf};

use atalaya_api::PaperField;
use serde::{Deserialize, Serialize};

/// Cuántas se guardan, como en Android.
pub const MAX_RECENT: usize = 10;

/// Una búsqueda reciente: el texto y dónde se buscó.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentSearch {
    pub q: String,
    #[serde(default)]
    pub field: PaperField,
}

impl RecentSearch {
    fn same_as(&self, other: &RecentSearch) -> bool {
        self.field == other.field && self.q.to_lowercase() == other.q.to_lowercase()
    }
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    recent: Vec<RecentSearch>,
}

/// Pone `search` la primera y quita la repetida, sin distinguir mayúsculas.
/// Guarda como mucho [`MAX_RECENT`].
pub fn add_recent(list: &mut Vec<RecentSearch>, search: RecentSearch) {
    let clean = RecentSearch {
        q: search.q.trim().to_owned(),
        field: search.field,
    };
    if clean.q.is_empty() {
        return;
    }
    list.retain(|r| !r.same_as(&clean));
    list.insert(0, clean);
    list.truncate(MAX_RECENT);
}

fn path() -> Option<PathBuf> {
    let state = match std::env::var_os("XDG_STATE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?)
            .join(".local")
            .join("state"),
    };
    Some(state.join("atalaya").join("busquedas.toml"))
}

/// Las guardadas, la más reciente primero. Si el archivo falta o está roto,
/// ninguna: perderlas no es grave.
pub fn load() -> Vec<RecentSearch> {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

fn load_from(path: &Path) -> Vec<RecentSearch> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<File>(&text).ok())
        .map(|f| f.recent)
        .unwrap_or_default()
}

pub fn save(list: &[RecentSearch]) -> Result<(), String> {
    let path = path().ok_or("No se sabe cuál es la carpeta de estado")?;
    save_to(&path, list).map_err(|e| format!("No se pudo guardar {}: {e}", path.display()))
}

fn save_to(path: &Path, list: &[RecentSearch]) -> std::io::Result<()> {
    let file = File {
        recent: list.to_vec(),
    };
    let text = toml::to_string(&file).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(q: &str, field: PaperField) -> RecentSearch {
        RecentSearch { q: q.into(), field }
    }

    #[test]
    fn newest_first_without_repeats() {
        let mut list = vec![s("uno", PaperField::All), s("dos", PaperField::All)];
        add_recent(&mut list, s("  DOS ", PaperField::All));
        assert_eq!(
            list,
            vec![s("DOS", PaperField::All), s("uno", PaperField::All)]
        );
        add_recent(&mut list, s("uno", PaperField::Author));
        assert_eq!(list.len(), 3);
        add_recent(&mut list, s("   ", PaperField::All));
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn keeps_at_most_ten() {
        let mut list = Vec::new();
        for i in 0..15 {
            add_recent(&mut list, s(&i.to_string(), PaperField::All));
        }
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].q, "14");
    }

    #[test]
    fn round_trip_on_disk() {
        let dir = std::env::temp_dir().join(format!("atalaya-busquedas-{}", std::process::id()));
        let path = dir.join("atalaya").join("busquedas.toml");
        let list = vec![
            s("modelos pequeños", PaperField::All),
            s("Ortega", PaperField::Author),
        ];
        save_to(&path, &list).unwrap();
        let back = load_from(&path);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(back, list);
        assert!(load_from(&path).is_empty());
    }
}
