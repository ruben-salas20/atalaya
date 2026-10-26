//! Traducción del texto completo de un artículo (paso 2.4; T2, T4, T5, L6, L7).
//!
//! Empieza al abrir o al guardar un artículo, en [`Translations::content`], y
//! responde enseguida con `Pending`. El trabajo sigue en segundo plano:
//!
//! 1. Si el feed solo trae un resumen, pide el artículo completo a Miniflux
//!    (ver [`is_summary_only`]) y lo guarda en la base, porque Miniflux no lo
//!    conserva: al refrescar un feed sin «artículo completo», reescribe el
//!    texto con el del feed.
//! 2. Divide el HTML en párrafos ([`split`]): lo que hay entre dos etiquetas
//!    de bloque (`<p>`, `<li>`, `<h2>`, `<br>`…). Las etiquetas de bloque, las
//!    imágenes sueltas y el código no pasan por el modelo.
//! 3. Manda cada párrafo a la cola de la GPU, que comparte con los títulos.
//!    Cada traducción se comprueba ([`check`]); si sale mal, se repite una
//!    vez, y si sigue mal se deja el párrafo original (ADR 0009).
//! 4. Recompone el HTML con los párrafos traducidos y lo guarda.
//!
//! El trabajo se identifica por el SHA-256 del texto que da Miniflux, antes de
//! pedir el completo. Así no cambia cuando Miniflux reescribe el resumen.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use atalaya_api::{Id, Progress, TranslationState};
use reqwest::Method;
use serde::Deserialize;
use tokio::sync::oneshot;

use crate::{
    error::AppError,
    llama::{CUT_OFF, Llama},
    miniflux::Miniflux,
    store::{self, FIELD_CONTENT, STATE_DONE, STATE_FAILED},
    titles::{NewEntry, Priority, SOURCE_LANGS},
    translations::Translations,
};

/// Un feed sin «artículo completo» cuyo texto visible tiene menos caracteres
/// que esto se toma por un resumen. Los resúmenes de The Verge rondan los 700;
/// sus artículos completos más cortos, unos 3000.
const SUMMARY_MAX_CHARS: usize = 1500;

/// Tras un fallo, abrir el artículo otra vez no reintenta hasta pasado este
/// tiempo. Así la app recibe `Failed` y deja de preguntar.
const RETRY_FAILED_AFTER_SECS: i64 = 600;

/// Intentos por párrafo antes de dejar el original.
const ATTEMPTS_PER_SEGMENT: usize = 2;

/// Tiempo máximo para que Miniflux descargue el artículo completo.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

const HTML_PROMPT: &str = "Translate the following text from English into Latin American Spanish. \
Its tags are replaced by short placeholders: <t1>...</t1> wraps some text, and <t2/> stands alone. \
Keep every placeholder exactly as it is, with the same number and in the same place around the words it wraps. \
Do not add, remove, renumber or invent placeholders. Translate only the visible text. \
Reply with the translated text only, without notes, explanations or code fences.\n\n";

const TEXT_PROMPT: &str = "Translate the following text from English into Latin American Spanish. \
Reply with the Spanish translation only, without notes, explanations or quotes.\n\n";

/// Elementos cuyo contenido no se traduce.
const VERBATIM: &[&str] = &[
    "pre", "code", "script", "style", "svg", "math", "iframe", "video", "audio", "object",
    "textarea", "noscript",
];

/// Elementos que separan párrafos. Lo que no está aquí (`a`, `em`, `img`…)
/// viaja dentro del párrafo.
const BLOCK: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "br",
    "caption",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "picture",
    "section",
    "source",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
];

/// Etiquetas que nunca tienen cierre (`<br>`, `<img>`…).
const VOID: &[&str] = &[
    "area", "base", "col", "embed", "input", "link", "meta", "param", "source", "track", "wbr",
    "br", "hr", "img",
];

/// Palabras que en un texto español casi nunca aparecen. Las mismas que
/// `bench/traduccion/revisar.py`.
const ENGLISH: &[&str] = &[
    "the", "and", "of", "to", "is", "that", "with", "for", "this", "are", "was", "have", "it",
    "which", "from",
];

/// Artículos que se están traduciendo ahora mismo, con lo que llevan
/// traducido (ADR 0015).
#[derive(Default)]
pub struct InProgress(Mutex<HashMap<Id, Partial>>);

/// Lo traducido hasta ahora de un artículo en curso. Vacío mientras se pide el
/// artículo completo a Miniflux.
#[derive(Default)]
pub struct Partial {
    parts: Vec<Part>,
    /// Uno por cada [`Part::Translate`], en orden. `None`: aún no llegó.
    translated: Vec<Option<String>>,
}

impl Partial {
    /// El HTML con lo traducido y el resto en el original, y el avance. `None`
    /// si aún no se sabe cuántos párrafos hay.
    fn render(&self) -> Option<(String, Progress)> {
        if self.parts.is_empty() {
            return None;
        }
        let done = self.translated.iter().filter(|t| t.is_some()).count();
        let progress = Progress {
            done: done as u32,
            total: self.translated.len() as u32,
        };
        Some((assemble_partial(&self.parts, &self.translated), progress))
    }
}

/// Lo que se sabe de la traducción del texto completo de un artículo.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentState {
    pub translation: TranslationState,
    /// Con `Done`, el HTML en español; con `Pending`, el parcial, si ya hay.
    pub html: Option<String>,
    pub progress: Option<Progress>,
}

impl ContentState {
    fn only(translation: TranslationState) -> Self {
        Self {
            translation,
            html: None,
            progress: None,
        }
    }
}

/// Saca el artículo de [`InProgress`] al terminar, también si el trabajo falla.
pub(crate) struct InProgressGuard {
    translations: Translations,
    entry_id: Id,
}

impl Drop for InProgressGuard {
    fn drop(&mut self) {
        let map = &self.translations.in_progress().0;
        map.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.entry_id);
    }
}

/// Un párrafo por traducir, para el trabajador de la GPU.
#[derive(Debug)]
pub struct SegmentJob {
    /// Artículo al que pertenece: decide su turno en la cola (ADR 0015). Los
    /// papers completos usan ids negativos ([`crate::papers::full`]).
    pub entry_id: Id,
    pub kind: SegmentKind,
    pub html: String,
    pub reply: oneshot::Sender<Result<Segment, String>>,
}

/// Qué se traduce y con qué comprobaciones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// Párrafo de un artículo: [`translate_segment`].
    Article,
    /// Párrafo de un paper con fórmulas `<m id="N">LaTeX</m>` (ADR 0010).
    Paper,
    /// Celdas de una tabla de paper: `html` es una lista JSON de textos y la
    /// respuesta, otra con las traducciones en el mismo orden.
    TableCells,
}

/// Resultado de un párrafo.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub html: String,
    /// `false` si se dejó el original porque la traducción salía mal.
    pub translated: bool,
}

