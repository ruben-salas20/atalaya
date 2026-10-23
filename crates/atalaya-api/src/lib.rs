//! Contrato de la API de Atalaya.
//!
//! El servicio (`atalaya-server`) y la app de PC usan estos tipos; la app de
//! Android los repite en Kotlin. Todo viaja en JSON, con los campos en
//! `snake_case` y las fechas en RFC 3339 (UTC).
//!
//! # Autenticación
//!
//! Todo lo que cuelga de `/api` exige la cabecera `Authorization: Bearer <clave>`.
//! Sin ella, o con otra clave, responde 401 con un [`ApiError`]. `/health` no
//! la pide, para que Uptime Kuma pueda vigilarlo.
//!
//! # Rutas
//!
//! | Método y ruta | Cuerpo | Respuesta | Paso |
//! |---|---|---|---|
//! | `GET /health` | | [`Health`] | 2.1 |
//! | `GET /api/status` | | [`Status`] | 2.1 |
//! | `GET /api/sidebar` | | [`Sidebar`] | 2.2 |
//! | `GET /api/entries` | [`EntryQuery`] en la URL | [`EntryPage`] | 2.2 |
//! | `GET /api/entries/{id}` | | [`Entry`] | 2.2 y 2.4 |
//! | `PATCH /api/entries/{id}` | [`EntryUpdate`] | [`EntrySummary`] | 2.2 |
//! | `POST /api/feeds/discover` | [`DiscoverRequest`] | `Vec<`[`DiscoveredFeed`]`>` | 2.5 |
//! | `GET /api/feeds` | | `Vec<`[`Feed`]`>` | 2.5 |
//! | `POST /api/feeds` | [`NewFeed`] | [`Feed`] | 2.5 |
//! | `PATCH /api/feeds/{id}` | [`FeedUpdate`] | [`Feed`] | 2.5 |
//! | `DELETE /api/feeds/{id}` | | 204 sin cuerpo | 2.5 |
//! | `GET /api/categories` | | `Vec<`[`Category`]`>` | 2.5 |
//! | `POST /api/categories` | [`CategoryInput`] | [`Category`] | 2.5 |
//! | `PATCH /api/categories/{id}` | [`CategoryInput`] | [`Category`] | 2.5 |
//! | `DELETE /api/categories/{id}` | | 204 sin cuerpo | 2.5 |
//!
//! En Miniflux, una carpeta se llama «categoría». Aquí se usa su nombre.
//!
//! Los errores responden con el código HTTP que toque y un [`ApiError`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Identificador de un artículo, un feed o una categoría. Es el de Miniflux.
pub type Id = i64;

/// Cuerpo de cualquier respuesta con error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    /// Qué falló, en español, para mostrarlo tal cual.
    pub error: String,
}

/// `GET /health`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    /// Siempre `"ok"` si el servicio responde.
    pub status: String,
    pub version: String,
}

/// `GET /api/status`: lo que usa el botón «Probar» de los ajustes (S3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    /// Si el servicio llega a Miniflux con su clave.
    pub miniflux: bool,
}

// ---------------------------------------------------------------------------
// Lectura
// ---------------------------------------------------------------------------

/// Vista fija de la lista (L1). Las de un feed o carpeta se piden con
/// `feed_id` o `category_id` en [`EntryQuery`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    All,
    Unread,
    Starred,
}

/// Parámetros de `GET /api/entries`, en la URL:
/// `?view=unread&category_id=3&limit=50&offset=0`.
///
/// `view` se combina con `feed_id` o `category_id`: «sin leer de este feed».
/// Si llegan los dos, manda `feed_id`. El orden es siempre del más nuevo al
/// más viejo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryQuery {
    #[serde(default)]
    pub view: View,
    pub feed_id: Option<Id>,
    pub category_id: Option<Id>,
    /// Por defecto 50; como mucho 200.
    pub limit: Option<u32>,
    /// Por defecto 0.
    pub offset: Option<u32>,
}

/// Una página de la lista.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryPage {
    /// Cuántos artículos hay en total en esa vista, no solo en la página.
    pub total: u64,
    pub entries: Vec<EntrySummary>,
}

/// Una fila de la lista (L2, L3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntrySummary {
    pub id: Id,
    pub feed_id: Id,
    /// Nombre del feed, que la lista muestra como fuente.
    pub feed_title: String,
    /// Título para mostrar: en español si ya está traducido; si no, el original.
    pub title: String,
    pub original_title: String,
    /// Si `title` es una traducción.
    pub title_translated: bool,
    /// Idioma original en ISO 639-1 y minúsculas (`"en"`). `None` mientras no
    /// se ha detectado. La app lo muestra en mayúsculas, salvo si es `"es"`.
    pub lang: Option<String>,
    pub url: String,
    pub published_at: DateTime<Utc>,
    pub read: bool,
    pub starred: bool,
}

