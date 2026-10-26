//! «Texto completo» de un paper de arXiv (P7, ADR 0010): pedirlo con «Leer
//! completo», el avance mientras se traduce y el HTML con sus fórmulas y
//! figuras. Lo usan el lector de papers y el de un artículo que es un paper
//! de arXiv (#90), así que el estado, el sondeo y el pintado son los mismos.
//!
//! - `chunks.rs`: el HTML partido en trozos, guardado para no partirlo en cada
//!   pintado (#89).
//! - `formulas.rs`: las fórmulas ya bajadas, listas como imagen.
//! - `full.rs`: qué hay en un HTML del servicio (texto, fórmulas aparte y sus
//!   direcciones).

pub mod chunks;
pub mod formulas;
pub mod full;

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use atalaya_api::{FullPaper, FullPaperState};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, FontWeight, HighlightStyle, Hsla, ImageSource, Overflow, Pixels,
    SharedUri, StyleRefinement, Task, Window, div, img, px, relative,
};
use gpui_base::text::{TextView, TextViewStyle};

use self::chunks::{Chunk, ChunkCache};
use self::formulas::Formulas;
use self::full::{EX_PER_FONT, Formula};
use crate::palette::Palette;
use crate::ui::reader::{POLL_EVERY, progress, translation_progress};

/// Cuántas fórmulas se bajan a la vez.
const FORMULA_BATCH: usize = 8;

/// Mientras el servicio traduce, hay que volver a pedir el paper.
pub fn keeps_polling(state: FullPaperState) -> bool {
    state == FullPaperState::Pending
}

/// Un fallo de red mientras se traducía no para la lectura: se vuelve a pedir.
/// Con cualquier otro estado, el fallo se enseña.
pub fn retries_after_error(previous: Option<FullPaperState>) -> bool {
    previous.is_some_and(keeps_polling)
}

/// El título de la sección.
pub const TITLE: &str = "TEXTO COMPLETO";

/// El aviso de un estado que no tiene botón, si hay algo que decir.
pub fn note_text(state: FullPaperState) -> Option<&'static str> {
    match state {
        FullPaperState::NotAvailable => {
            Some("arXiv no tiene versión HTML de este paper. Quedan el resumen y el PDF.")
        }
        FullPaperState::Failed => Some("No se pudo traducir el texto completo."),
        _ => None,
    }
}

/// El botón de la sección: «Leer completo» si nadie lo ha pedido, «Reintentar»
/// tras un fallo. Mientras se pide, «Pidiendo…». `None` si el estado no tiene
/// botón.
pub fn action_text(state: FullPaperState, requesting: bool) -> Option<&'static str> {
    let label = match state {
        FullPaperState::NotRequested => "Leer completo",
        FullPaperState::Failed => "Reintentar",
        _ => return None,
    };
    Some(if requesting { "Pidiendo…" } else { label })
}

/// El texto completo de un paper: lo que dijo el servicio, el aviso de un
/// fallo y el HTML pintado. Cada lector tiene uno y lo pinta debajo de su
/// resumen o su artículo.
pub struct FullText {
    /// El paper al que corresponde lo de abajo. Sin paper, no hay sección.
    arxiv_id: Option<String>,
    /// El paper completo tal como lo dio el último `GET` o `POST`.
    full: Option<FullPaper>,
    error: Option<String>,
    /// Se está pidiendo el paper (`POST`), para no pedirlo dos veces.
    requesting: bool,
    formulas: Arc<Formulas>,
    /// Los trozos del HTML ya partidos, para no partirlo en cada render (#89).
    chunks: RefCell<ChunkCache>,
    _task: Task<()>,
}

impl Default for FullText {
    fn default() -> Self {
        Self {
            arxiv_id: None,
            full: None,
            error: None,
            requesting: false,
            formulas: Arc::new(Formulas::new()),
            chunks: RefCell::default(),
            _task: Task::ready(()),
        }
    }
}