impl Translations {
    /// Ver [`Translations::content`]. Los errores de Miniflux o de la base
    /// suben hasta allí.
    pub(crate) async fn content_state(&self, entry_id: Id) -> Result<ContentState, String> {
        if let Some(partial) = self.partial(entry_id) {
            // Quien pregunta lo está leyendo: sus párrafos pasan delante.
            self.touch_content(entry_id);
            return Ok(partial);
        }
        let entry = self
            .miniflux()
            .entry_source(entry_id)
            .await
            .map_err(|e| e.message)?;
        if !self.needs_translation(&entry).await? {
            return Ok(ContentState::only(TranslationState::NotNeeded));
        }

        let hash = store::source_hash(&entry.content);
        let row = self
            .store()
            .call(move |c| store::content_row(c, entry_id))
            .await
            .map_err(|e| e.to_string())?;
        if let Some(row) = row.filter(|r| r.source_hash == hash) {
            match row.state.as_str() {
                STATE_DONE => {
                    return Ok(ContentState {
                        translation: TranslationState::Done,
                        html: row.text,
                        progress: None,
                    });
                }
                STATE_FAILED if row.age_secs < RETRY_FAILED_AFTER_SECS => {
                    return Ok(ContentState::only(TranslationState::Failed));
                }
                // `pending` sin trabajo en marcha: el servicio se reinició.
                _ => {}
            }
        }

        if self.mark_in_progress(entry_id) {
            let guard = InProgressGuard {
                translations: self.clone(),
                entry_id,
            };
            tokio::spawn(self.clone().translate_content(entry, hash, guard));
        }
        Ok(ContentState::only(TranslationState::Pending))
    }

    /// Si el artículo está en curso, lo traducido hasta ahora.
    fn partial(&self, entry_id: Id) -> Option<ContentState> {
        let map = self
            .in_progress()
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let partial = map.get(&entry_id)?;
        let (html, progress) = partial.render().unzip();
        Some(ContentState {
            translation: TranslationState::Pending,
            html,
            progress,
        })
    }

    /// Apunta en el artículo en curso qué párrafos hay ([`Self::translate_html`]).
    fn start_partial(&self, entry_id: Id, parts: &[Part]) {
        let total = parts
            .iter()
            .filter(|p| matches!(p, Part::Translate(_)))
            .count();
        let mut map = self
            .in_progress()
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(partial) = map.get_mut(&entry_id) {
            partial.parts = parts.to_vec();
            partial.translated = vec![None; total];
        }
    }

    /// Apunta un párrafo ya traducido del artículo en curso.
    fn record_segment(&self, entry_id: Id, index: usize, html: &str) {
        let mut map = self
            .in_progress()
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(slot) = map
            .get_mut(&entry_id)
            .and_then(|p| p.translated.get_mut(index))
        {
            *slot = Some(html.to_owned());
        }
    }

    /// El artículo completo pedido a Miniflux, si se pidió para este mismo
    /// texto del feed (`feed_content`).
    pub async fn original_content(&self, entry_id: Id, feed_content: &str) -> Option<String> {
        let hash = store::source_hash(feed_content);
        match self
            .store()
            .call(move |c| store::content_row(c, entry_id))
            .await
        {
            Ok(row) => row
                .filter(|r| r.source_hash == hash)
                .and_then(|r| r.original),
            Err(e) => {
                tracing::error!(entry = entry_id, "no se pudo leer el original: {e}");
                None
            }
        }
    }

    /// `true` si no estaba en curso y ahora sí.
    fn mark_in_progress(&self, entry_id: Id) -> bool {
        let mut map = self
            .in_progress()
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if map.contains_key(&entry_id) {
            return false;
        }
        map.insert(entry_id, Partial::default());
        true
    }

    /// Da el texto del artículo por «en curso» hasta soltar lo que devuelve,
    /// para que abrirlo no empiece otra traducción. `None` si ya lo estaba.
    /// Lo usa un paper guardado mientras se traduce su resumen (paso 4.2).
    pub(crate) fn hold_in_progress(&self, entry_id: Id) -> Option<InProgressGuard> {
        self.mark_in_progress(entry_id).then(|| InProgressGuard {
            translations: self.clone(),
            entry_id,
        })
    }

    /// Si el artículo está en un idioma que se traduce (T1, T3). Si aún no se
    /// había visto, detecta su idioma y pone en cola su título.
    async fn needs_translation(&self, entry: &SourceEntry) -> Result<bool, String> {
        let id = entry.id;
        let mut lang = self
            .store()
            .call(move |c| store::entry_lang(c, id))
            .await
            .map_err(|e| e.to_string())?;
        if lang.is_none() {
            let seen = NewEntry::new(id, entry.feed_id, entry.title.clone(), &entry.content);
            self.submit_titles(vec![seen], Priority::Urgent).await;
            lang = self
                .store()
                .call(move |c| store::entry_lang(c, id))
                .await
                .map_err(|e| e.to_string())?;
        }
        let lang = lang.flatten();
        Ok(lang.is_some_and(|l| SOURCE_LANGS.contains(&l.as_str()))
            && (!entry.feed.crawler || has_letters(&entry.content)))
    }

    async fn translate_content(self, entry: SourceEntry, hash: String, _guard: InProgressGuard) {
        let id = entry.id;
        let started = Instant::now();

        let mut original = entry.content.clone();
        let mut fetched = None;
        if is_summary_only(&entry) {
            match self.miniflux().fetch_full_content(id).await {
                Ok(full) if visible_chars(&full) > visible_chars(&original) => {
                    let full = with_feed_images(full, &entry.content);
                    original = full.clone();
                    fetched = Some(full);
                }
                Ok(_) => tracing::info!(entry = id, "el artículo completo no es más largo"),
                Err(e) => tracing::warn!(
                    entry = id,
                    "no se pudo pedir el artículo completo: {}",
                    e.message
                ),
            }
        }
        let start_hash = hash.clone();
        let saved = self
            .store()
            .call(move |c| store::start_content(c, id, &start_hash, fetched.as_deref()))
            .await;
        if let Err(e) = saved {
            tracing::error!(entry = id, "no se pudo guardar el comienzo: {e}");
        }

        let result = self.translate_html(id, &original).await;
        let secs = started.elapsed().as_secs_f32();
        match &result {
            Ok(_) => tracing::info!(entry = id, secs, "texto traducido"),
            Err(e) => tracing::warn!(entry = id, secs, "no se pudo traducir el texto: {e}"),
        }
        let model = self.llama().model().to_owned();
        let saved = self
            .store()
            .call(move |c| store::save_translation(c, id, FIELD_CONTENT, &hash, &model, &result))
            .await;
        if let Err(e) = saved {
            tracing::error!(entry = id, "no se pudo guardar el texto traducido: {e}");
        }
    }

