//! Columna 3 de la pantalla 4: el paper abierto (P4), con
//! `GET /api/papers/{arxiv_id}`, y el botón de guardarlo (P5).
//!
//! Como el lector de artículos: si el resumen se está traduciendo, enseña el
//! original en gris con «Traduciendo…» y lo vuelve a pedir cada pocos
//! segundos. Guardar no se deshace aquí, igual que en Android: se quita
//! desde Guardados.

use std::sync::Arc;
use std::time::Duration;

use atalaya_api::{EntrySummary, FullPaper, FullPaperState, Paper, PaperSummary, TranslationState};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, ElementId, EventEmitter, FontWeight, HighlightStyle, Hsla,
    ImageSource, Overflow, Pixels, SharedString, SharedUri, StyleRefinement, Task, Window, div,
    img, px, relative,
};
use gpui_base::text::{TextView, TextViewStyle};
use gpui_component::scroll::ScrollableElement;
use gpui_component::tooltip::Tooltip;

use super::format::{abstract_paragraphs, paper_date};
use super::formulas::Formulas;
use super::full::{self, Formula, Part};
use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::format::{code_in_pre, split_pending};
use crate::ui::reader::{progress, toast, translation_progress};
use crate::ui::widgets::{bar, icon, icon_button, more_menu};

const POLL_EVERY: Duration = Duration::from_secs(3);
/// Cuántas fórmulas se bajan a la vez.
const FORMULA_BATCH: usize = 8;

pub enum PaperReaderEvent {
    /// Se guardó: la fila de resultados y el menú deben reflejarlo.
    Saved {
        arxiv_id: String,
        entry: EntrySummary,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Spanish,
    Original,
}

enum Load {
    Loading,
    Ready(Box<Paper>),
    Failed(String),
}

pub struct PaperReader {
    /// La fila elegida: con ella se pinta la cabecera mientras llega el resto.
    summary: Option<PaperSummary>,
    load: Load,
    lang: Lang,
    saving: bool,
    action_error: Option<String>,
    _fetch: Task<()>,
    /// El paper completo (P7, ADR 0010), tal como lo dio el último `GET` o `POST`.
    full: Option<FullPaper>,
    full_error: Option<String>,
    /// Se está pidiendo el paper completo (`POST`), para no pedirlo dos veces.
    requesting: bool,
    formulas: Arc<Formulas>,
    _full: Task<()>,
}

impl EventEmitter<PaperReaderEvent> for PaperReader {}

impl Default for PaperReader {
    fn default() -> Self {
        Self {
            summary: None,
            load: Load::Loading,
            lang: Lang::Spanish,
            saving: false,
            action_error: None,
            _fetch: Task::ready(()),
            full: None,
            full_error: None,
            requesting: false,
            formulas: Arc::new(Formulas::new()),
            _full: Task::ready(()),
        }
    }
}

impl PaperReader {
    pub fn show(&mut self, paper: PaperSummary, cx: &mut Context<Self>) {
        let id = paper.arxiv_id.clone();
        self.summary = Some(paper);
        self.load = Load::Loading;
        self.lang = Lang::Spanish;
        self.saving = false;
        self.action_error = None;
        self.full = None;
        self.full_error = None;
        self.requesting = false;
        self.formulas.forget_failed();
        self.load_full(id.clone(), Duration::ZERO, cx);
        self.fetch(id, Duration::ZERO, cx);
        cx.notify();
    }

    fn is_current(&self, arxiv_id: &str) -> bool {
        self.summary.as_ref().map(|s| s.arxiv_id.as_str()) == Some(arxiv_id)
    }

