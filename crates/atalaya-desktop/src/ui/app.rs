//! La ventana: el menú lateral a la izquierda y, a su derecha, la pantalla
//! elegida. Aquí solo se conectan las vistas; cada una vive en su archivo.

use atalaya_api::View;
use gpui::prelude::*;
use gpui::{Context, Entity, Subscription, Window, div};
use gpui_component::Theme;

use crate::assets::FONT_FAMILY;
use crate::palette::Palette;
use crate::ui::entry_list::{EntryList, EntryListEvent};
use crate::ui::feeds::FeedsScreen;
use crate::ui::reader::{Reader, ReaderEvent};
use crate::ui::settings::SettingsScreen;
use crate::ui::sidebar::{Sidebar, SidebarEvent, Source};

/// Lo que ocupa la ventana a la derecha del menú.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Lista y lectura (pantallas 1 y 2 de las maquetas).
    Reading,
    /// Gestión de feeds (pantalla 3).
    Feeds,
    /// Ajustes (pantalla 5).
    Settings,
}

pub struct AppView {
    screen: Screen,
    sidebar: Entity<Sidebar>,
    entry_list: Entity<EntryList>,
    reader: Entity<Reader>,
    feeds: Entity<FeedsScreen>,
    settings: Entity<SettingsScreen>,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Las maquetas abren en «Sin leer».
        let initial = Source::View(View::Unread);
        let sidebar = cx.new(|cx| Sidebar::new(initial.clone(), cx));
        let entry_list = cx.new(|cx| EntryList::new(initial, cx));
        let reader = cx.new(|_| Reader::default());
        let feeds = cx.new(|_| FeedsScreen);
        let settings = cx.new(|_| SettingsScreen);

        let subscriptions = vec![
            cx.subscribe(&sidebar, |this, _, event: &SidebarEvent, cx| match event {
                SidebarEvent::SourceSelected(source) => {
                    this.screen = Screen::Reading;
                    this.entry_list
                        .update(cx, |list, cx| list.show(source.clone(), cx));
                    this.reader.update(cx, |reader, cx| reader.clear(cx));
                    cx.notify();
                }
                SidebarEvent::ScreenOpened(screen) => {
                    this.screen = *screen;
                    cx.notify();
                }
            }),
            cx.subscribe(
                &entry_list,
                |this, _, event: &EntryListEvent, cx| match event {
                    EntryListEvent::Selected(entry) => {
                        this.reader
                            .update(cx, |reader, cx| reader.show(entry.clone(), cx));
                    }
                    EntryListEvent::Refreshed => {
                        this.sidebar.update(cx, |sidebar, cx| sidebar.reload(cx));
                    }
                },
            ),
            cx.subscribe(&reader, |this, _, event: &ReaderEvent, cx| match event {
                ReaderEvent::Changed(entry) => {
                    this.entry_list
                        .update(cx, |list, cx| list.update_entry(entry, cx));
                    this.sidebar.update(cx, |sidebar, cx| sidebar.reload(cx));
                }
            }),
            // Tema claro u oscuro según el sistema, también si cambia con la
            // app abierta.
            cx.observe_window_appearance(window, |_, window, cx| {
                Theme::change(window.appearance(), Some(window), cx);
            }),
        ];

        Self {
            screen: Screen::Reading,
            sidebar,
            entry_list,
            reader,
            feeds,
            settings,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let root = div()
            .size_full()
            .flex()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(FONT_FAMILY)
            .text_size(gpui::px(14.))
            .child(self.sidebar.clone());
        match self.screen {
            Screen::Reading => root
                .child(self.entry_list.clone())
                .child(self.reader.clone()),
            Screen::Feeds => root.child(self.feeds.clone()),
            Screen::Settings => root.child(self.settings.clone()),
        }
    }
}