    /// Traduce el HTML párrafo a párrafo, por la cola de la GPU.
    async fn translate_html(&self, id: Id, html: &str) -> Result<String, String> {
        let parts = split(html);
        self.start_partial(id, &parts);
        // Se ponen todos en cola de una vez: así la GPU no espera entre uno y
        // otro, y lo urgente puede colarse entre dos párrafos.
        let mut replies = Vec::new();
        for part in &parts {
            if let Part::Translate(text) = part {
                let (reply, rx) = oneshot::channel();
                self.queue_segment(SegmentJob {
                    entry_id: id,
                    kind: SegmentKind::Article,
                    html: text.clone(),
                    reply,
                });
                replies.push(rx);
            }
        }
        let total = replies.len();
        let mut translated = Vec::with_capacity(total);
        let mut kept = 0;
        for (index, rx) in replies.into_iter().enumerate() {
            // Si algo falla, salir suelta los demás `rx` y el trabajador
            // descarta sus párrafos sin traducirlos.
            let segment = rx.await.map_err(|_| "la cola de traducción se cerró")??;
            if !segment.translated {
                kept += 1;
            }
            // Ya se puede leer mientras sigue el resto (ADR 0015).
            self.record_segment(id, index, &segment.html);
            translated.push(segment.html);
        }
        if total > 0 && kept == total {
            return Err(format!("ninguno de los {total} párrafos salió bien"));
        }
        if kept > 0 {
            tracing::info!(
                entry = id,
                "{kept} de {total} párrafos quedan en el original"
            );
        }
        Ok(assemble(&parts, &translated))
    }
}

/// Traduce un párrafo con sus comprobaciones. Lo llama el trabajador de la
/// GPU. Devuelve error solo si llama-swap no responde: entonces el artículo
/// entero falla, porque seguir sería sumar esperas.
pub async fn translate_segment(llama: &Llama, html: &str) -> Result<Segment, String> {
    translate_segment_with(llama, html, |_, _| Ok(())).await
}

/// Como [`translate_segment`], con una comprobación más de cada traducción
/// (la de las fórmulas de los papers, por ejemplo).
pub async fn translate_segment_with(
    llama: &Llama,
    html: &str,
    extra_check: impl Fn(&str, &str) -> Result<(), String>,
) -> Result<Segment, String> {
    // Un título en negrita al principio del párrafo («Resultados. El modelo…»)
    // el modelo lo pierde: se traduce aparte.
    if let Some((head, tail)) = split_run_in_heading(html) {
        let head = translate_piece(llama, head, &extra_check).await?;
        let tail = translate_piece(llama, tail, &extra_check).await?;
        return Ok(Segment {
            html: format!("{}{}", head.html, tail.html),
            translated: head.translated && tail.translated,
        });
    }
    translate_piece(llama, html, &extra_check).await
}

async fn translate_piece(
    llama: &Llama,
    html: &str,
    extra_check: &impl Fn(&str, &str) -> Result<(), String>,
) -> Result<Segment, String> {
    // Una etiqueta que envuelve todo el párrafo (un título en negrita, por
    // ejemplo) el modelo la suele perder: se quita antes y se pone después.
    let (before, inner, after) = peel_wrappers(html);
    let (source, originals) = abbreviate(inner);
    // Un salto de línea tras un título en negrita hace que el modelo traduzca
    // solo el título y pare: en HTML vale lo mismo que un espacio.
    let source = join_lines(&source);
    let prompt = if originals.is_empty() {
        format!("{TEXT_PROMPT}{source}")
    } else {
        format!("{HTML_PROMPT}{source}")
    };
    let max_tokens = (html.len() / 2 + 256).clamp(256, 8192) as u32;
    for attempt in 1..=ATTEMPTS_PER_SEGMENT {
        let raw = match llama.complete(&prompt, max_tokens).await {
            Ok(raw) => raw,
            Err(e) if e == CUT_OFF => {
                tracing::debug!(attempt, "párrafo cortado por largo");
                continue;
            }
            Err(e) => return Err(e),
        };
        let candidate = match restore(&clean_output(&raw), &originals) {
            Ok(restored) if html.contains('*') => format!("{before}{restored}{after}"),
            // El modelo marca en cursiva Markdown los préstamos del inglés
            // (`*prompts*`); si el original no tiene asteriscos, sobran.
            Ok(restored) => format!("{before}{}{after}", restored.replace('*', "")),
            Err(why) => {
                tracing::debug!(attempt, "párrafo rechazado: {why}");
                continue;
            }
        };
        match check(html, &candidate).and_then(|()| extra_check(html, &candidate)) {
            Ok(()) => {
                return Ok(Segment {
                    html: candidate,
                    translated: true,
                });
            }
            Err(why) => tracing::debug!(attempt, "párrafo rechazado: {why}"),
        }
    }
    Ok(Segment {
        html: html.to_owned(),
        translated: false,
    })
}

/// Lo que se usa de un artículo de Miniflux para traducir su texto.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceEntry {
    pub id: Id,
    pub feed_id: Id,
    pub title: String,
    #[serde(default)]
    pub content: String,
    pub feed: SourceFeed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourceFeed {
    /// «Obtener el contenido original» de Miniflux: si está activo, Miniflux
    /// ya guarda el artículo completo.
    #[serde(default)]
    pub crawler: bool,
}

impl Miniflux {
    async fn entry_source(&self, id: Id) -> Result<SourceEntry, AppError> {
        self.get(&format!("/v1/entries/{id}")).await
    }

    /// Artículo completo, descargado por Miniflux desde su web con su
    /// extractor. En Miniflux 2.3.4 la ruta es `GET`, no `PUT` como en
    /// versiones anteriores. Sin `update_content=true`, Miniflux no lo guarda:
    /// solo lo devuelve. No cambia nada del artículo.
    async fn fetch_full_content(&self, id: Id) -> Result<String, AppError> {
        #[derive(Deserialize)]
        struct Fetched {
            content: String,
        }
        let req = self
            .request(Method::GET, &format!("/v1/entries/{id}/fetch-content"))
            .timeout(FETCH_TIMEOUT);
        let fetched: Fetched = Self::send(req).await?;
        Ok(fetched.content)
    }
}

/// Si el texto de un artículo es solo un resumen (L7): el feed no tiene
/// activo «artículo completo» en Miniflux y el texto visible es corto, o
/// acaba en puntos suspensivos o en un «sigue leyendo».
pub fn is_summary_only(entry: &SourceEntry) -> bool {
    if entry.feed.crawler {
        return false;
    }
    let text = visible_text(&entry.content);
    let text = text.trim();
    if text.chars().count() < SUMMARY_MAX_CHARS {
        return true;
    }
    let tail: String = text
        .chars()
        .rev()
        .take(200)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>()
        .to_lowercase();
    [
        "…",
        "[...]",
        "read the full story",
        "continue reading",
        "read more",
    ]
    .iter()
    .any(|m| tail.contains(m))
}

/// El artículo completo con las imágenes del resumen del feed delante, si
/// no trae ninguna. El extractor de Miniflux suele perder la imagen principal
/// (en The Verge, siempre), y el resumen sí la trae.
pub fn with_feed_images(full: String, summary: &str) -> String {
    if full.contains("<img") {
        return full;
    }
    let images = feed_images(summary);
    if images.is_empty() {
        full
    } else {
        images + &full
    }
}

