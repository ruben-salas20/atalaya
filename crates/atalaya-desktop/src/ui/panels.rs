//! Qué paneles caben en la pantalla de lectura: menú lateral, lista y lector.
//!
//! El lector necesita al menos [`READER_MIN`] de ancho. Si la ventana no da
//! para los tres, se pliega solo primero el menú y luego la lista. Mientras,
//! la lista se estrecha hasta [`LIST_MIN`]. El usuario también los pliega con
//! un botón o con Ctrl+B y Ctrl+Shift+B, y eso se guarda en los ajustes.

use gpui::prelude::*;
use gpui::{Div, actions, div, px};
use gpui_component::tooltip::Tooltip;

use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::widgets::icon_button;

actions!(atalaya, [ToggleSidebar, ToggleList]);

pub const SIDEBAR_WIDTH: f32 = 240.;
pub const LIST_WIDTH: f32 = 360.;
pub const LIST_MIN: f32 = 300.;
pub const READER_MIN: f32 = 600.;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Panels {
    /// Lo que eligió el usuario. Se guarda en los ajustes.
    pub sidebar_folded: bool,
    pub list_folded: bool,
    /// Abierto a mano aunque no quepa. Se olvida cuando cambia lo que cabe.
    sidebar_peek: bool,
    list_peek: bool,
}

/// Ancho de cada panel en una ventana dada. `None` si está plegado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub sidebar: Option<f32>,
    pub list: Option<f32>,
}

fn sidebar_fits(width: f32) -> bool {
    width >= SIDEBAR_WIDTH + LIST_MIN + READER_MIN
}

fn list_fits(rest: f32) -> bool {
    rest >= LIST_MIN + READER_MIN
}

impl Panels {
    pub fn new(sidebar_folded: bool, list_folded: bool) -> Self {
        Self {
            sidebar_folded,
            list_folded,
            ..Default::default()
        }
    }

    pub fn layout(&self, width: f32) -> Layout {
        let sidebar = !self.sidebar_folded && (self.sidebar_peek || sidebar_fits(width));
        let rest = if sidebar {
            width - SIDEBAR_WIDTH
        } else {
            width
        };
        let list = !self.list_folded && (self.list_peek || list_fits(rest));
        Layout {
            sidebar: sidebar.then_some(SIDEBAR_WIDTH),
            list: list.then(|| (rest - READER_MIN).clamp(LIST_MIN, LIST_WIDTH)),
        }
    }

    pub fn toggle_sidebar(&mut self, width: f32) {
        if self.layout(width).sidebar.is_some() {
            self.sidebar_folded = true;
            self.sidebar_peek = false;
        } else {
            self.sidebar_folded = false;
            self.sidebar_peek = !sidebar_fits(width);
        }
    }

    pub fn toggle_list(&mut self, width: f32) {
        let layout = self.layout(width);
        if layout.list.is_some() {
            self.list_folded = true;
            self.list_peek = false;
        } else {
            let rest = width - layout.sidebar.unwrap_or(0.);
            self.list_folded = false;
            self.list_peek = !list_fits(rest);
        }
    }

    /// La ventana cambió de ancho. Lo abierto a mano se pliega si cambia lo
    /// que cabe; si no, un cambio pequeño de tamaño lo cerraría sin motivo.
    pub fn resized(&mut self, before: f32, after: f32) {
        if sidebar_fits(before) != sidebar_fits(after) {
            self.sidebar_peek = false;
        }
        let rest = |w: f32| {
            if sidebar_fits(w) {
                w - SIDEBAR_WIDTH
            } else {
                w
            }
        };
        if list_fits(rest(before)) != list_fits(rest(after)) {
            self.list_peek = false;
        }
    }
}

/// Botones para plegar el menú y la lista, al principio de la barra del
/// lector. Lanzan las mismas acciones que los atajos; las recoge `AppView`.
pub fn buttons(p: &Palette) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(2.))
        .child(
            icon_button("toggle-sidebar", Icon::Menu, p)
                .tooltip(|window, cx| Tooltip::new("Menú lateral (Ctrl+B)").build(window, cx))
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleSidebar), cx)),
        )
        .child(
            icon_button("toggle-list", Icon::List, p)
                .tooltip(|window, cx| {
                    Tooltip::new("Lista de artículos (Ctrl+Shift+B)").build(window, cx)
                })
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleList), cx)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_window_shows_all() {
        let layout = Panels::default().layout(1440.);
        assert_eq!(layout.sidebar, Some(SIDEBAR_WIDTH));
        assert_eq!(layout.list, Some(LIST_WIDTH));
    }

    #[test]
    fn half_screen_folds_sidebar_first() {
        // Media pantalla de un monitor de 1920 px, con algo de margen.
        for width in [960., 940.] {
            let layout = Panels::default().layout(width);
            assert_eq!(layout.sidebar, None);
            let list = layout.list.unwrap();
            assert!(width - list >= READER_MIN, "{width}: lista de {list}");
        }
    }

    #[test]
    fn list_shrinks_before_sidebar_folds() {
        let layout = Panels::default().layout(1160.);
        assert_eq!(layout.sidebar, Some(SIDEBAR_WIDTH));
        assert_eq!(layout.list, Some(1160. - SIDEBAR_WIDTH - READER_MIN));
    }

    #[test]
    fn narrow_window_folds_list_too() {
        let layout = Panels::default().layout(700.);
        assert_eq!(
            layout,
            Layout {
                sidebar: None,
                list: None
            }
        );
    }

    #[test]
    fn toggle_folds_and_peeks() {
        let mut panels = Panels::default();
        panels.toggle_sidebar(1440.);
        assert!(panels.sidebar_folded);
        assert_eq!(panels.layout(1440.).sidebar, None);
        panels.toggle_sidebar(1440.);
        assert_eq!(panels.layout(1440.).sidebar, Some(SIDEBAR_WIDTH));

        // Abrir a mano lo que no cabe: se ve hasta que cambia lo que cabe.
        panels.toggle_sidebar(960.);
        assert!(!panels.sidebar_folded);
        assert_eq!(panels.layout(960.).sidebar, Some(SIDEBAR_WIDTH));
        panels.resized(960., 950.);
        assert_eq!(panels.layout(950.).sidebar, Some(SIDEBAR_WIDTH));
        panels.resized(950., 1440.);
        panels.resized(1440., 950.);
        assert_eq!(panels.layout(950.).sidebar, None);
    }

    #[test]
    fn toggle_list() {
        let mut panels = Panels::default();
        panels.toggle_list(960.);
        assert!(panels.list_folded);
        assert_eq!(panels.layout(960.).list, None);
        panels.toggle_list(960.);
        assert_eq!(panels.layout(960.).list, Some(LIST_WIDTH));
    }
}
