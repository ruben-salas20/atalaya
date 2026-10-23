//! Pantalla 3: gestión de feeds (F1–F6). La hace el paso 5.4; ocupa todo lo
//! que queda a la derecha del menú lateral.

use gpui::prelude::*;
use gpui::{Context, Window, div};

use crate::palette::Palette;

#[derive(Default)]
pub struct FeedsScreen;

impl Render for FeedsScreen {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(p.text3)
            .child("La gestión de feeds llega en el paso 5.4")
    }
}