/// Las imágenes de un HTML: cada `<figure>` entera, con su pie, y cada
/// `<img>` suelta dentro de un `<p>`.
fn feed_images(html: &str) -> String {
    let mut out = String::new();
    let mut figure: Option<(String, usize)> = None;
    for token in tokenize(html) {
        if let Some((raw, depth)) = &mut figure {
            raw.push_str(token.raw());
            if let Token::Tag { name, closing, .. } = &token
                && name == "figure"
                && !token.is_self_closing()
            {
                if *closing {
                    *depth -= 1;
                } else {
                    *depth += 1;
                }
            }
            if *depth == 0 {
                out.push_str(raw);
                figure = None;
            }
            continue;
        }
        if let Token::Tag {
            raw,
            name,
            closing: false,
        } = &token
        {
            if name == "figure" && !token.is_self_closing() {
                figure = Some(((*raw).to_owned(), 1));
            } else if name == "img" {
                out.push_str("<p>");
                out.push_str(raw);
                out.push_str("</p>");
            }
        }
    }
    out
}

/// Un trozo del HTML original.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// Se copia tal cual: etiquetas de bloque, espacios, código, imágenes sueltas.
    Keep(String),
    /// Un párrafo: texto con sus etiquetas de dentro (`<a>`, `<em>`…).
    Translate(String),
}

/// Divide un HTML en párrafos por traducir y lo que se copia tal cual. Unir
/// todos los trozos da el HTML original, byte a byte.
pub fn split(html: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut run = String::new();
    let mut verbatim: Option<(String, usize)> = None;
    for token in tokenize(html) {
        if let Some((name, depth)) = &mut verbatim {
            if let Token::Tag {
                name: n, closing, ..
            } = &token
                && *n == *name
                && !token.is_self_closing()
            {
                if *closing {
                    *depth -= 1;
                } else {
                    *depth += 1;
                }
            }
            let done = *depth == 0;
            push_keep(&mut parts, token.raw());
            if done {
                verbatim = None;
            }
            continue;
        }
        match &token {
            Token::Text(raw) => run.push_str(raw),
            Token::Tag {
                raw,
                name,
                closing: false,
            } if VERBATIM.contains(&name.as_str()) => {
                flush(&mut parts, &mut run);
                push_keep(&mut parts, raw);
                if !token.is_self_closing() {
                    verbatim = Some((name.clone(), 1));
                }
            }
            Token::Tag { raw, name, .. } if BLOCK.contains(&name.as_str()) => {
                flush(&mut parts, &mut run);
                push_keep(&mut parts, raw);
            }
            Token::Tag { raw, .. } => run.push_str(raw),
            Token::Other(raw) => {
                flush(&mut parts, &mut run);
                push_keep(&mut parts, raw);
            }
        }
    }
    flush(&mut parts, &mut run);
    parts
}

/// Une los trozos con las traducciones, en el orden de los [`Part::Translate`].
pub fn assemble(parts: &[Part], translated: &[String]) -> String {
    let mut out = String::new();
    let mut next = translated.iter();
    for part in parts {
        match part {
            Part::Keep(s) => out.push_str(s),
            Part::Translate(s) => out.push_str(next.next().unwrap_or(s)),
        }
    }
    out
}

/// Como [`assemble`], pero con los párrafos que faltan en el original, dentro
/// de `<span class="atalaya-pendiente">` para que las apps los marquen
/// (ADR 0015). Un párrafo solo lleva etiquetas de dentro de un bloque, así
/// que el `span` siempre queda bien anidado.
pub fn assemble_partial(parts: &[Part], translated: &[Option<String>]) -> String {
    let mut out = String::new();
    let mut next = translated.iter();
    for part in parts {
        match part {
            Part::Keep(s) => out.push_str(s),
            Part::Translate(s) => match next.next() {
                Some(Some(t)) => out.push_str(t),
                _ => {
                    out.push_str("<span class=\"atalaya-pendiente\">");
                    out.push_str(s);
                    out.push_str("</span>");
                }
            },
        }
    }
    out
}

fn push_keep(parts: &mut Vec<Part>, raw: &str) {
    if raw.is_empty() {
        return;
    }
    if let Some(Part::Keep(last)) = parts.last_mut() {
        last.push_str(raw);
    } else {
        parts.push(Part::Keep(raw.to_owned()));
    }
}

/// Cierra el párrafo en curso. Si no tiene letras, se copia tal cual; si las
/// tiene, los espacios de los extremos quedan fuera de lo que se traduce.
fn flush(parts: &mut Vec<Part>, run: &mut String) {
    if run.is_empty() {
        return;
    }
    let text = std::mem::take(run);
    if !has_letters(&text) {
        push_keep(parts, &text);
        return;
    }
    let core = text.trim();
    let start = text.len() - text.trim_start().len();
    push_keep(parts, &text[..start]);
    parts.push(Part::Translate(core.to_owned()));
    push_keep(parts, &text[start + core.len()..]);
}

/// Comprueba la traducción de un párrafo (ADR 0009, `revisar.py`): mismas
/// etiquetas y mismos enlaces, que no siga en inglés y que no se haya comido
/// ni inventado texto.
pub fn check(original: &str, translated: &str) -> Result<(), String> {
    if translated.trim().is_empty() {
        return Err("vacío".into());
    }
    let (tags_o, urls_o) = signature(original);
    let (tags_t, urls_t) = signature(translated);
    if tags_o != tags_t {
        return Err(format!("etiquetas distintas: {tags_o:?} → {tags_t:?}"));
    }
    if urls_o != urls_t {
        return Err("enlaces distintos".into());
    }
    let text_o = visible_text(original);
    let text_t = visible_text(translated);
    if english_share(&text_t) > 0.12 {
        return Err("sigue en inglés".into());
    }
    let words = words(&text_o).len();
    if words >= 4 && text_o.trim() == text_t.trim() {
        return Err("sin traducir".into());
    }
    let (len_o, len_t) = (text_o.trim().chars().count(), text_t.trim().chars().count());
    if len_o >= 40 && (len_t * 2 < len_o || len_t * 2 > len_o * 5) {
        return Err(format!("largo extraño: {len_o} → {len_t} caracteres"));
    }
    Ok(())
}

/// Quita lo que el modelo añade alrededor: espacios y vallas de código.
pub fn clean_output(raw: &str) -> String {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // Quita la primera línea (```html) y la valla del final.
        let body = rest.split_once('\n').map_or("", |(_, b)| b);
        let body = body.trim_end();
        let body = body.strip_suffix("```").unwrap_or(body);
        return body.trim().to_owned();
    }
    t.to_owned()
}

/// Una etiqueta original, tal cual estaba. `close` solo existe si la etiqueta
/// de apertura tiene su cierre dentro del párrafo.
struct Original {
    raw: String,
    close: Option<String>,
}

/// La forma corta de una etiqueta: `<tN>` y `</tN>` si forma pareja, `<tN/>`
/// si va sola. Los números empiezan en 1.
fn placeholder(n: usize, pair: bool, closing: bool) -> String {
    match (pair, closing) {
        (false, _) => format!("<t{n}/>"),
        (true, true) => format!("</t{n}>"),
        (true, false) => format!("<t{n}>"),
    }
}