/// Estado de la traducción del texto completo (L5, L6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationState {
    /// El original ya está en español, o el idioma no se pudo detectar.
    NotNeeded,
    /// Se está traduciendo. La app muestra el original con «Traduciendo…» y
    /// vuelve a pedir el artículo cada pocos segundos.
    Pending,
    /// `translated_content` tiene la traducción.
    Done,
    /// La traducción falló. La app muestra el original.
    Failed,
}

/// `GET /api/entries/{id}`: un artículo abierto.
///
/// Pedirlo pone en marcha la traducción del texto completo si hace falta, y
/// responde enseguida con `translation: Pending` (ADR 0003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(flatten)]
    pub summary: EntrySummary,
    pub author: Option<String>,
    /// HTML original. Si el feed solo trae un resumen, el servicio pide antes
    /// el artículo completo a Miniflux (L7).
    pub content: String,
    pub translation: TranslationState,
    /// HTML en español, con los mismos enlaces, listas e imágenes (T5). Solo
    /// con `translation: Done`.
    pub translated_content: Option<String>,
}

/// `PATCH /api/entries/{id}`. Los campos que faltan no cambian.
///
/// Guardar (`starred: true`) también pone en marcha la traducción del texto
/// completo (T2).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryUpdate {
    pub read: Option<bool>,
    pub starred: Option<bool>,
}

/// `GET /api/sidebar`: el menú lateral (L1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidebar {
    /// Artículos en «Todos».
    pub total: u64,
    pub unread: u64,
    pub starred: u64,
    /// Carpetas en orden alfabético, cada una con sus feeds.
    pub categories: Vec<SidebarCategory>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidebarCategory {
    pub id: Id,
    pub title: String,
    /// Suma de los sin leer de sus feeds.
    pub unread: u64,
    pub feeds: Vec<SidebarFeed>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidebarFeed {
    pub id: Id,
    pub title: String,
    pub unread: u64,
}

// ---------------------------------------------------------------------------
// Feeds y carpetas (ADR 0007)
// ---------------------------------------------------------------------------

/// Una carpeta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: Id,
    pub title: String,
}

/// Cuerpo para crear o renombrar una carpeta (F5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryInput {
    pub title: String,
}

/// `POST /api/feeds/discover` (F1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoverRequest {
    /// Dirección de la web o del feed, con o sin `https://`: `theverge.com`.
    pub url: String,
}

/// Un feed encontrado en una web, aún sin añadir (F1, F2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredFeed {
    pub feed_url: String,
    pub title: String,
    /// Idioma de sus artículos en ISO 639-1, o `None` si no se pudo saber.
    pub lang: Option<String>,
    /// Media de artículos al día, calculada con las fechas de los últimos.
    /// `None` si no hay fechas suficientes.
    pub articles_per_day: Option<f64>,
}

/// `POST /api/feeds` (F3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewFeed {
    pub feed_url: String,
    pub category_id: Id,
    /// Descargar el artículo completo: el «crawler» de Miniflux.
    pub crawler: bool,
}

/// `PATCH /api/feeds/{id}` (F4). Los campos que faltan no cambian.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeedUpdate {
    pub title: Option<String>,
    pub category_id: Option<Id>,
    pub crawler: Option<bool>,
}

/// Un feed suscrito (F2, F4, F6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feed {
    pub id: Id,
    pub title: String,
    pub site_url: String,
    pub feed_url: String,
    pub category: Category,
    pub crawler: bool,
    /// Idioma de sus artículos en ISO 639-1, o `None` si no se sabe aún.
    pub lang: Option<String>,
    pub articles_per_day: Option<f64>,
    /// Última vez que Miniflux lo actualizó.
    pub checked_at: Option<DateTime<Utc>>,
    /// Si falló la última actualización.
    pub error: Option<FeedError>,
}

/// Fallo al actualizar un feed (F6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedError {
    /// El mensaje de Miniflux, tal cual.
    pub message: String,
    /// Cuántas veces seguidas ha fallado.
    pub count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_flattens_summary() {
        let entry = Entry {
            summary: EntrySummary {
                id: 1,
                feed_id: 2,
                feed_title: "The Verge".into(),
                title: "Hola".into(),
                original_title: "Hello".into(),
                title_translated: true,
                lang: Some("en".into()),
                url: "https://example.com".into(),
                published_at: DateTime::UNIX_EPOCH,
                read: false,
                starred: false,
            },
            author: None,
            content: "<p>Hello</p>".into(),
            translation: TranslationState::Pending,
            translated_content: None,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["translation"], "pending");
        assert_eq!(json["published_at"], "1970-01-01T00:00:00Z");
        assert_eq!(serde_json::from_value::<Entry>(json).unwrap(), entry);
    }

    #[test]
    fn query_defaults_to_all() {
        let q: EntryQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.view, View::All);
    }
}
