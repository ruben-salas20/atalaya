//! Ajustes de la app, en `~/.config/atalaya/ajustes.toml` (permisos 600).
//!
//! La pantalla de ajustes los escribe con [`Settings::save`]. El tamaño de
//! letra del lector vive además en el global [`FontSize`], para que el lector
//! lo lea sin abrir el archivo y se repinte cuando cambia.

use std::fmt;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use gpui::{App, Global, Pixels, px};
use serde::{Deserialize, Serialize};

/// Tamaños de letra del lector que ofrece la pantalla de ajustes, en px.
pub const FONT_SIZES: &[u32] = &[14, 15, 16, 17, 18, 19, 20, 22, 24];
/// El de las maquetas (`.article p { font-size: 17px }`).
pub const DEFAULT_FONT_SIZE: u32 = 17;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Dirección del servicio, sin `/api`: `https://atalaya.rubensalas.dev`.
    #[serde(default)]
    pub service_url: String,
    /// Clave que va en `Authorization: Bearer`.
    #[serde(default)]
    pub api_key: String,
    /// Tamaño de letra del texto del artículo, en px.
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    /// El usuario plegó el menú lateral o la lista (ver `ui::panels`).
    #[serde(default)]
    pub sidebar_folded: bool,
    #[serde(default)]
    pub list_folded: bool,
}

fn default_font_size() -> u32 {
    DEFAULT_FONT_SIZE
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            service_url: String::new(),
            api_key: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            sidebar_folded: false,
            list_folded: false,
        }
    }
}

// A mano, para que la clave no salga nunca en un log.
impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("service_url", &self.service_url)
            .field("api_key", &"<oculta>")
            .field("font_size", &self.font_size)
            .field("sidebar_folded", &self.sidebar_folded)
            .field("list_folded", &self.list_folded)
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

    /// Lee el archivo sin exigir que esté completo: la pantalla de ajustes
    /// lo necesita así para que se pueda corregir. Para conectar, después
    /// hay que llamar a [`Settings::validate`].
    pub fn read() -> Result<Self, String> {
        let path = Self::path().ok_or("No se sabe cuál es la carpeta de configuración")?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("el archivo no es válido: {}", e.message()))
    }

    /// Comprueba que hay lo necesario para hablar con el servicio.
    pub fn validate(self) -> Result<Self, String> {
        if self.service_url.trim().is_empty() || self.api_key.trim().is_empty() {
            return Err("Faltan la dirección del servicio o la clave en los ajustes".into());
        }
        Ok(self)
    }

    /// Escribe el archivo con permisos 600. Primero escribe una copia al
    /// lado y luego la renombra, para no dejar el archivo a medias.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("No se sabe cuál es la carpeta de configuración")?;
        self.save_to(&path)
            .map_err(|e| format!("No se pudo guardar {}: {e}", path.display()))
    }

    /// Lee el archivo, aplica `change` y lo guarda. Así cada pantalla cambia
    /// solo lo suyo sin borrar lo que guardó otra.
    /// Si el archivo existe pero no se puede leer, no se toca: se perderían
    /// la dirección y la clave que hubiera que corregir.
    pub fn update(change: impl FnOnce(&mut Self)) -> Result<Self, String> {
        let exists = Self::path().is_some_and(|p| p.exists());
        let mut settings = if exists {
            Self::read()?
        } else {
            Self::default()
        };
        change(&mut settings);
        settings.save()?;
        Ok(settings)
    }

    fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        // `mode` solo cuenta al crear: si la copia ya existía, se fuerza aquí.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    }
}

/// Tamaño de letra del lector, compartido por las vistas. `main` lo pone al
/// arrancar y la pantalla de ajustes lo cambia al elegir otro.
///
/// El lector lo lee con [`font_size`] en su `render`; `AppView` lo repinta
/// cuando cambia.
pub struct FontSize(pub u32);

impl Global for FontSize {}

/// Tamaño de letra del texto del artículo.
pub fn font_size(cx: &App) -> Pixels {
    let size = cx
        .try_global::<FontSize>()
        .map_or(DEFAULT_FONT_SIZE, |f| f.0);
    px(size as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_keys() {
        let s = Settings::parse("service_url = \"https://x.dev\"\napi_key = \"k\"\n").unwrap();
        assert_eq!(s.service_url, "https://x.dev");
        assert_eq!(s.api_key, "k");
        assert_eq!(s.font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn rejects_missing_key() {
        let missing = Settings::parse("service_url = \"https://x.dev\"\n").unwrap();
        assert!(missing.validate().is_err());
        let empty = Settings::parse("service_url = \"https://x.dev\"\napi_key = \"\"\n").unwrap();
        assert!(empty.validate().is_err());
    }

    #[test]
    fn debug_hides_key() {
        let s = Settings {
            service_url: "https://x.dev".into(),
            api_key: "secreta".into(),
            ..Default::default()
        };
        assert!(!format!("{s:?}").contains("secreta"));
    }

    #[test]
    fn saves_with_mode_600() {
        let dir = std::env::temp_dir().join(format!("atalaya-ajustes-{}", std::process::id()));
        let path = dir.join("atalaya").join("ajustes.toml");
        let s = Settings {
            service_url: "https://x.dev".into(),
            api_key: "k".into(),
            font_size: 20,
            sidebar_folded: true,
            list_folded: false,
        };
        s.save_to(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        let back = Settings::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(mode, 0o600);
        assert_eq!(back, s);
    }
}