/// Cambia cada etiqueta del párrafo por su forma corta. Devuelve el texto
/// abreviado y la tabla para deshacer el cambio con [`restore`]. Un cierre
/// sin apertura, o una apertura sin cierre, queda como etiqueta suelta.
fn abbreviate(html: &str) -> (String, Vec<Original>) {
    let tokens = group_opaque(html, tokenize(html));
    let mut originals: Vec<Original> = Vec::new();
    // Por token: la etiqueta de la tabla que le toca y si es de cierre.
    let mut slots: Vec<Option<(usize, bool)>> = Vec::with_capacity(tokens.len());
    // Aperturas que aún esperan su cierre.
    let mut open: Vec<(&str, usize)> = Vec::new();
    for token in &tokens {
        let slot = match token {
            Token::Text(_) => None,
            Token::Other(raw) => {
                originals.push(Original {
                    raw: (*raw).to_owned(),
                    close: None,
                });
                Some((originals.len() - 1, false))
            }
            Token::Tag { raw, name, closing } => {
                if *closing && let Some(k) = open.iter().rposition(|(n, _)| *n == name.as_str()) {
                    let (_, e) = open[k];
                    // Las aperturas que quedaron encima se quedan sueltas.
                    open.truncate(k);
                    originals[e].close = Some((*raw).to_owned());
                    Some((e, true))
                } else {
                    originals.push(Original {
                        raw: (*raw).to_owned(),
                        close: None,
                    });
                    let e = originals.len() - 1;
                    if !*closing && !token.is_self_closing() && !VOID.contains(&name.as_str()) {
                        open.push((name.as_str(), e));
                    }
                    Some((e, *closing))
                }
            }
        };
        slots.push(slot);
    }
    let mut out = String::with_capacity(html.len());
    for (token, slot) in tokens.iter().zip(slots) {
        match slot {
            None => out.push_str(token.raw()),
            Some((e, closing)) => {
                let pair = originals[e].close.is_some();
                out.push_str(&placeholder(e + 1, pair, closing));
            }
        }
    }
    (out, originals)
}

/// Si el párrafo empieza con un título en negrita (`<b>`, `<strong>` o un
/// `span` con `font_bold`) y sigue con más texto, lo parte en
/// `(título, resto)`.
fn split_run_in_heading(html: &str) -> Option<(&str, &str)> {
    let tokens = tokenize(html);
    let first = tokens
        .iter()
        .position(|t| !matches!(t, Token::Text(x) if x.trim().is_empty()))?;
    let Token::Tag {
        raw,
        name,
        closing: false,
    } = &tokens[first]
    else {
        return None;
    };
    if !(matches!(name.as_str(), "b" | "strong") || (name == "span" && raw.contains("font_bold"))) {
        return None;
    }
    let mut depth = 0i32;
    let close = tokens[first..].iter().position(|t| {
        if let Token::Tag {
            name: n, closing, ..
        } = t
            && n == name
        {
            depth += if *closing { -1 } else { 1 };
        }
        depth == 0
    })? + first;
    let at =
        tokens[close].raw().as_ptr() as usize - html.as_ptr() as usize + tokens[close].raw().len();
    let (head, tail) = html.split_at(at);
    let has_words = |s: &str| {
        tokenize(s)
            .iter()
            .any(|t| matches!(t, Token::Text(x) if x.chars().any(char::is_alphabetic)))
    };
    (has_words(head) && has_words(tail)).then_some((head, tail))
}

/// Cambia cada salto de línea, con los espacios de alrededor, por un espacio.
fn join_lines(text: &str) -> String {
    text.split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Separa las etiquetas que envuelven el párrafo entero, de fuera a dentro:
/// `(apertura, interior, cierre)`. Si no hay, la apertura y el cierre van vacíos.
fn peel_wrappers(html: &str) -> (&str, &str, &str) {
    let (mut start, mut end) = (0, html.len());
    loop {
        let inner = &html[start..end];
        let tokens = tokenize(inner);
        let solid = |t: &&Token<'_>| !matches!(t, Token::Text(x) if x.trim().is_empty());
        let (Some(first), Some(last)) = (
            tokens.iter().position(|t| solid(&t)),
            tokens.iter().rposition(|t| solid(&t)),
        ) else {
            break;
        };
        let (
            Token::Tag {
                name,
                closing: false,
                ..
            },
            Token::Tag {
                name: last_name,
                closing: true,
                ..
            },
        ) = (&tokens[first], &tokens[last])
        else {
            break;
        };
        if first >= last
            || name != last_name
            || tokens[first].is_self_closing()
            || VOID.contains(&name.as_str())
        {
            break;
        }
        // La apertura debe cerrarse justo en el último token, no antes.
        let mut depth = 0i32;
        let closes_at_end = tokens[first..=last].iter().enumerate().all(|(k, t)| {
            if let Token::Tag {
                name: n, closing, ..
            } = t
                && n == name
            {
                depth += if *closing { -1 } else { 1 };
            }
            depth > 0 || first + k == last
        });
        if !closes_at_end {
            break;
        }
        let offset = |t: &Token<'_>| t.raw().as_ptr() as usize - inner.as_ptr() as usize;
        let new_start = start + offset(&tokens[first]) + tokens[first].raw().len();
        end = start + offset(&tokens[last]);
        start = new_start;
    }
    (&html[..start], &html[start..end], &html[end..])
}

/// Elementos que viajan enteros como una sola forma corta `<tN/>`: el modelo
/// no ve su interior. Las citas de un paper (`[<a>1</a>, <a>2</a>]`) se
/// rompían al traducirlas: el modelo pegaba el corchete al cierre, `</t3]` (#38).
const OPAQUE: &[&str] = &["cite"];

/// Junta cada elemento de [`OPAQUE`], de la apertura a su cierre, en un solo
/// [`Token::Other`]. Si no se cierra, se queda como estaba.
fn group_opaque<'a>(html: &'a str, tokens: Vec<Token<'a>>) -> Vec<Token<'a>> {
    let offset = |t: &Token<'_>| t.raw().as_ptr() as usize - html.as_ptr() as usize;
    // Por cada apertura opaca bien cerrada: (índice de su cierre, texto entero).
    let mut spans = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if let Token::Tag {
            name,
            closing: false,
            ..
        } = &tokens[i]
            && OPAQUE.contains(&name.as_str())
            && !tokens[i].is_self_closing()
        {
            let mut depth = 0i32;
            let end = tokens[i..].iter().position(|t| {
                if let Token::Tag {
                    name: n, closing, ..
                } = t
                    && n == name
                {
                    depth += if *closing { -1 } else { 1 };
                }
                depth == 0
            });
            if let Some(e) = end {
                let last = &tokens[i + e];
                spans.push((
                    i,
                    i + e,
                    &html[offset(&tokens[i])..offset(last) + last.raw().len()],
                ));
                i += e + 1;
                continue;
            }
        }
        i += 1;
    }
    let mut out = Vec::with_capacity(tokens.len());
    let mut spans = spans.into_iter().peekable();
    let mut skip_to = None;
    for (k, token) in tokens.into_iter().enumerate() {
        if let Some(end) = skip_to {
            if k == end {
                skip_to = None;
            }
            continue;
        }
        if let Some(&(start, end, raw)) = spans.peek()
            && start == k
        {
            spans.next();
            out.push(Token::Other(raw));
            if end > k {
                skip_to = Some(end);
            }
            continue;
        }
        out.push(token);
    }
    out
}

