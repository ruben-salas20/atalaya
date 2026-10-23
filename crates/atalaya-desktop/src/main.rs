//! App de PC de Atalaya: lector RSS que habla con el servicio de `ai-lab`.

mod assets;
mod client;
mod palette;
mod settings;
mod ui;

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{App, Bounds, WindowBounds, WindowDecorations, WindowOptions, px, size};
use gpui_component::{Root, Theme};

use crate::client::ApiClient;
use crate::settings::{FontSize, Settings};
use crate::ui::Connection;
use crate::ui::app::AppView;

fn main() {
    let settings = Settings::read();
    let font_size = settings
        .as_ref()
        .map_or(settings::DEFAULT_FONT_SIZE, |s| s.font_size);
    let connection = settings
        .and_then(Settings::validate)
        .and_then(|s| ApiClient::new(&s))
        .map(Arc::new);
    if let Err(error) = &connection {
        eprintln!("atalaya: {error}");
    }

    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            if let Err(error) = assets::load_fonts(cx) {
                eprintln!("atalaya: no se pudo cargar Roboto: {error}");
            }
            gpui_component::init(cx);
            Theme::update(cx, |theme| theme.font_family = assets::FONT_FAMILY.into());
            cx.set_global(Connection(connection));
            cx.set_global(FontSize(font_size));

            let bounds = Bounds::centered(None, size(px(1200.), px(740.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(500.))),
                // Hyprland no pone barra de título, y las maquetas tampoco.
                window_decorations: Some(WindowDecorations::Server),
                app_id: Some("atalaya".into()),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| {
                Theme::change(window.appearance(), Some(window), cx);
                let view = cx.new(|cx| AppView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("no se pudo abrir la ventana");
            cx.activate(true);
        });
}