impl FullText {
    /// Otro paper, o ninguno (`None`): olvida el anterior y, con un id, lee su
    /// estado del servicio.
    pub fn open(&mut self, arxiv_id: Option<String>, cx: &mut Context<Self>) {
        self.full = None;
        self.error = None;
        self.requesting = false;
        self.formulas.forget_failed();
        self.chunks.get_mut().clear();
        self._task = Task::ready(());
        self.arxiv_id = arxiv_id.clone();
        if let Some(id) = arxiv_id {
            self.load(id, Duration::ZERO, cx);
        }
        cx.notify();
    }

    /// Como `open`, pero si ya es el paper abierto no hace nada. El lector de
    /// artículos lo llama cada vez que llega el artículo.
    pub fn ensure(&mut self, arxiv_id: Option<String>, cx: &mut Context<Self>) {
        if self.arxiv_id != arxiv_id {
            self.open(arxiv_id, cx);
        }
    }

    /// Lee el estado del paper (`GET`) tras `delay`. Si sigue traduciéndose,
    /// se vuelve a llamar sola.
    fn load(&mut self, arxiv_id: String, delay: Duration, cx: &mut Context<Self>) {
        let client = match crate::ui::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        self._task = cx.spawn(async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let id = arxiv_id.clone();
            let result = cx
                .background_spawn(async move { client.full_paper(&id) })
                .await;
            this.update(cx, |this, cx| {
                if this.arxiv_id.as_deref() == Some(arxiv_id.as_str()) {
                    this.receive(arxiv_id, result, cx);
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// «Leer completo» y «Reintentar»: pide el paper (`POST`), que lo pone en
    /// marcha si hace falta, y sigue su avance.
    pub fn request(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.arxiv_id.clone() else {
            return;
        };
        let client = match crate::ui::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.requesting = true;
        self.error = None;
        self._task = cx.spawn(async move |this, cx| {
            let rid = id.clone();
            let result = cx
                .background_spawn(async move { client.request_full_paper(&rid) })
                .await;
            this.update(cx, |this, cx| {
                if this.arxiv_id.as_deref() == Some(id.as_str()) {
                    this.receive(id, result, cx);
                }
                cx.notify();
            })
            .ok();
        });
        cx.notify();
    }

    fn receive(
        &mut self,
        arxiv_id: String,
        result: Result<FullPaper, String>,
        cx: &mut Context<Self>,
    ) {
        self.requesting = false;
        match result {
            Ok(full) => {
                self.error = None;
                if let (Some(html), Ok(client)) = (full.html.as_deref(), crate::ui::client(cx)) {
                    let urls = full::formula_urls(html, client.service_url());
                    self.fetch_formulas(urls, cx);
                }
                let polling = keeps_polling(full.state);
                self.full = Some(full);
                if polling {
                    self.load(arxiv_id, POLL_EVERY, cx);
                }
            }
            Err(error) => {
                if retries_after_error(self.full.as_ref().map(|f| f.state)) {
                    self.load(arxiv_id, POLL_EVERY, cx);
                } else {
                    self.error = Some(error);
                }
            }
        }
    }

    /// Baja en segundo plano las fórmulas que aún no están, de a lotes. Cada
    /// lote avisa al terminar para que el texto se repinte.
    pub fn fetch_formulas(&self, urls: Vec<String>, cx: &mut Context<Self>) {
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

    /// La sección «Texto completo»: pedirlo, su avance y el HTML del servicio
    /// con sus fórmulas (ADR 0010).
    fn section(
        &self,
        arxiv_id: &str,
        p: &Palette,
        body_size: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(full) = self.full.as_ref() else {
            return match &self.error {
                Some(error) => note(error, p).into_any_element(),
                None => div().into_any_element(),
            };
        };
        let mut section = div().flex().flex_col().mt(px(36.)).child(
            div()
                .text_size(px(12.))
                .text_color(p.text3)
                .mb(px(10.))
                .child(TITLE),
        );
        if let Some(error) = &self.error {
            section = section.child(note(error, p).mb(px(10.)));
        }
        match full.state {
            FullPaperState::NotRequested => section = section.child(self.action(full.state, p, cx)),
            FullPaperState::NotAvailable => {
                section = section.children(note_text(full.state).map(|text| note(text, p)))
            }
            FullPaperState::Failed => {
                section = section
                    .children(note_text(full.state).map(|text| note(text, p).mb(px(10.))))
                    .child(self.action(full.state, p, cx))
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

    /// Botón relleno de «Leer completo», «Reintentar» o «Pidiendo…». Mientras
    /// se pide, no responde.
    fn action(&self, state: FullPaperState, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(label) = action_text(state, self.requesting) else {
            return div().into_any_element();
        };
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
            .child(label)
            .when(!busy, |d| {
                d.cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.request(cx)))
            })
            .into_any_element()
    }

    /// Los trozos de un HTML del servicio (el paper completo o el resumen), en
    /// orden: texto con sus fórmulas en línea, y las fórmulas aparte,
    /// centradas. `id` da nombre a los textos; `dimmed` los pinta en gris.
    /// Los trozos vienen de la caché: el HTML solo se parte si cambia (#89).
    pub fn html_parts(
        &self,
        id: &str,
        html: &str,
        dimmed: bool,
        p: &Palette,
        body_size: Pixels,
        cx: &App,
    ) -> Vec<AnyElement> {
        let Ok(client) = crate::ui::client(cx) else {
            return Vec::new();
        };
        let font_px = f32::from(body_size);
        let ex_px = font_px * EX_PER_FONT;
        let chunks = self
            .chunks
            .borrow_mut()
            .get(id, html, client.service_url(), font_px);
        let color = hex(p.text);
        let style = text_style(cx);
        chunks
            .iter()
            .map(|chunk| match chunk {
                Chunk::Text { id, html, pending } => {
                    let formulas = self.formulas.clone();
                    let color = color.clone();
                    let view = TextView::html(id.clone(), html.clone())
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
                    let view = if *pending || dimmed {
                        view.text_color(p.text3)
                    } else {
                        view
                    };
                    div()
                        .text_size(body_size)
                        .line_height(relative(1.65))
                        .child(view)
                        .into_any_element()
                }
                Chunk::Block(formula) => self.formula_block(formula, &color, ex_px, p),
            })
            .collect()
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
}

impl Render for FullText {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(arxiv_id) = self.arxiv_id.clone() else {
            return div().into_any_element();
        };
        let p = Palette::of(window);
        let body_size = crate::settings::font_size(cx);
        self.section(&arxiv_id, &p, body_size, cx)
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
pub fn hex(color: Hsla) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls_only_while_the_service_translates() {
        assert!(keeps_polling(FullPaperState::Pending));
        for state in [
            FullPaperState::NotRequested,
            FullPaperState::NotAvailable,
            FullPaperState::Done,
            FullPaperState::Failed,
        ] {
            assert!(!keeps_polling(state), "{state:?} no debe sondear");
        }
    }

    #[test]
    fn a_network_error_is_retried_only_while_translating() {
        assert!(retries_after_error(Some(FullPaperState::Pending)));
        assert!(!retries_after_error(None));
        assert!(!retries_after_error(Some(FullPaperState::Done)));
        assert!(!retries_after_error(Some(FullPaperState::Failed)));
    }

    #[test]
    fn notes_say_what_is_missing() {
        assert_eq!(
            note_text(FullPaperState::NotAvailable),
            Some("arXiv no tiene versión HTML de este paper. Quedan el resumen y el PDF.")
        );
        assert_eq!(
            note_text(FullPaperState::Failed),
            Some("No se pudo traducir el texto completo.")
        );
        assert_eq!(note_text(FullPaperState::NotRequested), None);
        assert_eq!(note_text(FullPaperState::Pending), None);
    }

    #[test]
    fn the_button_asks_or_retries_and_shows_it_is_busy() {
        assert_eq!(
            action_text(FullPaperState::NotRequested, false),
            Some("Leer completo")
        );
        assert_eq!(
            action_text(FullPaperState::Failed, false),
            Some("Reintentar")
        );
        assert_eq!(
            action_text(FullPaperState::NotRequested, true),
            Some("Pidiendo…")
        );
        assert_eq!(action_text(FullPaperState::Failed, true), Some("Pidiendo…"));
        assert_eq!(action_text(FullPaperState::Pending, false), None);
        assert_eq!(action_text(FullPaperState::Done, false), None);
        assert_eq!(action_text(FullPaperState::NotAvailable, false), None);
    }
}