    fn fetch(&mut self, arxiv_id: String, delay: Duration, cx: &mut Context<Self>) {
        let client = match crate::ui::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.load = Load::Failed(error);
                return;
            }
        };
        self._fetch = cx.spawn(async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let id = arxiv_id.clone();
            let result = cx.background_spawn(async move { client.paper(&id) }).await;
            this.update(cx, |this, cx| {
                if !this.is_current(&arxiv_id) {
                    return;
                }
                match result {
                    Ok(mut paper) => {
                        if paper.translation == TranslationState::Pending {
                            this.fetch(arxiv_id, POLL_EVERY, cx);
                        }
                        // Guardado lo manda la app: el servicio puede tardar
                        // en saberlo si se guardó hace un momento.
                        if let Some(saved) = this.summary.as_ref().and_then(|s| s.saved_entry_id) {
                            paper.summary.saved_entry_id = Some(saved);
                        }
                        this.summary = Some(paper.summary.clone());
                        if let Ok(client) = crate::ui::client(cx) {
                            let urls = abstract_formula_urls(
                                paper.abstract_html.as_deref(),
                                paper.translated_abstract_html.as_deref(),
                                client.service_url(),
                            );
                            this.fetch_formulas(urls, cx);
                        }
                        this.load = Load::Ready(Box::new(paper));
                    }
                    Err(error) => match this.load {
                        Load::Ready(_) => this.fetch(arxiv_id, POLL_EVERY, cx),
                        _ => this.load = Load::Failed(error),
                    },
                }
                cx.notify();
            })
            .ok();
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(summary) = self.summary.clone() else {
            return;
        };
        if summary.saved_entry_id.is_some() || self.saving {
            return;
        }
        let client = match crate::ui::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.action_error = Some(error);
                cx.notify();
                return;
            }
        };
        self.saving = true;
        self.action_error = None;
        cx.notify();
        let arxiv_id = summary.arxiv_id;
        // Sin guardar la tarea: cambiar de paper no debe cortar el guardado.
        cx.spawn(async move |this, cx| {
            let id = arxiv_id.clone();
            let result = cx
                .background_spawn(async move { client.save_paper(&id) })
                .await;
            this.update(cx, |this, cx| {
                let current = this.is_current(&arxiv_id);
                match result {
                    Ok(entry) => {
                        if current {
                            this.saving = false;
                            this.set_saved(entry.id);
                        }
                        cx.emit(PaperReaderEvent::Saved { arxiv_id, entry });
                    }
                    Err(error) => {
                        if current {
                            this.saving = false;
                            this.action_error = Some(format!("No se pudo guardar: {error}"));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn set_saved(&mut self, entry_id: atalaya_api::Id) {
        if let Some(summary) = &mut self.summary {
            summary.saved_entry_id = Some(entry_id);
        }
        if let Load::Ready(paper) = &mut self.load {
            paper.summary.saved_entry_id = Some(entry_id);
        }
    }

    fn translated(&self) -> bool {
        matches!(&self.load, Load::Ready(p) if p.translation == TranslationState::Done
            && p.translated_abstract.is_some())
    }

    fn spanish(&self) -> bool {
        self.translated() && self.lang == Lang::Spanish
    }

    fn top_bar(&self, summary: &PaperSummary, p: &Palette, cx: &mut Context<Self>) -> gpui::Div {
        let translated = self.translated();
        let on = if translated {
            self.lang
        } else {
            Lang::Original
        };
        let saved = summary.saved_entry_id.is_some();
        let abs_url = match &self.load {
            Load::Ready(paper) => paper.abs_url.clone(),
            _ => format!("https://arxiv.org/abs/{}", summary.arxiv_id),
        };
        bar(p)
            .px(px(12.))
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .bg(p.surface)
                    .rounded(px(16.))
                    .p(px(3.))
                    .when(!translated, |d| d.opacity(0.45))
                    .child(self.segment("Español", Lang::Spanish, on, translated, p, cx))
                    .child(self.segment("Original", Lang::Original, on, translated, p, cx)),
            )
            .child(div().flex_1())
            .child(
                icon_button(
                    "paper-save",
                    if saved {
                        Icon::Bookmark
                    } else {
                        Icon::BookmarkOutline
                    },
                    p,
                )
                .tooltip(move |window, cx| {
                    let text = if saved { "En Guardados" } else { "Guardar" };
                    Tooltip::new(text).build(window, cx)
                })
                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            )
            .child(
                icon_button("paper-open", Icon::Open, p)
                    .tooltip(|window, cx| Tooltip::new("Ver en arXiv").build(window, cx))
                    .on_click({
                        let abs_url = abs_url.clone();
                        move |_, _, cx| cx.open_url(&abs_url)
                    }),
            )
            .child(more_menu("paper-more", abs_url))
    }

    fn segment(
        &self,
        label: &'static str,
        lang: Lang,
        on: Lang,
        enabled: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = lang == on;
        div()
            .id(label)
            .text_size(px(13.))
            .px(px(12.))
            .py(px(5.))
            .rounded(px(13.))
            .when(selected, |d| {
                d.bg(p.accent)
                    .text_color(p.on_accent)
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!selected, |d| d.text_color(p.text2))
            .when(enabled && !selected, |d| d.cursor_pointer())
            .child(label)
            .when(enabled, |d| {
                d.on_click(cx.listener(move |this, _, _, cx| {
                    this.lang = lang;
                    cx.notify();
                }))
            })
    }

    /// Botón con borde y un icono pequeño (`.btn-o`).
    fn outline_button(
        id: &'static str,
        glyph: Icon,
        label: &'static str,
        url: String,
        p: &Palette,
    ) -> impl IntoElement {
        let hover = p.surface;
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .h(px(36.))
            .px(px(14.))
            .rounded(px(18.))
            .border_1()
            .border_color(p.divider)
            .text_size(px(13.5))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(icon(glyph, px(16.), p.text))
            .child(label)
            .on_click(move |_, _, cx| cx.open_url(&url))
    }

    /// «Guardar», relleno (`.save-btn`). Una vez guardado queda como
    /// «Guardado» y ya no se pulsa.
    fn save_button(&self, saved: bool, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let (bg, fg) = if saved {
            (p.surface2, p.text)
        } else {
            (p.accent, p.on_accent)
        };
        let label = match (saved, self.saving) {
            (true, _) => "Guardado",
            (false, true) => "Guardando…",
            (false, false) => "Guardar",
        };
        div()
            .id("paper-save-button")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .h(px(42.))
            .px(px(20.))
            .rounded(px(21.))
            .bg(bg)
            .text_color(fg)
            .text_size(px(14.5))
            .font_weight(FontWeight::MEDIUM)
            .when(!saved && !self.saving, |d| d.cursor_pointer())
            .child(icon(
                if saved {
                    Icon::Bookmark
                } else {
                    Icon::BookmarkOutline
                },
                px(16.),
                fg,
            ))
            .child(label)
            .on_click(cx.listener(|this, _, _, cx| this.save(cx)))
    }

    /// Lee el paper completo (`GET`). Si el servicio lo está traduciendo,
    /// lo vuelve a pedir cada pocos segundos (ADR 0015).
    fn load_full(&mut self, arxiv_id: String, delay: Duration, cx: &mut Context<Self>) {
        let client = match crate::ui::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.full_error = Some(error);
                return;
            }
        };
        self._full = cx.spawn(async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let id = arxiv_id.clone();
            let result = cx
                .background_spawn(async move { client.full_paper(&id) })
                .await;
            this.update(cx, |this, cx| {
                if this.is_current(&arxiv_id) {
                    this.receive_full(arxiv_id, result, cx);
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// Pide el paper completo (`POST`), que lo pone en marcha si hace falta,
    /// y sigue su avance.
    fn request_full(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.summary.as_ref().map(|s| s.arxiv_id.clone()) else {
            return;
        };
        let client = match crate::ui::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.full_error = Some(error);
                cx.notify();
                return;
            }
        };
        self.requesting = true;
        self.full_error = None;
        self._full = cx.spawn(async move |this, cx| {
            let rid = id.clone();
            let result = cx
                .background_spawn(async move { client.request_full_paper(&rid) })
                .await;
            this.update(cx, |this, cx| {
                if this.is_current(&id) {
                    this.receive_full(id, result, cx);
                }
                cx.notify();
            })
            .ok();
        });
        cx.notify();
    }

    fn receive_full(
        &mut self,
        arxiv_id: String,
        result: Result<FullPaper, String>,
        cx: &mut Context<Self>,
    ) {
        self.requesting = false;
        match result {
            Ok(full) => {
                let pending = full.state == FullPaperState::Pending;
                self.full_error = None;
                if let (Some(html), Ok(client)) = (full.html.as_deref(), crate::ui::client(cx)) {
                    let urls = full::formula_urls(html, client.service_url());
                    self.fetch_formulas(urls, cx);
                }
                self.full = Some(full);
                if pending {
                    self.load_full(arxiv_id, POLL_EVERY, cx);
                }
            }
            Err(error) => {
                // Un fallo de red mientras se traduce no para la lectura.
                if self
                    .full
                    .as_ref()
                    .is_some_and(|f| f.state == FullPaperState::Pending)
                {
                    self.load_full(arxiv_id, POLL_EVERY, cx);
                } else {
                    self.full_error = Some(error);
                }
            }
        }
    }

    /// Baja en segundo plano las fórmulas que aún no están, de a lotes. Cada
    /// lote avisa al terminar para que el texto se repinte.
    fn fetch_formulas(&self, urls: Vec<String>, cx: &mut Context<Self>) {
        let missing = self.formulas.take_missing(urls);
        if missing.is_empty() {
            return;
        }
        let Ok(client) = crate::ui::client(cx) else {
            return;
        };
        let formulas = self.formulas.clone();
        cx.spawn(async move |this, cx| {
            for batch in missing.chunks(FORMULA_BATCH) {
                let tasks: Vec<_> = batch
                    .iter()
                    .map(|url| {
                        let client = client.clone();
                        let url = url.clone();
                        cx.background_spawn(async move {
                            let svg = client.formula_svg(&url);
                            (url, svg)
                        })
                    })
                    .collect();
                for task in tasks {
                    let (url, svg) = task.await;
                    match svg {
                        Ok(svg) => formulas.loaded(&url, svg),
                        Err(_) => formulas.failed(&url),
                    }
                }
                this.update(cx, |_, cx| cx.notify()).ok();
            }
        })
        .detach();
    }

    /// El texto completo, debajo del resumen: pedirlo, su avance y el HTML del
    /// servicio con sus fórmulas (ADR 0010).
    fn full_section(
        &self,
        arxiv_id: &str,
        p: &Palette,
        body_size: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(full) = self.full.as_ref() else {
            return match &self.full_error {
                Some(error) => note(error, p).into_any_element(),
                None => div().into_any_element(),
            };
        };
        let mut section = div().flex().flex_col().mt(px(36.)).child(
            div()
                .text_size(px(12.))
                .text_color(p.text3)
                .mb(px(10.))
                .child("TEXTO COMPLETO"),
        );
        if let Some(error) = &self.full_error {
            section = section.child(note(error, p).mb(px(10.)));
        }
        match full.state {
            FullPaperState::NotRequested => {
                section = section.child(self.full_action("Leer completo", p, cx))
            }
            FullPaperState::NotAvailable => {
                section = section.child(note(
                    "arXiv no tiene versión HTML de este paper. Quedan el resumen y el PDF.",
                    p,
                ))
            }
            FullPaperState::Failed => {
                section = section
                    .child(note("No se pudo traducir el texto completo.", p).mb(px(10.)))
                    .child(self.full_action("Reintentar", p, cx))
            }
            FullPaperState::Pending | FullPaperState::Done => {
                if full.state == FullPaperState::Pending {
                    let bar = match full.progress {
                        Some(avance) => translation_progress(avance, p).into_any_element(),
                        None => progress(p).into_any_element(),
                    };
                    section = section.child(div().mb(px(10.)).child(bar));
                }
                if let Some(html) = full.html.as_deref() {
                    let id = format!("full-{arxiv_id}");
                    section = section.children(self.html_parts(&id, html, false, p, body_size, cx));
                }
            }
        }
        section.into_any_element()
    }

    /// Botón relleno de «Leer completo» y «Reintentar». Mientras se pide, no
    /// responde.
    fn full_action(&self, label: &'static str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.requesting;
        div()
            .id("paper-full-action")
            .flex()
            .flex_none()
            .items_center()
            .h(px(42.))
            .px(px(20.))
            .rounded(px(21.))
            .bg(p.accent)
            .text_color(p.on_accent)
            .text_size(px(14.5))
            .font_weight(FontWeight::MEDIUM)
            .child(if busy { "Pidiendo…" } else { label })
            .when(!busy, |d| {
                d.cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.request_full(cx)))
            })
            .into_any_element()
    }

    /// Los trozos de un HTML del servicio (el paper completo o el resumen), en
    /// orden: texto con sus fórmulas en línea, y las fórmulas aparte,
    /// centradas. `id` da nombre a los textos; `dimmed` los pinta en gris.
    fn html_parts(
        &self,
        id: &str,
        html: &str,
        dimmed: bool,
        p: &Palette,
        body_size: Pixels,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Ok(client) = crate::ui::client(cx) else {
            return Vec::new();
        };
        let base = client.service_url().to_owned();
        let font_px = f32::from(body_size);
        let ex_px = font_px * full::EX_PER_FONT;
        let color = hex(p.text);
        let style = text_style(cx);
        let mut out = Vec::new();
        let mut n = 0;
        for piece in split_pending(html) {
            for part in full::parts(&piece.html, &base, font_px) {
                n += 1;
                match part {
                    Part::Text(text) => {
                        let text_id = SharedString::from(format!("{id}-{n}"));
                        let formulas = self.formulas.clone();
                        let color = color.clone();
                        let view = TextView::html(text_id, code_in_pre(&text))
                            .selectable(true)
                            .style(style.clone())
                            .image_source(move |uri: &SharedUri| {
                                // Solo las fórmulas salen del registro; las
                                // figuras de arXiv se bajan como cualquier
                                // imagen (#88).
                                if full::is_formula_url(uri) {
                                    ImageSource::Image(formulas.image(uri, &color, ex_px))
                                } else {
                                    uri.clone().into()
                                }
                            });
                        let view = if piece.pending || dimmed {
                            view.text_color(p.text3)
                        } else {
                            view
                        };
                        out.push(
                            div()
                                .text_size(body_size)
                                .line_height(relative(1.65))
                                .child(view)
                                .into_any_element(),
                        );
                    }
                    Part::Block(formula) => {
                        out.push(self.formula_block(&formula, &color, ex_px, p))
                    }
                }
            }
        }
        out
    }

    /// Una fórmula aparte: su imagen centrada, con el número de la ecuación si lo tiene.
    fn formula_block(&self, formula: &Formula, color: &str, ex_px: f32, p: &Palette) -> AnyElement {
        let image = self.formulas.image(&formula.url, color, ex_px);
        div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(16.))
            .py(px(8.))
            .child(
                img(ImageSource::Image(image))
                    .w(px(formula.width))
                    .h(px(formula.height))
                    .flex_none(),
            )
            .when_some(formula.number.clone(), |d, number| {
                d.child(div().text_color(p.text2).child(number))
            })
            .into_any_element()
    }

    fn article(
        &self,
        summary: &PaperSummary,
        p: &Palette,
        body_size: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let paper = match &self.load {
            Load::Ready(paper) => Some(paper.as_ref()),
            _ => None,
        };
        let spanish = self.spanish();
        let pending = paper.is_some_and(|p| p.translation == TranslationState::Pending);
        let not_needed = paper.is_some_and(|p| p.translation == TranslationState::NotNeeded);
        // Igual que en Android: el título en español va con el resumen en
        // español; el original, con el original.
        let title = if spanish || not_needed || paper.is_none() || !summary.title_translated {
            summary.title.clone()
        } else {
            summary.original_title.clone()
        };

        let header = div()
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(p.text2)
                    .mb(px(10.))
                    .child(format!(
                        "arXiv:{} · {} · {}",
                        summary.arxiv_id,
                        summary.category,
                        paper_date(summary.published_at)
                    )),
            )
            .child(
                div()
                    .text_size(px(28.))
                    .line_height(relative(1.25))
                    .font_weight(FontWeight::MEDIUM)
                    .mb(px(14.))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(relative(1.45))
                    .text_color(p.text2)
                    .mb(px(12.))
                    .child(summary.authors.join(", ")),
            );

        let note = match paper.map(|p| p.translation) {
            Some(TranslationState::Done) if spanish => Some((Icon::Translate, "Traducido")),
            Some(TranslationState::Failed) => Some((Icon::Warn, "No se pudo traducir")),
            _ => None,
        }
        .map(|(glyph, text)| {
            div().flex().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(p.text3)
                    .border_1()
                    .border_color(p.divider)
                    .rounded(px(14.))
                    .pl(px(8.))
                    .pr(px(10.))
                    .py(px(3.))
                    .mb(px(22.))
                    .child(icon(glyph, px(14.), p.text3))
                    .child(text),
            )
        });

        let body = match (&self.load, paper) {
            (_, Some(paper)) => {
                // Con fórmulas, el resumen viene en HTML y se pinta como el
                // paper completo (#80). Sin ellas, el texto plano de siempre.
                let rich = if spanish {
                    paper.translated_abstract_html.as_deref()
                } else {
                    paper.abstract_html.as_deref()
                };
                let id = format!("abstract-{}-{spanish}", summary.arxiv_id);
                let text = match rich {
                    Some(html) => div()
                        .children(self.html_parts(&id, html, pending, p, body_size, cx))
                        .into_any_element(),
                    None => {
                        let plain = if spanish {
                            paper.translated_abstract.as_deref().unwrap_or_default()
                        } else {
                            &paper.abstract_text
                        };
                        let html: String = abstract_paragraphs(plain)
                            .iter()
                            .map(|para| format!("<p>{}</p>", escape(para)))
                            .collect();
                        div()
                            .text_size(body_size)
                            .line_height(relative(1.65))
                            .when(pending, |d| d.text_color(p.text3))
                            .child(TextView::html(SharedString::from(id), html).selectable(true))
                            .into_any_element()
                    }
                };
                div()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text3)
                            .mt(px(6.))
                            .mb(px(8.))
                            .child(if spanish { "RESUMEN" } else { "ABSTRACT" }),
                    )
                    .child(text)
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(10.))
                            .mt(px(8.))
                            .child(self.save_button(summary.saved_entry_id.is_some(), p, cx))
                            .child(Self::outline_button(
                                "paper-abs",
                                Icon::Open,
                                "Ver en arXiv",
                                paper.abs_url.clone(),
                                p,
                            ))
                            .child(Self::outline_button(
                                "paper-pdf",
                                Icon::Pdf,
                                "Abrir PDF",
                                paper.pdf_url.clone(),
                                p,
                            )),
                    )
                    .child(self.full_section(&summary.arxiv_id, p, body_size, cx))
                    .into_any_element()
            }
            (Load::Failed(error), None) => div()
                .text_size(px(14.))
                .text_color(p.text2)
                .child(error.clone())
                .into_any_element(),
            _ => div()
                .text_size(px(14.))
                .text_color(p.text3)
                .child("Cargando…")
                .into_any_element(),
        };

        div()
            .id(ElementId::Name(
                format!("paper-{}", summary.arxiv_id).into(),
            ))
            .flex_1()
            .min_h_0()
            .child(
                div().py(px(32.)).child(
                    div()
                        .max_w(px(640.))
                        .mx_auto()
                        .px(px(32.))
                        .child(header)
                        .children(note)
                        .child(body),
                ),
            )
            .overflow_y_scrollbar()
    }
}

