//! Ajustes de la app, en `~/.config/atalaya/ajustes.toml` (permisos 600).
//!
//! Hoy solo se leen. La pantalla de ajustes (paso 5.4) añadirá aquí la
//! escritura: por eso [`Settings`] también se puede serializar.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Dirección del servicio, sin `/api`: `https://atalaya.rubensalas.dev`.
    pub service_url: String,
    /// Clave que va en `Authorization: Bearer`.
    pub api_key: String,
}

// A mano, para que la clave no salga nunca en un log.
impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("service_url", &self.service_url)
            .field("api_key", &"<oculta>")
            .finish()
    }
}

impl Settings {
    /// Ruta del archivo: `$XDG_CONFIG_HOME/atalaya/ajustes.toml`, o
    /// `~/.config/atalaya/ajustes.toml` si esa variable no está.
    pub fn path() -> Option<PathBuf> {
        let config = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
        };
        Some(config.join("atalaya").join("ajustes.toml"))
    }

    pub fn load() -> Result<Self, String> {
        let path = Self::path().ok_or("No se sabe cuál es la carpeta de configuración")?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn parse(text: &str) -> Result<Self, String> {
        let settings: Self = toml::from_str(text)
            .map_err(|e| format!("el archivo no es válido: {}", e.message()))?;
        if settings.service_url.trim().is_empty() || settings.api_key.trim().is_empty() {
            return Err("faltan service_url o api_key".into());
        }
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_keys() {
        let s = Settings::parse("service_url = \"https://x.dev\"\napi_key = \"k\"\n").unwrap();
        assert_eq!(s.service_url, "https://x.dev");
        assert_eq!(s.api_key, "k");
    }

    #[test]
    fn rejects_missing_key() {
        assert!(Settings::parse("service_url = \"https://x.dev\"\n").is_err());
        assert!(Settings::parse("service_url = \"https://x.dev\"\napi_key = \"\"\n").is_err());
    }

    #[test]
    fn debug_hides_key() {
        let s = Settings {
            service_url: "https://x.dev".into(),
            api_key: "secreta".into(),
        };
        assert!(!format!("{s:?}").contains("secreta"));
    }
}
