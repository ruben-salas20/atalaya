//! Archivos que van dentro del ejecutable: la letra Roboto y los iconos.
//!
//! Roboto 2.138 (Apache 2.0, ver `assets/fonts/roboto/LICENSE`) va incluida
//! para no depender de que el sistema la tenga. Roboto Mono (SIL OFL 1.1, ver
//! `assets/fonts/roboto-mono/OFL.txt`) es para el código de los artículos:
//! la letra mono que pide GPUI en Linux, DejaVu Sans Mono, no siempre está. Los iconos son los Material
//! de las maquetas (`docs/diseño/maquetas-pc.html`), también Apache 2.0.

use std::borrow::Cow;

use gpui::{App, AssetSource, Result, SharedString};

/// Nombre de la familia con la que se pide la letra.
pub const FONT_FAMILY: &str = "Roboto";
pub const MONO_FONT_FAMILY: &str = "Roboto Mono";

const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/roboto/Roboto-Regular.ttf"),
    include_bytes!("../assets/fonts/roboto/Roboto-Italic.ttf"),
    include_bytes!("../assets/fonts/roboto/Roboto-Medium.ttf"),
    include_bytes!("../assets/fonts/roboto/Roboto-MediumItalic.ttf"),
    include_bytes!("../assets/fonts/roboto/Roboto-Bold.ttf"),
    include_bytes!("../assets/fonts/roboto/Roboto-BoldItalic.ttf"),
    include_bytes!("../assets/fonts/roboto-mono/RobotoMono-Regular.ttf"),
];

pub fn load_fonts(cx: &App) -> Result<()> {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
}

macro_rules! icons {
    ($($variant:ident => $file:literal),* $(,)?) => {
        /// Iconos de las maquetas. Se pintan con [`crate::ui::widgets::icon`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[allow(dead_code)] // Algunos son para los pasos 5.2 y 5.4.
        pub enum Icon { $($variant),* }

        impl Icon {
            pub fn path(self) -> SharedString {
                match self { $(Icon::$variant => concat!("atalaya/icons/", $file, ".svg").into()),* }
            }
        }

        fn icon_bytes(path: &str) -> Option<&'static [u8]> {
            match path {
                $(concat!("atalaya/icons/", $file, ".svg") =>
                    Some(include_bytes!(concat!("../assets/icons/", $file, ".svg"))),)*
                _ => None,
            }
        }
    };
}

icons! {
    Add => "add",
    Bookmark => "bookmark",
    BookmarkOutline => "bookmark-o",
    Check => "check",
    ChevronRight => "chevron",
    Close => "close",
    DoneAll => "done-all",
    Edit => "edit",
    ExpandMore => "expand",
    History => "history",
    List => "list",
    More => "more",
    Open => "open",
    Pdf => "pdf",
    Refresh => "refresh",
    Rss => "rss",
    Search => "search",
    Translate => "translate",
    Tune => "tune",
    Unread => "unread",
    Warn => "warn",
}

/// Primero los iconos propios; si no, los de gpui-component (Lucide), que
/// usan sus componentes por dentro.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match icon_bytes(path) {
            Some(bytes) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit_assets::Assets.list(path)
    }
}