/// Un texto corto de aviso, en gris.
fn note(text: &str, p: &Palette) -> gpui::Div {
    div()
        .text_size(px(13.))
        .text_color(p.text2)
        .child(text.to_owned())
}

/// El color del tema en `#rrggbb`, para cambiar `currentColor` del SVG.
fn hex(color: Hsla) -> String {
    let rgb = color.to_rgb();
    let channel = |c: f32| (c * 255.).round().clamp(0., 255.) as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgb.r),
        channel(rgb.g),
        channel(rgb.b)
    )
}

/// El estilo del visor de texto, con las tablas en su propio desplazamiento:
/// una tabla ancha se recorre con su barra y no corta el texto (ADR 0010).
/// Es el de `gpui-component` (`base_text_view_style`), con esa única
/// diferencia.
fn text_style(cx: &App) -> TextViewStyle {
    let theme = gpui_component::Theme::global(cx);
    let radius = theme.semantic_tokens().radius.md;
    let mut table = StyleRefinement::default();
    table.overflow.x = Some(Overflow::Scroll);
    table.corner_radii.top_left = Some(radius.into());
    table.corner_radii.top_right = Some(radius.into());
    table.corner_radii.bottom_left = Some(radius.into());
    table.corner_radii.bottom_right = Some(radius.into());
    let code_block = StyleRefinement {
        corner_radii: table.corner_radii.clone(),
        ..Default::default()
    };
    let table_head = StyleRefinement::default()
        .bg(theme.table_head)
        .text_color(theme.table_head_foreground);
    TextViewStyle::default()
        .with_foreground(theme.foreground)
        .with_muted_foreground(theme.muted_foreground)
        .with_link(theme.link)
        .with_selection(theme.selection)
        .with_code_background(theme.muted)
        .with_border(theme.border)
        .with_code_block(code_block)
        .with_table(table)
        .with_table_head(table_head)
        .with_inline_code(HighlightStyle {
            background_color: Some(theme.accent),
            ..Default::default()
        })
        .with_dark(theme.is_dark())
}

