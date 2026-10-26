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
//! | `GET /api/entries` | [`EntryQuery`] en la URL (con `q` busca en los títulos) | [`EntryPage`] | 2.2, #36 |
//! | `GET /api/entries/{id}` | | [`Entry`] | 2.2 y 2.4 |
//! | `PATCH /api/entries/{id}` | [`EntryUpdate`] | [`EntrySummary`] | 2.2 |
//! | `POST /api/entries/mark-all-as-read` | [`MarkAllReadQuery`] en la URL | 204 sin cuerpo | #19 |
//! | `POST /api/feeds/discover` | [`DiscoverRequest`] | `Vec<`[`DiscoveredFeed`]`>` | 2.5 |
//! | `GET /api/feeds` | | `Vec<`[`Feed`]`>` | 2.5 |
//! | `POST /api/feeds` | [`NewFeed`] | 201 y [`Feed`] | 2.5 |
//! | `PATCH /api/feeds/{id}` | [`FeedUpdate`] | [`Feed`] | 2.5 |
//! | `DELETE /api/feeds/{id}` | | 204 sin cuerpo | 2.5 |
//! | `GET /api/categories` | | `Vec<`[`Category`]`>` | 2.5 |
//! | `POST /api/categories` | [`CategoryInput`] | 201 y [`Category`] | 2.5 |
//! | `PATCH /api/categories/{id}` | [`CategoryInput`] | [`Category`] | 2.5 |
//! | `DELETE /api/categories/{id}` | | 204 sin cuerpo; 409 si tiene feeds | 2.5 |
//! | `GET /api/papers/translate` | [`QueryTranslationRequest`] en la URL | [`QueryTranslation`] | 4.1 |
//! | `GET /api/papers/search` | [`PaperQuery`] en la URL | [`PaperPage`] | 4.1 |
//! | `GET /api/papers/{arxiv_id}` | | [`Paper`] | 4.2 |
//! | `POST /api/papers/{arxiv_id}/save` | | 201 y [`EntrySummary`]; 200 si ya estaba | 4.2 |
//!
//! Los identificadores antiguos de arXiv llevan una barra (`hep-th/9901001`).
//! En la ruta va codificada: `hep-th%2F9901001`.
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
///
/// `q` busca en el título original y en el traducido, sin distinguir
/// mayúsculas ni tildes (ver [`fold`]). Con `q`, la vista se recorre entera:
/// `total` cuenta los que encajan, no los de la vista.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryQuery {
    #[serde(default)]
    pub view: View,
    pub feed_id: Option<Id>,
    pub category_id: Option<Id>,
    /// Texto a buscar en el título. Vacío o ausente: no filtra.
    pub q: Option<String>,
    /// Por defecto 50; como mucho 200.
    pub limit: Option<u32>,
    /// Por defecto 0.
    pub offset: Option<u32>,
}

/// Pliega un texto para comparar búsquedas: minúsculas y sin tildes, diéresis
/// ni la marca de la «ñ» (que pasa a «n»). «Modelos» y «MODELÓS» dan lo mismo.
///
/// La app de Android tiene la misma tabla en
/// `android/app/src/main/java/dev/rubensalas/atalaya/data/list/Search.kt`;
/// si cambia aquí, cambia también allí.
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        // Marcas sueltas (acento escrito aparte, como en «o\u{301}»).
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .collect()
}

/// Parámetros de `POST /api/entries/mark-all-as-read`, en la URL:
/// `?category_id=3`. Sin ninguno, marca como leído todo lo visible en «Todos».
///
/// Igual que en [`EntryQuery`], `feed_id` gana a `category_id` si llegan los dos.
/// Los papers guardados no se tocan: solo se marcan los artículos de «Todos».
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MarkAllReadQuery {
    pub feed_id: Option<Id>,
    pub category_id: Option<Id>,
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
    /// HTML en español, con los mismos enlaces, listas e imágenes (T5). Con
    /// `translation: Done`, entero. Con `Pending`, lo traducido hasta ahora y
    /// el resto en el original, cada párrafo pendiente dentro de
    /// `<span class="atalaya-pendiente">` (ADR 0015).
    pub translated_content: Option<String>,
    /// Con `Pending`, cuántos párrafos van traducidos (ADR 0015).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    /// Si el artículo es un paper de arXiv (un paper guardado, o un feed de
    /// arXiv), su identificador sin versión: la app ofrece «Leer completo»
    /// con `/api/papers/{arxiv_id}/full` (#90).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arxiv_id: Option<String>,
}

