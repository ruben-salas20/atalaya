//! La ventana: el menú lateral a la izquierda y, a su derecha, la pantalla
//! elegida. Aquí solo se conectan las vistas; cada una vive en su archivo.

use atalaya_api::View;
use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, Subscription, Window, div, px};
use gpui_component::Theme;

use crate::assets::FONT_FAMILY;
use crate::palette::Palette;
use crate::settings::{FontSize, Settings};
use crate::ui::Connection;
use crate::ui::entry_list::{EntryList, EntryListEvent};
use crate::ui::feeds::{FeedsEvent, FeedsScreen};
use crate::ui::panels::{Panels, ToggleList, ToggleSidebar};
use crate::ui::papers::reader::{PaperReader, PaperReaderEvent};
use crate::ui::papers::search::{PaperSearch, PaperSearchEvent};
use crate::ui::reader::{Reader, ReaderEvent};
use crate::ui::settings::SettingsScreen;
use crate::ui::sidebar::{Sidebar, SidebarEvent, Source};

/// Lo que ocupa la ventana a la derecha del menú.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Lista y lectura (pantallas 1 y 2 de las maquetas).
    Reading,
    /// Buscar papers (pantalla 4).
    Papers,
    /// Gestión de feeds (pantalla 5).
    Feeds,
    /// Ajustes (pantalla 3).
    Settings,
}

pub struct AppView {
    screen: Screen,
    /// Qué columnas de la pantalla de lectura están plegadas.
    panels: Panels,
    /// Ancho de la ventana en el último cambio de tamaño.
    width: f32,
    /// Sin foco, GPUI solo manda las teclas a la raíz de la ventana y los
    /// atajos de los paneles no llegarían aquí.
    focus: FocusHandle,
    sidebar: Entity<Sidebar>,
    entry_list: Entity<EntryList>,
    reader: Entity<Reader>,
    feeds: Entity<FeedsScreen>,
    paper_search: Entity<PaperSearch>,
    paper_reader: Entity<PaperReader>,
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
        let feeds = cx.new(|cx| FeedsScreen::new(window, cx));
        let settings = cx.new(|cx| SettingsScreen::new(window, cx));
        let paper_search = cx.new(|cx| PaperSearch::new(window, cx));
        let paper_reader = cx.new(|_| PaperReader::default());

        let subscriptions = vec![
            cx.observe_global::<FontSize>(|this, cx| {
                this.reader.update(cx, |_, cx| cx.notify());
            }),
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
                    if *screen == Screen::Feeds {
                        this.feeds.update(cx, |feeds, cx| feeds.reload(cx));
                    }
                    cx.notify();
                }
            }),
            // Al abrir «Buscar papers», el cursor va al buscador.
            cx.subscribe_in(
                &sidebar,
                window,
                |this, _, event: &SidebarEvent, window, cx| {
                    if let SidebarEvent::ScreenOpened(Screen::Papers) = event {
                        this.paper_search
                            .update(cx, |search, cx| search.focus(window, cx));
                    }
                },
            ),
            cx.subscribe(
                &paper_search,
                |this, _, event: &PaperSearchEvent, cx| match event {
                    PaperSearchEvent::Selected(paper) => {
                        this.paper_reader
                            .update(cx, |reader, cx| reader.show(paper.clone(), cx));
                    }
                },
            ),
            cx.subscribe(
                &paper_reader,
                |this, _, event: &PaperReaderEvent, cx| match event {
                    PaperReaderEvent::Saved { arxiv_id, entry } => {
                        this.paper_search
                            .update(cx, |search, cx| search.mark_saved(arxiv_id, entry.id, cx));
                        this.sidebar.update(cx, |sidebar, cx| sidebar.reload(cx));
                    }
                },
            ),
            cx.subscribe(&feeds, |this, _, event: &FeedsEvent, cx| match event {
                FeedsEvent::Changed => {
                    this.sidebar.update(cx, |sidebar, cx| sidebar.reload(cx));
                }
            }),
            // Los ajustes guardaron otra conexión: todo se vuelve a pedir.
            cx.observe_global::<Connection>(|this, cx| {
                this.sidebar.update(cx, |sidebar, cx| sidebar.reload(cx));
                this.entry_list.update(cx, |list, cx| list.refresh(cx));
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
            cx.observe_window_bounds(window, |this, window, cx| {
                let width = f32::from(window.viewport_size().width);
                this.panels.resized(this.width, width);
                this.width = width;
                cx.notify();
            }),
        ];

        let saved = Settings::read().unwrap_or_default();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            focus,
            screen: Screen::Reading,
            panels: Panels::new(saved.sidebar_folded, saved.list_folded),
            width: f32::from(window.viewport_size().width),
            sidebar,
            entry_list,
            reader,
            feeds,
            paper_search,
            paper_reader,
            settings,
            _subscriptions: subscriptions,
        }
    }
}

impl AppView {
    /// Pliega o despliega una columna y guarda la elección. Solo en la
    /// pantalla de lectura: en las demás el menú es la única forma de salir.
    fn toggle_panel(&mut self, change: impl FnOnce(&mut Panels, f32), cx: &mut Context<Self>) {
        if self.screen != Screen::Reading {
            return;
        }
        change(&mut self.panels, self.width);
        let (sidebar_folded, list_folded) = (self.panels.sidebar_folded, self.panels.list_folded);
        if let Err(error) = Settings::update(|s| {
            s.sidebar_folded = sidebar_folded;
            s.list_folded = list_folded;
        }) {
            eprintln!("atalaya: {error}");
        }
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let layout = self.panels.layout(self.width);
        let root =
            div()
                .size_full()
                .flex()
                .bg(p.bg)
                .text_color(p.text)
                .font_family(FONT_FAMILY)
                .text_size(gpui::px(14.))
                .track_focus(&self.focus)
                .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                    this.toggle_panel(Panels::toggle_sidebar, cx)
                }))
                .on_action(cx.listener(|this, _: &ToggleList, _, cx| {
                    this.toggle_panel(Panels::toggle_list, cx)
                }))
                .when(
                    self.screen != Screen::Reading || layout.sidebar.is_some(),
                    |d| d.child(self.sidebar.clone()),
                );
        match self.screen {
            Screen::Reading => root
                .when_some(layout.list, |d, width| {
                    d.child(
                        div()
                            .w(px(width))
                            .h_full()
                            .flex_none()
                            .min_w_0()
                            .child(self.entry_list.clone()),
                    )
                })
                .child(self.reader.clone()),
            Screen::Papers => root
                .child(self.paper_search.clone())
                .child(self.paper_reader.clone()),
            Screen::Feeds => root.child(self.feeds.clone()),
            Screen::Settings => root.child(self.settings.clone()),
        }
    }
}