/// Las fórmulas que hay que bajar de un resumen, en original y en español. Las
/// dos suelen usar las mismas direcciones; el registro de descargas no repite.
fn abstract_formula_urls(
    original: Option<&str>,
    translated: Option<&str>,
    base: &str,
) -> Vec<String> {
    [original, translated]
        .into_iter()
        .flatten()
        .flat_map(|html| full::formula_urls(html, base))
        .collect()
}

/// El resumen llega en texto plano y se pinta como HTML: `<` o `&` en un
/// resumen (fórmulas, «A&B») no deben leerse como etiquetas.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Render for PaperReader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let column = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .relative();
        let Some(summary) = self.summary.clone() else {
            return column.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.text3)
                    .child("Busca y elige un paper"),
            );
        };
        let pending =
            matches!(&self.load, Load::Ready(p) if p.translation == TranslationState::Pending);
        column
            .child(
                self.top_bar(&summary, &p, cx)
                    .when(pending, |d| d.border_b_0()),
            )
            .when(pending, |d| d.child(progress(&p)))
            .when_some(self.action_error.clone(), |d, error| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(16.))
                        .py(px(8.))
                        .text_size(px(13.))
                        .text_color(p.text2)
                        .bg(p.surface)
                        .child(error),
                )
            })
            .child(self.article(&summary, &p, crate::settings::font_size(cx), cx))
            .when(pending, |d| d.child(toast(&p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup() {
        assert_eq!(escape("A<B & C>D"), "A&lt;B &amp; C&gt;D");
    }

    #[test]
    fn abstract_formulas_come_from_both_languages() {
        let original = "<p><img class=\"atalaya-formula\" src=\"/formulas/hep-th%2F9901001/r0.svg\" alt=\"a\"></p>";
        let translated = "<p><img class=\"atalaya-formula\" src=\"/formulas/hep-th%2F9901001/r1.svg\" alt=\"b\"></p>";
        assert_eq!(
            abstract_formula_urls(Some(original), Some(translated), "http://s"),
            [
                "http://s/formulas/hep-th%2F9901001/r0.svg",
                "http://s/formulas/hep-th%2F9901001/r1.svg",
            ]
        );
        assert_eq!(
            abstract_formula_urls(Some(original), None, "http://s").len(),
            1
        );
        assert!(abstract_formula_urls(None, None, "http://s").is_empty());
    }
}