/// Avance de una traducción en curso: `done` de `total` párrafos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
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
    /// Si es el feed donde caen los papers guardados (Guardados).
    #[serde(default)]
    pub papers: bool,
}

/// Fallo al actualizar un feed (F6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedError {
    /// El mensaje de Miniflux, tal cual.
    pub message: String,
    /// Cuántas veces seguidas ha fallado.
    pub count: u32,
}

// ---------------------------------------------------------------------------
// Papers de arXiv (ADR 0005)
// ---------------------------------------------------------------------------

/// Parámetros de `GET /api/papers/translate`, en la URL: `?q=modelos+pequeños`.
///
/// La app lo pide cuando Rubén deja de escribir unos 0,6 s, para mostrar
/// «Se buscará en inglés: …» antes de buscar (P1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryTranslationRequest {
    pub q: String,
}

/// Cómo se buscará en arXiv una búsqueda escrita en español.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryTranslation {
    /// La búsqueda tal como llegó.
    pub q: String,
    /// La búsqueda en inglés. Igual que `q` si ya estaba en inglés.
    pub english: String,
}

/// Dónde buscar en arXiv (P2). Equivale a los prefijos `all`, `ti`, `au` y
/// `abs` de su API.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperField {
    #[default]
    All,
    Title,
    Author,
    Abstract,
}

/// Cómo ordenar los resultados (P2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperSort {
    /// Del más nuevo al más viejo, por fecha de envío.
    #[default]
    Recent,
    Relevance,
}

/// Parámetros de `GET /api/papers/search`, en la URL:
/// `?q=modelos+pequeños&field=title&sort=relevance&limit=20&offset=0`.
///
/// Con `field: Author`, la búsqueda no se traduce: los nombres van tal cual.
///
/// Si `q` es solo un identificador de arXiv (`1706.03762`, `arXiv:1706.03762v7`
/// o el enlace a su página o su PDF), devuelve ese paper o ninguno, y
/// `english_query` es el identificador. Lo que va entre comillas se busca
/// como frase exacta, también después de traducirlo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PaperQuery {
    /// La búsqueda en español.
    pub q: String,
    #[serde(default)]
    pub field: PaperField,
    #[serde(default)]
    pub sort: PaperSort,
    /// Por defecto 20; como mucho 50.
    pub limit: Option<u32>,
    /// Por defecto 0.
    pub offset: Option<u32>,
}

/// Una página de resultados (P1, P3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperPage {
    /// La búsqueda tal como se envió a arXiv, para «buscado como …».
    pub english_query: String,
    /// Cuántos resultados hay en total según arXiv, no solo en la página.
    pub total: u64,
    pub papers: Vec<PaperSummary>,
}

/// Una fila de los resultados (P3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperSummary {
    /// Identificador de arXiv sin versión: `2610.08812`.
    pub arxiv_id: String,
    /// Título para mostrar: en español si ya está traducido; si no, el original.
    pub title: String,
    pub original_title: String,
    pub title_translated: bool,
    /// En el orden de arXiv. La app abrevia: «Chen y otros 4».
    pub authors: Vec<String>,
    /// Fecha de la primera versión.
    pub published_at: DateTime<Utc>,
    /// Categoría principal de arXiv: `cs.CL`.
    pub category: String,
    /// El artículo de Miniflux si el paper está en Guardados (el marcador de
    /// la pantalla 7); `None` si no.
    pub saved_entry_id: Option<Id>,
}

