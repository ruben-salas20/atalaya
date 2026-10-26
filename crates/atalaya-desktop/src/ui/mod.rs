//! Interfaz. Cada columna o pantalla es una vista (`Entity`) en su archivo,
//! para que los pasos trabajen sin pisarse:
//!
//! | Archivo | Qué pinta | Paso |
//! |---|---|---|
//! | `app.rs` | La ventana: elige qué pantalla va a la derecha del menú y conecta los eventos | 5.1 |
//! | `sidebar.rs` | Columna 1: Todos, Sin leer, Guardados, carpetas, Feeds y Ajustes | 5.1 |
//! | `entry_list.rs` | Columna 2: la lista de artículos | 5.1 |
//! | `panels.rs` | Qué columnas caben y cómo se pliegan | #29 |
//! | `reader.rs` | Columna 3: el artículo abierto | 5.2 |
//! | `feeds.rs` | Pantalla 5: gestión de feeds | 5.4 |
//! | `papers/` | Pantalla 4: buscar, abrir y guardar papers | 5.3 |
//! | `fulltext/` | «Texto completo» de un paper: pedirlo, avance y pintado, en el lector de papers y en el de artículos | #90 |
//! | `settings.rs` | Pantalla 3: ajustes | 5.4 |
//! | `updates.rs` | Aviso de versión nueva y sección «Versión» de los ajustes | #54 |
//! | `widgets.rs`, `format.rs` | Piezas comunes: iconos, botones, horas, idioma | todos |

pub mod app;
pub mod entry_list;
pub mod feeds;
pub mod format;
pub mod fulltext;
pub mod panels;
pub mod papers;
pub mod reader;
pub mod settings;
pub mod sidebar;
pub mod smooth_scroll;
pub mod updates;
pub mod widgets;

use std::sync::Arc;

use gpui::{App, Global};

use crate::client::ApiClient;

/// Conexión con el servicio, compartida por todas las vistas. Si los ajustes
/// fallaron, guarda el mensaje para mostrarlo. La pantalla de ajustes (5.4)
/// la sustituye con `cx.set_global` al guardar.
pub struct Connection(pub Result<Arc<ApiClient>, String>);

impl Global for Connection {}

/// El cliente, o el mensaje de por qué no lo hay.
pub fn client(cx: &App) -> Result<Arc<ApiClient>, String> {
    cx.global::<Connection>().0.clone()
}
