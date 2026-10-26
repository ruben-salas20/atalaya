//! Paper completo traducido desde el HTML de arXiv (P7, ADR 0010).
//!
//! `POST /api/papers/{id}/full` lo pide y `GET` dice cómo va. El trabajo:
//!
//! 1. Descarga `https://arxiv.org/html/<id>/`. Si no hay HTML, el paper queda
//!    como «no disponible».
//! 2. [`prep`] se queda con el cuerpo, sin bibliografía, y cambia cada
//!    fórmula por una marca con su LaTeX.
//! 3. [`mathjax`] convierte todas las fórmulas en SVG, de una vez, antes de
//!    traducir: así lo parcial ya se ve con fórmulas (ADR 0015).
//! 4. Los párrafos van a la cola de la GPU como los de los artículos, con la
//!    comprobación de las marcas; las tablas, enteras, con sus celdas juntas.
//! 5. Al terminar, se montan tablas y fórmulas ([`render`]) y se guarda.

pub mod db;
pub mod mathjax;
pub mod prep;
pub mod render;

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Mutex},
};

use atalaya_api::{FullPaper, FullPaperState, Id, Progress};
use tokio::sync::oneshot;

use crate::{
    content::{self, Part, Segment, SegmentJob, SegmentKind},
    llama::{CUT_OFF, Llama},
    translations::Translations,
};
use mathjax::Svg;

/// Página de arXiv con el HTML de un paper.
const HTML_URL: &str = "https://arxiv.org/html/";
/// Intentos por tabla antes de dejarla en el original.
const ATTEMPTS_PER_TABLE: usize = 2;

#[derive(Clone)]
pub struct FullPapers {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    translations: Translations,
    /// Script de MathJax: `ATALAYA_MATHJAX` o [`mathjax::DEFAULT_SCRIPT`].
    script: String,
    running: Mutex<HashMap<String, Running>>,
}

/// Un paper que se está traduciendo: lo que lleva hecho (ADR 0015).
#[derive(Default)]
struct Running {
    parts: Vec<Part>,
    translated: Vec<Option<String>>,
    tables: Vec<String>,
    tables_es: Vec<Option<String>>,
    formulas: Vec<(String, bool)>,
    svgs: Vec<Option<Svg>>,
}

impl Running {
    fn progress(&self) -> Option<Progress> {
        if self.parts.is_empty() {
            return None;
        }
        let done = self.translated.iter().filter(|t| t.is_some()).count()
            + self.tables_es.iter().filter(|t| t.is_some()).count();
        Some(Progress {
            done: done as u32,
            total: (self.translated.len() + self.tables_es.len()) as u32,
        })
    }

    fn html(&self, arxiv_id: &str, partial: bool) -> String {
        let body = if partial {
            content::assemble_partial(&self.parts, &self.translated)
        } else {
            let done: Vec<String> = self
                .translated
                .iter()
                .zip(self.parts.iter().filter_map(|p| match p {
                    Part::Translate(s) => Some(s),
                    Part::Keep(_) => None,
                }))
                .map(|(t, original)| t.clone().unwrap_or_else(|| original.clone()))
                .collect();
            content::assemble(&self.parts, &done)
        };
        let tables: Vec<String> = self
            .tables
            .iter()
            .zip(&self.tables_es)
            .map(|(original, es)| es.clone().unwrap_or_else(|| original.clone()))
            .collect();
        let formulas: Vec<render::FormulaOut> = self
            .formulas
            .iter()
            .zip(&self.svgs)
            .map(|((latex, display), svg)| render::FormulaOut {
                latex,
                display: *display,
                svg: svg.as_ref(),
            })
            .collect();
        // Los identificadores antiguos llevan barra (`hep-th/9901001`): en la
        // ruta de la fórmula va codificada.
        let url_id = arxiv_id.replace('/', "%2F");
        render::put_formulas(&render::put_tables(&body, &tables), &url_id, &formulas)
    }
}

