//! Columna 3: el artículo abierto. Hoy solo muestra la cabecera de la fila
//! elegida; el paso 5.2 pide el artículo y añade el selector «Español» y
//! «Original», guardar y marcar como leído (L1–L6).

use atalaya_api::EntrySummary;
use chrono::Utc;
use gpui::prelude::*;
use gpui::{Context, FontWeight, Window, div, px, relative};

use crate::palette::Palette;
use crate::ui::format::ago;
use crate::ui::widgets::bar;

#[derive(Default)]
pub struct Reader {
    entry: Option<EntrySummary>,
}

impl Reader {
    pub fn show(&mut self, entry: EntrySummary, cx: &mut Context<Self>) {
        self.entry = Some(entry);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.entry = None;
        cx.notify();
    }
}

impl Render for Reader {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let column = div().flex_1().min_w_0().h_full().flex().flex_col();
        let Some(entry) = &self.entry else {
            return column.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.text3)
                    .child("Elige un artículo de la lista"),
            );
        };
        column.child(bar(&p).px(px(12.))).child(
            div().py(px(32.)).child(
                div()
                    .max_w(px(640.))
                    .mx_auto()
                    .px(px(32.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text2)
                            .mb(px(10.))
                            .child(format!(
                                "{} · hace {}",
                                entry.feed_title,
                                ago(entry.published_at, Utc::now())
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(28.))
                            .line_height(relative(1.25))
                            .font_weight(FontWeight::MEDIUM)
                            .child(entry.title.clone()),
                    ),
            ),
        )
    }
}