/// `GET /api/papers/{arxiv_id}`: un paper abierto desde la búsqueda (P4).
///
/// Pedirlo pone en marcha la traducción del resumen si hace falta, y responde
/// enseguida con `translation: Pending`, como [`Entry`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paper {
    #[serde(flatten)]
    pub summary: PaperSummary,
    /// Resumen original, en texto plano. Los párrafos van separados por una
    /// línea en blanco.
    pub abstract_text: String,
    pub translation: TranslationState,
    /// Resumen en español, con los mismos párrafos. Solo con
    /// `translation: Done`.
    pub translated_abstract: Option<String>,
    /// El resumen original en HTML, solo si tiene fórmulas (`$…$`): un `<p>`
    /// por párrafo y cada fórmula como en [`FullPaper::html`] (#80). Sin
    /// fórmulas no viene, y la app usa `abstract_text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstract_html: Option<String>,
    /// Lo mismo para `translated_abstract`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_abstract_html: Option<String>,
    /// Página del paper: `https://arxiv.org/abs/2610.08812`.
    pub abs_url: String,
    /// `https://arxiv.org/pdf/2610.08812`.
    pub pdf_url: String,
}

/// Estado del paper completo traducido (P7, ADR 0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullPaperState {
    /// Nadie lo ha pedido todavía: la app muestra «Leer completo».
    NotRequested,
    /// arXiv no tiene versión HTML de este paper: queda el resumen y el PDF.
    NotAvailable,
    /// Se está traduciendo; `html` trae lo que ya hay (ADR 0015).
    Pending,
    /// `html` trae el paper entero en español.
    Done,
    /// Falló; pedirlo otra vez lo reintenta.
    Failed,
}

/// `GET /api/papers/{arxiv_id}/full` (estado) y `POST` (lo pide; responde
/// igual). P7, ADR 0010.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FullPaper {
    pub state: FullPaperState,
    /// Cuerpo del paper en HTML, sin bibliografía. Con `Pending`, lo
    /// traducido hasta ahora y cada párrafo pendiente en el original dentro de
    /// `<span class="atalaya-pendiente">`; con `Done`, todo en español.
    ///
    /// Cada fórmula es una imagen SVG del servicio, sin clave:
    /// `<img class="atalaya-formula" src="/formulas/2610.31646v1/3.svg"
    /// alt="LaTeX" style="vertical-align: -0.44ex; width: 5.43ex; height: 2ex">`.
    /// Las fórmulas aparte llevan además la clase `atalaya-formula-bloque`.
    /// El SVG pinta con `currentColor`: la app lo cambia por el color de su
    /// texto. Si MathJax no pudo con una fórmula, va como
    /// `<code class="atalaya-latex">LaTeX</code>`.
    pub html: Option<String>,
    /// Con `Pending`, cuántos trozos van traducidos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
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
            progress: Some(Progress { done: 2, total: 5 }),
            arxiv_id: None,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["progress"]["done"], 2);
        assert_eq!(json["translation"], "pending");
        assert_eq!(json["published_at"], "1970-01-01T00:00:00Z");
        assert_eq!(serde_json::from_value::<Entry>(json).unwrap(), entry);
    }

    #[test]
    fn paper_query_defaults() {
        let q: PaperQuery = serde_json::from_str(r#"{"q":"modelos"}"#).unwrap();
        assert_eq!(q.field, PaperField::All);
        assert_eq!(q.sort, PaperSort::Recent);
        assert_eq!(
            serde_json::to_value(PaperField::Abstract).unwrap(),
            "abstract"
        );
    }

    #[test]
    fn query_defaults_to_all() {
        let q: EntryQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.view, View::All);
    }

    #[test]
    fn fold_ignores_case_and_accents() {
        assert_eq!(fold("MODELÓS"), "modelos");
        assert_eq!(fold("Año"), "ano");
        assert_eq!(fold("o\u{301}"), "o");
        assert_eq!(fold("Ç ÿ ü"), "c y u");
        assert_eq!(fold("Conocimiento"), fold("conocimiénto"));
    }

    #[test]
    fn query_accepts_search_text() {
        let q: EntryQuery = serde_json::from_str(r#"{"q":"conocimiento"}"#).unwrap();
        assert_eq!(q.q.as_deref(), Some("conocimiento"));
    }
}