impl FullPapers {
    pub fn new(http: reqwest::Client, translations: Translations) -> Self {
        let script =
            std::env::var("ATALAYA_MATHJAX").unwrap_or_else(|_| mathjax::DEFAULT_SCRIPT.into());
        Self {
            inner: Arc::new(Inner {
                http,
                translations,
                script,
                running: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn running(&self) -> std::sync::MutexGuard<'_, HashMap<String, Running>> {
        self.inner.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Cómo va el paper. Si se está traduciendo, sus párrafos pasan delante en
    /// la cola: quien pregunta lo está leyendo (ADR 0015).
    pub async fn state(&self, arxiv_id: &str) -> Result<FullPaper, String> {
        if let Some(paper) = self.partial(arxiv_id) {
            self.inner.translations.touch_content(queue_id(arxiv_id));
            return Ok(paper);
        }
        let id = arxiv_id.to_owned();
        let row = self
            .inner
            .translations
            .store()
            .call(move |c| db::row(c, &id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(match row {
            Some(r) if r.state == db::STATE_DONE => FullPaper {
                state: FullPaperState::Done,
                html: r.html,
                progress: None,
            },
            Some(r) if r.state == db::STATE_FAILED => only(FullPaperState::Failed),
            Some(r) if r.state == db::STATE_NOT_AVAILABLE => only(FullPaperState::NotAvailable),
            // `pending` sin trabajo en marcha: el servicio se reinició.
            _ => only(FullPaperState::NotRequested),
        })
    }

    /// Pide el paper completo. Si ya está, o en marcha, solo dice cómo va.
    pub async fn request(&self, arxiv_id: &str) -> Result<FullPaper, String> {
        let current = self.state(arxiv_id).await?;
        if matches!(
            current.state,
            FullPaperState::Pending | FullPaperState::Done | FullPaperState::NotAvailable
        ) {
            return Ok(current);
        }
        let started = {
            let mut running = self.running();
            if running.contains_key(arxiv_id) {
                false
            } else {
                running.insert(arxiv_id.to_owned(), Running::default());
                true
            }
        };
        if !started {
            return self.state(arxiv_id).await;
        }
        let id = arxiv_id.to_owned();
        self.inner
            .translations
            .store()
            .call(move |c| db::set_state(c, &id, db::STATE_PENDING, None, None, None))
            .await
            .map_err(|e| e.to_string())?;
        tokio::spawn(self.clone().run(arxiv_id.to_owned()));
        Ok(only(FullPaperState::Pending))
    }

    /// El SVG de una fórmula, para `/formulas/<id>/<n>.svg`.
    pub async fn formula(&self, arxiv_id: &str, n: i64) -> Result<Option<String>, String> {
        if let Some(svg) = self.running().get(arxiv_id).and_then(|r| {
            usize::try_from(n)
                .ok()
                .and_then(|n| r.svgs.get(n).cloned().flatten())
        }) {
            return Ok(Some(svg.svg));
        }
        let id = arxiv_id.to_owned();
        self.inner
            .translations
            .store()
            .call(move |c| db::formula(c, &id, n))
            .await
            .map_err(|e| e.to_string())
    }

    fn partial(&self, arxiv_id: &str) -> Option<FullPaper> {
        let running = self.running();
        let r = running.get(arxiv_id)?;
        let progress = r.progress();
        Some(FullPaper {
            state: FullPaperState::Pending,
            html: progress.map(|_| r.html(arxiv_id, true)),
            progress,
        })
    }

    async fn run(self, arxiv_id: String) {
        let result = self.translate(&arxiv_id).await;
        let final_html = self
            .running()
            .remove(&arxiv_id)
            .filter(|_| matches!(result, Ok(true)))
            .map(|r| r.html(&arxiv_id, false));
        let model = self.inner.translations.llama().model().to_owned();
        let (state, error) = match &result {
            Ok(true) => (db::STATE_DONE, None),
            Ok(false) => (db::STATE_NOT_AVAILABLE, None),
            Err(e) => {
                tracing::warn!(paper = arxiv_id, "no se pudo traducir el paper completo: {e}");
                (db::STATE_FAILED, Some(e.clone()))
            }
        };
        let id = arxiv_id.clone();
        let saved = self
            .inner
            .translations
            .store()
            .call(move |c| {
                db::set_state(c, &id, state, final_html.as_deref(), error.as_deref(), Some(&model))
            })
            .await;
        if let Err(e) = saved {
            tracing::error!(paper = arxiv_id, "no se pudo guardar el paper completo: {e}");
        }
    }

    /// `Ok(false)` si arXiv no tiene HTML de este paper.
    async fn translate(&self, arxiv_id: &str) -> Result<bool, String> {
        let started = std::time::Instant::now();
        let response = self
            .inner
            .http
            .get(format!("{HTML_URL}{arxiv_id}/"))
            .send()
            .await
            .map_err(|e| format!("arXiv no responde: {e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        let page = response
            .error_for_status()
            .map_err(|e| format!("arXiv respondió con error: {e}"))?
            .text()
            .await
            .map_err(|e| format!("no se pudo leer el HTML de arXiv: {e}"))?;
        if !page.contains("ltx_document") {
            return Ok(false);
        }

        let prepared = prep::prepare(&page, HTML_URL);
        let formulas: Vec<(String, bool)> = prepared
            .formulas
            .iter()
            .map(|f| (f.latex.clone(), f.display))
            .collect();
        let svgs = match mathjax::render(&self.inner.script, &formulas).await {
            Ok(svgs) => svgs,
            Err(e) => {
                // Sin SVG, las fórmulas salen como su LaTeX: se puede leer igual.
                tracing::warn!(paper = arxiv_id, "MathJax: {e}");
                vec![None; formulas.len()]
            }
        };
        let (id, to_save) = (arxiv_id.to_owned(), svgs.clone());
        if let Err(e) = self
            .inner
            .translations
            .store()
            .call(move |c| db::save_formulas(c, &id, &to_save))
            .await
        {
            tracing::error!(paper = arxiv_id, "no se pudieron guardar las fórmulas: {e}");
        }

        let (html, tables) = render::extract_tables(&prepared.html);
        let parts = content::split(&html);
        let paragraphs = parts
            .iter()
            .filter(|p| matches!(p, Part::Translate(_)))
            .count();
        {
            let mut running = self.running();
            let r = running.entry(arxiv_id.to_owned()).or_default();
            r.translated = vec![None; paragraphs];
            r.tables_es = vec![None; tables.len()];
            r.tables = tables.clone();
            r.formulas = formulas;
            r.svgs = svgs;
            r.parts = parts.clone();
        }

        // Todo a la cola, en el orden del documento.
        let qid = queue_id(arxiv_id);
        let mut pending = Vec::new();
        let mut index = 0;
        for part in &parts {
            match part {
                Part::Translate(text) => {
                    if prep::has_text_outside_markers(text) {
                        pending.push(Unit::Paragraph(index, self.queue(qid, SegmentKind::Paper, text.clone())));
                    } else {
                        self.record_paragraph(arxiv_id, index, text);
                    }
                    index += 1;
                }
                Part::Keep(keep) => {
                    for k in table_refs(keep) {
                        let Some(table) = tables.get(k) else { continue };
                        let (template, cells) = prep::table_cells(table);
                        if cells.is_empty() {
                            self.record_table(arxiv_id, k, table);
                            continue;
                        }
                        let json = serde_json::to_string(&cells).map_err(|e| e.to_string())?;
                        pending.push(Unit::Table(k, template, self.queue(qid, SegmentKind::TableCells, json)));
                    }
                }
            }
        }

        let mut kept = 0;
        for unit in pending {
            match unit {
                Unit::Paragraph(i, rx) => {
                    let segment = rx.await.map_err(|_| "la cola de traducción se cerró")??;
                    if !segment.translated {
                        kept += 1;
                    }
                    self.record_paragraph(arxiv_id, i, &segment.html);
                }
                Unit::Table(k, template, rx) => {
                    let segment = rx.await.map_err(|_| "la cola de traducción se cerró")??;
                    let cells: Vec<String> =
                        serde_json::from_str(&segment.html).map_err(|e| e.to_string())?;
                    self.record_table(arxiv_id, k, &prep::fill_table(&template, &cells));
                }
            }
        }
        tracing::info!(
            paper = arxiv_id,
            secs = started.elapsed().as_secs_f32(),
            paragraphs,
            tables = tables.len(),
            kept,
            "paper completo traducido"
        );
        Ok(true)
    }

    fn queue(
        &self,
        entry_id: Id,
        kind: SegmentKind,
        html: String,
    ) -> oneshot::Receiver<Result<Segment, String>> {
        let (reply, rx) = oneshot::channel();
        self.inner.translations.queue_segment(SegmentJob {
            entry_id,
            kind,
            html,
            reply,
        });
        rx
    }

    fn record_paragraph(&self, arxiv_id: &str, index: usize, html: &str) {
        if let Some(slot) = self
            .running()
            .get_mut(arxiv_id)
            .and_then(|r| r.translated.get_mut(index))
        {
            *slot = Some(html.to_owned());
        }
    }

    fn record_table(&self, arxiv_id: &str, k: usize, html: &str) {
        if let Some(slot) = self
            .running()
            .get_mut(arxiv_id)
            .and_then(|r| r.tables_es.get_mut(k))
        {
            *slot = Some(html.to_owned());
        }
    }
}

enum Unit {
    Paragraph(usize, oneshot::Receiver<Result<Segment, String>>),
    Table(usize, String, oneshot::Receiver<Result<Segment, String>>),
}

fn only(state: FullPaperState) -> FullPaper {
    FullPaper {
        state,
        html: None,
        progress: None,
    }
}

/// Número de la tabla de cada `<img data-tabla="K">` de un trozo.
fn table_refs(keep: &str) -> Vec<usize> {
    keep.match_indices("data-tabla=\"")
        .filter_map(|(at, m)| {
            keep[at + m.len()..]
                .split('"')
                .next()
                .and_then(|n| n.parse().ok())
        })
        .collect()
}

/// Id en la cola de la GPU: negativo, para no chocar con los de Miniflux.
fn queue_id(arxiv_id: &str) -> Id {
    let mut h = DefaultHasher::new();
    arxiv_id.hash(&mut h);
    -((h.finish() >> 2) as i64) - 1
}

/// Un párrafo de paper: como uno de artículo, y además el LaTeX de cada
/// marca debe volver intacto y sin Markdown (ADR 0010).
pub async fn translate_paragraph(llama: &Llama, html: &str) -> Result<Segment, String> {
    content::translate_segment_with(llama, html, prep::check_markers).await
}

/// Las celdas con palabras de una tabla, todas en una petición: con la tabla
/// a la vista, el modelo acierta más en celdas cortas. Si no vuelven bien,
/// quedan en el original.
pub async fn translate_cells(llama: &Llama, json: &str) -> Result<Segment, String> {
    let cells: Vec<String> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let prompt = prep::cells_prompt(&cells);
    let max_tokens = (json.len() + 256).clamp(256, 8192) as u32;
    for attempt in 1..=ATTEMPTS_PER_TABLE {
        let raw = match llama.complete(&prompt, max_tokens).await {
            Ok(raw) => raw,
            Err(e) if e == CUT_OFF => continue,
            Err(e) => return Err(e),
        };
        if let Some(es) = prep::parse_cells(&raw, cells.len()) {
            return Ok(Segment {
                html: serde_json::to_string(&es).map_err(|e| e.to_string())?,
                translated: true,
            });
        }
        tracing::debug!(attempt, "celdas de tabla rechazadas");
    }
    Ok(Segment {
        html: json.to_owned(),
        translated: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_ids_are_negative_and_stable() {
        let a = queue_id("2610.31646");
        assert!(a < 0);
        assert_eq!(a, queue_id("2610.31646"));
        assert_ne!(a, queue_id("2610.31671"));
    }

    #[test]
    fn finds_table_refs() {
        assert_eq!(table_refs("<p></p><img data-tabla=\"3\"><img data-tabla=\"4\">"), [3, 4]);
    }
}