/// Deshace [`abbreviate`]. Solo acepta cada forma corta exacta, una vez por
/// etiqueta y bien anidada. Si falta, sobra o cambia alguna, devuelve error.
fn restore(abbreviated: &str, originals: &[Original]) -> Result<String, String> {
    let mut seen = vec![false; originals.len()];
    let mut open: Vec<usize> = Vec::new();
    let mut out = String::with_capacity(abbreviated.len());
    for token in tokenize(abbreviated) {
        let (raw, name, closing) = match token {
            Token::Text(text) => {
                out.push_str(text);
                continue;
            }
            Token::Other(raw) => return Err(format!("texto inventado: {raw}")),
            Token::Tag { raw, name, closing } => (raw, name, closing),
        };
        let n = name
            .strip_prefix('t')
            .and_then(|d| d.parse::<usize>().ok())
            .filter(|n| (1..=originals.len()).contains(n))
            .ok_or_else(|| format!("etiqueta inventada: {raw}"))?;
        let i = n - 1;
        let original = &originals[i];
        if raw != placeholder(n, original.close.is_some(), closing) {
            return Err(format!("etiqueta cambiada: {raw}"));
        }
        match &original.close {
            None => {
                if std::mem::replace(&mut seen[i], true) {
                    return Err(format!("etiqueta repetida: {raw}"));
                }
                out.push_str(&original.raw);
            }
            Some(close) if closing => {
                if open.pop() != Some(i) {
                    return Err(format!("etiqueta mal anidada: {raw}"));
                }
                out.push_str(close);
            }
            Some(_) => {
                if std::mem::replace(&mut seen[i], true) {
                    return Err(format!("etiqueta repetida: {raw}"));
                }
                open.push(i);
                out.push_str(&original.raw);
            }
        }
    }
    if !open.is_empty() {
        return Err("etiqueta sin cerrar".into());
    }
    if let Some(i) = seen.iter().position(|s| !s) {
        return Err(format!("falta la etiqueta {}", i + 1));
    }
    Ok(out)
}

/// Etiquetas (nombre, y `/` delante si cierra) y direcciones de `href` y
/// `src`, ordenadas para comparar sin mirar el orden.
fn signature(html: &str) -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut urls = Vec::new();
    for token in tokenize(html) {
        if let Token::Tag { raw, name, closing } = token {
            tags.push(if closing { format!("/{name}") } else { name });
            for attr in ["href", "src"] {
                if let Some(v) = attr_value(raw, attr) {
                    urls.push(v.replace("&amp;", "&"));
                }
            }
        }
    }
    tags.sort();
    urls.sort();
    (tags, urls)
}

/// Valor de un atributo entre comillas, como `href="…"`.
fn attr_value<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(attr) {
        let at = from + i;
        from = at + attr.len();
        let before_ok = lower[..at].chars().last().is_some_and(char::is_whitespace);
        let rest = lower[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value_start = tag.len() - rest[1..].trim_start().len();
        let quote = tag[value_start..].chars().next()?;
        if quote == '"' || quote == '\'' {
            let body = &tag[value_start + 1..];
            return body.find(quote).map(|end| &body[..end]);
        }
        let body = &tag[value_start..];
        let end = body
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(body.len());
        return Some(&body[..end]);
    }
    None
}

fn has_letters(html: &str) -> bool {
    visible_text(html).chars().any(char::is_alphabetic)
}

fn visible_chars(html: &str) -> usize {
    visible_text(html).trim().chars().count()
}

/// Texto sin etiquetas. Las entidades (`&amp;`, `&#39;`) cuentan como un
/// espacio: aquí solo importa si hay letras y cuántas.
fn visible_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    for token in tokenize(html) {
        match token {
            Token::Text(t) => out.push_str(&strip_entities(t)),
            _ => out.push(' '),
        }
    }
    out
}

