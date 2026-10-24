//! Columna 3 de la pantalla 4: el paper abierto (P4), con
//! `GET /api/papers/{arxiv_id}`, y el botón de guardarlo (P5).
//!
//! Como el lector de artículos: si el resumen se está traduciendo, enseña el
//! original en gris con «Traduciendo…» y lo vuelve a pedir cada pocos
//! segundos. Guardar no se deshace aquí, igual que en Android: se quita
//! desde Guardados.

use std::time::Duration;

use atalaya_api::{EntrySummary, Paper, PaperSummary, TranslationState};
use gpui::prelude::*;
use gpui::{
    Context, ElementId, EventEmitter, FontWeight, Pixels, SharedString, Task, Window, div, px,
    relative,
};
use gpui_component::scroll::ScrollableElement;
use gpui_component::text::TextView;
use gpui_component::tooltip::Tooltip;

use super::format::{abstract_paragraphs, paper_date};
use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::reader::{progress, toast};
use crate::ui::widgets::{bar, icon, icon_button, more_menu};

const POLL_EVERY: Duration = Duration::from_secs(3);

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
                let text = if spanish {
                    paper.translated_abstract.as_deref().unwrap_or_default()
                } else {
                    &paper.abstract_text
                };
                let html: String = abstract_paragraphs(text)
                    .iter()
                    .map(|para| format!("<p>{}</p>", escape(para)))
                    .collect();
                let id = SharedString::from(format!("abstract-{}-{spanish}", summary.arxiv_id));
                div()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text3)
                            .mt(px(6.))
                            .mb(px(8.))
                            .child(if spanish { "RESUMEN" } else { "ABSTRACT" }),
                    )
                    .child(
                        div()
                            .text_size(body_size)
                            .line_height(relative(1.65))
                            .when(pending, |d| d.text_color(p.text3))
                            .child(TextView::html(id, html).selectable(true)),
                    )
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
}
