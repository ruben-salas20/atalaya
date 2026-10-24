//! Piezas de interfaz que se repiten, con las medidas de las maquetas.

use gpui::{
    Anchor, ClipboardItem, Div, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement,
    Pixels, Stateful, Styled, Svg, div, px, svg,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};

use crate::assets::Icon;
use crate::palette::Palette;

pub fn icon(icon: Icon, size: Pixels, color: Hsla) -> Svg {
    svg()
        .path(icon.path())
        .size(size)
        .flex_none()
        .text_color(color)
}

/// Botón cuadrado de 34 px con un icono de 20 (`.icon-btn`).
pub fn icon_button(id: impl Into<ElementId>, glyph: Icon, p: &Palette) -> Stateful<Div> {
    let hover = p.surface;
    div()
        .id(id)
        .size(px(34.))
        .flex_none()
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(glyph, px(20.), p.text2))
}

/// Botón «⋮» del final de una barra, con las opciones que no tienen botón
/// propio. Por ahora solo «Copiar enlace».
pub fn more_menu(id: impl Into<ElementId>, url: String) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .icon(gpui_component::Icon::empty().path(Icon::More.path()))
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            let url = url.clone();
            menu.item(
                PopupMenuItem::new("Copiar enlace").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()))
                }),
            )
        })
}

/// Barra superior de una columna, de 52 px (`.bar`).
pub fn bar(p: &Palette) -> Div {
    div()
        .h(px(52.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.))
        .border_b_1()
        .border_color(p.divider)
}

/// Etiqueta del idioma original (`.badge`).
pub fn badge(text: impl Into<gpui::SharedString>, p: &Palette) -> impl IntoElement {
    div()
        .flex_none()
        .text_size(px(10.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(p.text3)
        .border_1()
        .border_color(p.text3)
        .rounded(px(4.))
        .px(px(4.))
        .line_height(px(14.))
        .child(text.into())
}