fn strip_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let len = after
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '#')
            .unwrap_or(after.len());
        if len > 0 && len <= 10 && after[len..].starts_with(';') {
            out.push(' ');
            rest = &after[len + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Parte de palabras inglesas muy comunes. Con menos de 8 palabras no se
/// juzga: un nombre propio basta para dar un falso aviso.
fn english_share(text: &str) -> f64 {
    let words = words(text);
    if words.len() < 8 {
        return 0.0;
    }
    let english = words
        .iter()
        .filter(|w| ENGLISH.contains(&w.as_str()))
        .count();
    english as f64 / words.len() as f64
}

#[derive(Debug, PartialEq)]
enum Token<'a> {
    Text(&'a str),
    Tag {
        raw: &'a str,
        /// En minúsculas.
        name: String,
        closing: bool,
    },
    /// Comentario, `<!DOCTYPE>` o `<?…?>`.
    Other(&'a str),
}

impl Token<'_> {
    fn raw(&self) -> &str {
        match self {
            Token::Text(r) | Token::Other(r) | Token::Tag { raw: r, .. } => r,
        }
    }

    fn is_self_closing(&self) -> bool {
        matches!(self, Token::Tag { raw, .. } if raw.ends_with("/>"))
    }
}

/// Corta el HTML en texto y etiquetas. No valida nada: un `<` que no abre
/// una etiqueta es texto, y una etiqueta sin cerrar llega hasta el final.
fn tokenize(html: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let bytes = html.as_bytes();
    let mut text_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        let after = bytes.get(i + 2).copied().unwrap_or(0);
        let end = if html[i..].starts_with("<!--") {
            html[i..].find("-->").map_or(html.len(), |e| i + e + 3)
        } else if next == b'!' || next == b'?' {
            html[i..].find('>').map_or(html.len(), |e| i + e + 1)
        } else if next.is_ascii_alphabetic() || (next == b'/' && after.is_ascii_alphabetic()) {
            tag_end(html, i)
        } else {
            i += 1;
            continue;
        };
        if text_start < i {
            tokens.push(Token::Text(&html[text_start..i]));
        }
        let raw = &html[i..end];
        if next == b'!' || next == b'?' {
            tokens.push(Token::Other(raw));
        } else {
            let closing = next == b'/';
            let name: String = raw[if closing { 2 } else { 1 }..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            tokens.push(Token::Tag { raw, name, closing });
        }
        i = end;
        text_start = end;
    }
    if text_start < html.len() {
        tokens.push(Token::Text(&html[text_start..]));
    }
    tokens
}

/// Fin de la etiqueta que empieza en `start`, sin cortar en un `>` que vaya
/// entre comillas dentro de un atributo.
fn tag_end(html: &str, start: usize) -> usize {
    let mut quote: Option<u8> = None;
    for (offset, &b) in html.as_bytes()[start..].iter().enumerate() {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => {
                // Solo cuenta como comilla de atributo tras un `=`.
                let prev = html.as_bytes()[..start + offset]
                    .iter()
                    .rev()
                    .find(|c| !c.is_ascii_whitespace());
                if prev == Some(&b'=') {
                    quote = Some(b);
                }
            }
            None if b == b'>' => return start + offset + 1,
            None => {}
        }
    }
    html.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translatable(parts: &[Part]) -> Vec<&str> {
        parts
            .iter()
            .filter_map(|p| match p {
                Part::Translate(s) => Some(s.as_str()),
                Part::Keep(_) => None,
            })
            .collect()
    }

    fn rejoin(parts: &[Part]) -> String {
        assemble(parts, &[])
    }

    const VERGE: &str = r#"<figure>

<img alt="Elon Musk looking up." src="https://x.com/a.jpg?quality=90&amp;strip=all" loading="lazy"/>
	<figcaption>
		</figcaption>
</figure>
<p>Last week, Starshield was <em>throttled</em> leading to a reported <a href="https://g.com/a">two deaths</a>.</p>
<h3>Fi Ultra dog tracker</h3>
The Fi Ultra is the first dog tracker.
<ul><li><a href="https://shop.com/?a=1&amp;b=2"> $299 at <strong>Fi</strong></a></li></ul>
<pre><code>let x = "not <p> translated";</code></pre>
<p><a href="https://www.theverge.com/x">Read the full story at The Verge.</a></p>"#;

    #[test]
    fn splits_paragraphs_and_keeps_the_rest() {
        let parts = split(VERGE);
        assert_eq!(rejoin(&parts), VERGE, "unir los trozos da el original");
        assert_eq!(
            translatable(&parts),
            [
                r#"Last week, Starshield was <em>throttled</em> leading to a reported <a href="https://g.com/a">two deaths</a>."#,
                "Fi Ultra dog tracker",
                "The Fi Ultra is the first dog tracker.",
                r#"<a href="https://shop.com/?a=1&amp;b=2"> $299 at <strong>Fi</strong></a>"#,
                r#"<a href="https://www.theverge.com/x">Read the full story at The Verge.</a>"#,
            ]
        );
    }

    #[test]
    fn line_breaks_separate_paragraphs() {
        let html = "<img src=\"t.gif\" alt=\"The Need to Read\"/><br/><br/>November 2022<br/><br/>In the \
                    books I read,\nreading was [<a href=\"#f1n\">1</a>] replaced.<br/><br/>&nbsp;<br/>";
        let parts = split(html);
        assert_eq!(rejoin(&parts), html);
        assert_eq!(
            translatable(&parts),
            [
                "November 2022",
                "In the books I read,\nreading was [<a href=\"#f1n\">1</a>] replaced."
            ]
        );
    }

    #[test]
    fn nested_verbatim_and_odd_markup() {
        let html = "<p>a < b and <!-- note --> c</p><pre>x<pre>y</pre>z</pre><p title=\"1 > 0\">Hi there</p>";
        let parts = split(html);
        assert_eq!(rejoin(&parts), html);
        assert_eq!(translatable(&parts), ["a < b and", "c", "Hi there"]);
    }

    #[test]
    fn partial_marks_pending_paragraphs() {
        let parts = split("<p>One <a href=\"x\">link</a>.</p><p>Two.</p><p>Three.</p>");
        let html = assemble_partial(
            &parts,
            &[Some("Uno <a href=\"x\">enlace</a>.".into()), None, None],
        );
        assert_eq!(
            html,
            "<p>Uno <a href=\"x\">enlace</a>.</p>\
             <p><span class=\"atalaya-pendiente\">Two.</span></p>\
             <p><span class=\"atalaya-pendiente\">Three.</span></p>"
        );
        let partial = Partial {
            translated: vec![Some("Uno.".into()), None, Some("Tres.".into())],
            parts,
        };
        let (_, progress) = partial.render().unwrap();
        assert_eq!(progress, Progress { done: 2, total: 3 });
        assert!(Partial::default().render().is_none());
    }

    #[test]
    fn assembles_translations_in_order() {
        let html = "<p>One</p>\n<p>Two <a href=\"u\">link</a></p>";
        let parts = split(html);
        let out = assemble(
            &parts,
            &["Uno".into(), "Dos <a href=\"u\">enlace</a>".into()],
        );
        assert_eq!(out, "<p>Uno</p>\n<p>Dos <a href=\"u\">enlace</a></p>");
    }

    #[test]
    fn check_accepts_a_good_translation() {
        let o = r#"Last week, Starshield was <em>throttled</em> leading to a reported <a href="https://g.com/a?x=1&amp;y=2">two deaths</a>."#;
        let t = r#"La semana pasada, Starshield fue <em>limitado</em>, lo que causó <a href="https://g.com/a?x=1&y=2">dos muertes</a>."#;
        assert_eq!(check(o, t), Ok(()));
    }

    #[test]
    fn check_rejects_lost_tags_links_and_english() {
        let o = r#"Read <a href="https://a.com">this</a> and <em>that</em> now."#;
        assert!(check(o, "Lee esto y <em>aquello</em> ahora.").is_err());
        assert!(
            check(
                o,
                r#"Lee <a href="https://b.com">esto</a> y <em>aquello</em> ahora."#
            )
            .is_err()
        );
        assert!(
            check(
                o,
                r#"Lee <a href="https://a.com">esto</a> y aquello ahora."#
            )
            .is_err()
        );
        assert!(check(o, "").is_err());

        let english = "The company said that the change is part of a plan to cut costs and that it was approved.";
        assert!(check(english, english).is_err());
        assert!(check("Fi Ultra dog tracker", "Fi Ultra dog tracker").is_err());
        // Un nombre corto puede quedar igual.
        assert_eq!(check("Fi Ultra", "Fi Ultra"), Ok(()));
        // Ni comerse la mitad ni inventar.
        assert!(check(english, "La empresa dijo.").is_err());
    }

    const ABBREVIATED: &str = r#"Read <a href="https://a.com/?q=1&amp;b=2" title='say "hi"'>this</a> and <em>that <strong>now</strong></em>, <img src="x.png"/> ok<br>."#;

    #[test]
    fn abbreviation_round_trips_without_changes() {
        let (abbr, originals) = abbreviate(ABBREVIATED);
        assert_eq!(
            abbr,
            "Read <t1>this</t1> and <t2>that <t3>now</t3></t2>, <t4/> ok<t5/>."
        );
        assert!(!abbr.contains("href"), "los atributos no van al modelo");
        assert_eq!(restore(&abbr, &originals), Ok(ABBREVIATED.to_owned()));
        let plain = "Sin etiquetas, solo texto.";
        let (abbr, originals) = abbreviate(plain);
        assert_eq!(abbr, plain);
        assert_eq!(restore(&abbr, &originals), Ok(plain.to_owned()));
    }

    #[test]
    fn abbreviation_keeps_nested_pairs() {
        let html = "<em>a <strong>b</strong></em>";
        let (abbr, originals) = abbreviate(html);
        assert_eq!(abbr, "<t1>a <t2>b</t2></t1>");
        assert_eq!(restore(&abbr, &originals), Ok(html.to_owned()));
    }

    #[test]
    fn restored_translation_passes_check() {
        let o = r#"Read <a href="https://a.com/?x=1&amp;y=2">this</a> now."#;
        let (_, originals) = abbreviate(o);
        let restored = restore("Lee <t1>esto</t1> ahora.", &originals).unwrap();
        assert_eq!(
            restored,
            r#"Lee <a href="https://a.com/?x=1&amp;y=2">esto</a> ahora."#
        );
        assert_eq!(check(o, &restored), Ok(()));
    }

    #[test]
    fn split_run_in_heading_separa_el_titulo() {
        let html = "<span class=\"ltx_text ltx_font_bold\">Part Quality.</span>\nWe first <b>evaluate</b>.";
        assert_eq!(
            split_run_in_heading(html),
            Some((
                "<span class=\"ltx_text ltx_font_bold\">Part Quality.</span>",
                "\nWe first <b>evaluate</b>."
            ))
        );
        assert_eq!(split_run_in_heading("<b>Solo título.</b>"), None);
        assert_eq!(split_run_in_heading("Texto <b>x</b> más"), None);
        assert_eq!(split_run_in_heading("<span>No negrita.</span> texto"), None);
    }

    #[test]
    fn join_lines_deja_una_sola_linea() {
        assert_eq!(
            join_lines("<t1>Título.</t1>\n  Texto\n\nmás "),
            "<t1>Título.</t1> Texto más"
        );
        assert_eq!(join_lines("sin saltos"), "sin saltos");
    }

    #[test]
    fn peel_wrappers_quita_lo_que_envuelve_todo() {
        assert_eq!(
            peel_wrappers("<span class=\"b\"><b>Título.</b></span>"),
            ("<span class=\"b\"><b>", "Título.", "</b></span>")
        );
        assert_eq!(
            peel_wrappers("<b>a</b> y <b>b</b>"),
            ("", "<b>a</b> y <b>b</b>", "")
        );
        assert_eq!(peel_wrappers("texto <b>x</b>"), ("", "texto <b>x</b>", ""));
        assert_eq!(peel_wrappers(" <i>x</i> "), (" <i>", "x", "</i> "));
    }

    #[test]
    fn citations_travel_as_one_placeholder() {
        let html = "computation <cite class=\"c\">[<a href=\"#b1\">1</a>, <a href=\"#b2\">2</a>]</cite>. Next";
        let (short, originals) = abbreviate(html);
        assert_eq!(short, "computation <t1/>. Next");
        let back = restore("cómputo <t1/>. Siguiente", &originals).unwrap();
        assert_eq!(
            back,
            "cómputo <cite class=\"c\">[<a href=\"#b1\">1</a>, <a href=\"#b2\">2</a>]</cite>. Siguiente"
        );
        assert!(check(html, &back).is_ok());
        // Sin cierre, se queda como estaba.
        assert_eq!(abbreviate("a <cite>b").0, "a <t1/>b");
    }

    #[test]
    fn restore_rejects_lost_tags() {
        let (_, originals) = abbreviate(ABBREVIATED);
        assert!(restore("Lee <t1>esto</t1> y aquello ahora.", &originals).is_err());
        assert!(restore("Lee esto y <t2>aquello</t2>.", &originals).is_err());
        assert!(restore("<t1>Lee</t1> <t2>y</t2><t3>a</t3></t2>", &originals).is_err());
    }

    #[test]
    fn restore_rejects_invented_tags() {
        let (_, originals) = abbreviate(ABBREVIATED);
        let base = "<t1>Lee</t1> <t2>y <t3>ya</t3></t2>, <t4/> ok<t5/>.";
        assert!(restore(base, &originals).is_ok());
        assert!(restore(&format!("{base}<t6/>"), &originals).is_err());
        assert!(restore(&format!("{base}<b>x</b>"), &originals).is_err());
        assert!(restore(&format!("{base}<!-- x -->"), &originals).is_err());
    }

    #[test]
    fn restore_rejects_bad_nesting_and_changed_forms() {
        let (_, originals) = abbreviate("<em>a <strong>b</strong></em>");
        assert!(restore("<t1>a <t2>b</t1></t2>", &originals).is_err());
        assert!(restore("<t1 class=\"x\">a <t2>b</t2></t1>", &originals).is_err());
        assert!(restore("<t1>a <t2>b</t2>", &originals).is_err());
        assert!(restore("<t1>a <t2>b</t2></t1><t1>c</t1>", &originals).is_err());
    }

    #[test]
    fn cleans_code_fences() {
        assert_eq!(clean_output("```html\n<p>Hola</p>\n```\n"), "<p>Hola</p>");
        assert_eq!(clean_output("  Hola  "), "Hola");
    }

    #[test]
    fn reads_attribute_values() {
        let tag = r#"<a class="x" href="https://a.com/?q=1" data-src='no' src=y.png>"#;
        assert_eq!(attr_value(tag, "href"), Some("https://a.com/?q=1"));
        assert_eq!(attr_value(tag, "src"), Some("y.png"));
        assert_eq!(attr_value("<a>", "href"), None);
    }

    fn entry(crawler: bool, content: &str) -> SourceEntry {
        SourceEntry {
            id: 1,
            feed_id: 1,
            title: "t".into(),
            content: content.into(),
            feed: SourceFeed { crawler },
        }
    }

    #[test]
    fn adds_feed_images_when_full_content_has_none() {
        let summary = r#"<figure><img src="a.jpg" alt=""><figcaption>Foto</figcaption></figure><p>Hi <img src="b.png"/></p>"#;
        let full = "<p>Full text.</p>".to_owned();
        assert_eq!(
            with_feed_images(full, summary),
            r#"<figure><img src="a.jpg" alt=""><figcaption>Foto</figcaption></figure><p><img src="b.png"/></p><p>Full text.</p>"#
        );
        let with_img = r#"<p><img src="c.jpg"></p>"#.to_owned();
        assert_eq!(with_feed_images(with_img.clone(), summary), with_img);
        assert_eq!(
            with_feed_images("<p>x</p>".into(), "<p>sin fotos</p>"),
            "<p>x</p>"
        );
    }

    #[test]
    fn summary_only_criterion() {
        let short = "<p>Short teaser about the news.</p>";
        assert!(is_summary_only(&entry(false, short)));
        assert!(!is_summary_only(&entry(true, short)));
        let long = format!(
            "<p>{}</p>",
            "A complete sentence about the news. ".repeat(60)
        );
        assert!(!is_summary_only(&entry(false, &long)));
        let cut = format!("<p>{}and then b …</p>", "A complete sentence. ".repeat(100));
        assert!(is_summary_only(&entry(false, &cut)));
        assert!(is_summary_only(&entry(false, "")));
    }

    #[test]
    fn entities_do_not_count_as_letters() {
        assert!(!has_letters("&nbsp;&#39; 42 <img alt=\"Photo\">"));
        assert!(has_letters("Tom &amp; Jerry"));
        assert_eq!(visible_chars("<p>a&amp;b</p>"), 3);
    }
}
