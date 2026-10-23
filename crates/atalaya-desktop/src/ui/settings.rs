//! Pantalla 5: ajustes (S3). La hace el paso 5.4; ocupa todo lo que queda a
//! la derecha del menú lateral. Al guardar, debe escribir el archivo con
//! `crate::settings` y sustituir la [`super::Connection`].

use gpui::prelude::*;
use gpui::{Context, Window, div};

use crate::palette::Palette;

#[derive(Default)]
pub struct SettingsScreen;

impl Render for SettingsScreen {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(p.text3)
            .child("Los ajustes llegan en el paso 5.4")
    }
}
