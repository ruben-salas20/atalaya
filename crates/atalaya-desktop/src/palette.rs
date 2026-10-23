//! Colores de las maquetas: negro, blanco y grises, en claro u oscuro según
//! el sistema. Los nombres son los de las variables CSS de
//! `docs/diseño/maquetas-pc.html`.

use gpui::{Hsla, Window, WindowAppearance, rgb};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Hsla,
    pub side: Hsla,
    pub surface: Hsla,
    pub surface2: Hsla,
    pub text: Hsla,
    pub text2: Hsla,
    pub text3: Hsla,
    pub divider: Hsla,
    pub accent: Hsla,
    #[allow(dead_code)] // Lo usa el selector «Español» y «Original» del paso 5.2.
    pub on_accent: Hsla,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            bg: rgb(0xffffff).into(),
            side: rgb(0xf6f6f6).into(),
            surface: rgb(0xf0f0f0).into(),
            surface2: rgb(0xe6e6e6).into(),
            text: rgb(0x111111).into(),
            text2: rgb(0x555555).into(),
            text3: rgb(0x8c8c8c).into(),
            divider: rgb(0xe6e6e6).into(),
            accent: rgb(0x111111).into(),
            on_accent: rgb(0xffffff).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg: rgb(0x121212).into(),
            side: rgb(0x171717).into(),
            surface: rgb(0x1f1f1f).into(),
            surface2: rgb(0x2a2a2a).into(),
            text: rgb(0xf1f1f1).into(),
            text2: rgb(0xb0b0b0).into(),
            text3: rgb(0x7c7c7c).into(),
            divider: rgb(0x262626).into(),
            accent: rgb(0xf1f1f1).into(),
            on_accent: rgb(0x111111).into(),
        }
    }

    /// La paleta que toca según el tema del sistema.
    pub fn of(window: &Window) -> Self {
        match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
        }
    }
}
