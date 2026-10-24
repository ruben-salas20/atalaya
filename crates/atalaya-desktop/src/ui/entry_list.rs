//! Columna 2: la lista de artículos (L2, L3), con `GET /api/entries`.

use std::time::Duration;

use atalaya_api::{EntryPage, EntrySummary, Id};
use chrono::Utc;
use gpui::prelude::*;
use gpui::{
    Context, ElementId, Entity, EventEmitter, FocusHandle, FontWeight, KeyDownEvent, Subscription,
    Task, Window, div, px, relative,
};
use gpui_component::Sizable;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;

use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::format::{ago, lang_badge};
use crate::ui::sidebar::Source;
use crate::ui::widgets::{badge, bar, icon, icon_button};

/// Pausa tras la última tecla antes de buscar.
const SEARCH_AFTER: Duration = Duration::from_millis(300);

pub enum EntryListEvent {
    Selected(EntrySummary),
    /// Se pulsó «Actualizar»: el menú lateral también debe recargarse.
    Refreshed,
}

enum Load {
    Idle,
    Loading,
    Failed(String),
}

pub struct EntryList {
    source: Source,
    entries: Vec<EntrySummary>,
    /// Cuántos hay en total en esta vista, no solo los cargados.
    total: u64,
    selected: Option<Id>,
    load: Load,
    _load: Task<()>,
    /// Campo de búsqueda, abierto con la lupa o con Ctrl+F.
    search: Option<Search>,
    /// Texto que se buscó por última vez. Vacío: lista normal.
    applied: String,
    _debounce: Task<()>,
    /// Lo que tenía el foco antes de abrir la búsqueda, para devolvérselo al cerrar.
    return_focus: Option<FocusHandle>,
}

/// El campo abierto y su suscripción a lo que se teclea.
struct Search {
    input: Entity<InputState>,
    _subscription: Subscription,
}

impl EventEmitter<EntryListEvent> for EntryList {}

impl EntryList {
    pub fn new(source: Source, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            source,
            entries: Vec::new(),
            total: 0,
            selected: None,
            load: Load::Idle,
            _load: Task::ready(()),
            search: None,
            applied: String::new(),
            _debounce: Task::ready(()),
            return_focus: None,
        };
        this.fetch(0, cx);
        this
    }

    /// Cambia de lista y carga su primera página. Cierra la búsqueda.
    pub fn show(&mut self, source: Source, cx: &mut Context<Self>) {
        self.source = source;
        self.entries.clear();
        self.total = 0;
        self.selected = None;
        self.search = None;
        self.applied.clear();
        self.fetch(0, cx);
    }

    /// Abre el campo de búsqueda, o lo cierra si ya está abierto.
    pub fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            self.close_search(window, cx);
            return;
        }
        self.return_focus = window.focused(cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Buscar en la lista"));
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if let InputEvent::Change = event {
                this.schedule_search(cx);
            }
        });
        // El campo aún no se ha dibujado: el foco se le da después, o se pierde.
        let field = input.clone();
        window.defer(cx, move |window, cx| {
            field.update(cx, |input, cx| input.focus(window, cx));
        });
        self.search = Some(Search {
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Cierra el campo y vuelve a la lista completa.
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        if let Some(focus) = self.return_focus.take() {
            window.focus(&focus, cx);
        }
        self._debounce = Task::ready(());
        if !self.applied.is_empty() {
            self.applied.clear();
            self.refresh(cx);
        }
        cx.notify();
    }

    /// Espera a que se deje de teclear y luego busca lo escrito.
    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self._debounce = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_AFTER).await;
            this.update(cx, |this, cx| this.apply_search(cx)).ok();
        });
    }

    fn apply_search(&mut self, cx: &mut Context<Self>) {
        let Some(search) = &self.search else {
            return;
        };
        let text = search.input.read(cx).value().trim().to_owned();
        if text != self.applied {
            self.applied = text;
            self.refresh(cx);
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.entries.clear();
        self.fetch(0, cx);
    }

    /// «Marcar todo como leído» de la lista abierta. Al acabar, recarga la
    /// lista y avisa para que el menú recargue sus números.
    fn mark_all_read(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.source.mark_all_scope() else {
            return;
        };
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.load = Load::Failed(error);
                cx.notify();
                return;
            }
        };
        self._load = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.mark_all_read(&scope) })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(()) => {
                    this.refresh(cx);
                    cx.emit(EntryListEvent::Refreshed);
                }
                Err(error) => {
                    this.load = Load::Failed(format!("No se pudo marcar: {error}"));
                    cx.notify();
                }
            })
            .ok();
        });
    }

    /// Pide la página que empieza en `offset` y la añade a la lista.
    fn fetch(&mut self, offset: u32, cx: &mut Context<Self>) {
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.load = Load::Failed(error);
                cx.notify();
                return;
            }
        };
        let mut query = self.source.query();
        query.offset = Some(offset);
        if !self.applied.is_empty() {
            query.q = Some(self.applied.clone());
        }
        self.load = Load::Loading;
        cx.notify();
        self._load = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.entries(&query) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(EntryPage { total, entries }) => {
                        if offset == 0 {
                            this.entries.clear();
                        }
                        this.total = total;
                        this.entries.extend(entries);
                        this.load = Load::Idle;
                    }
                    Err(error) => this.load = Load::Failed(error),
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// Cambia leído o guardado de una fila, sin recargar: la fila se queda
    /// aunque ya no encaje en la vista, para no mover la lista al leer.
    pub fn update_entry(&mut self, summary: &EntrySummary, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == summary.id) {
            entry.read = summary.read;
            entry.starred = summary.starred;
            cx.notify();
        }
    }

    fn select(&mut self, entry: EntrySummary, cx: &mut Context<Self>) {
        self.selected = Some(entry.id);
        cx.emit(EntryListEvent::Selected(entry));
        cx.notify();
    }

    fn row(&self, entry: &EntrySummary, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected == Some(entry.id);
        let read = entry.read;
        let meta = format!(
            "{} · {}",
            entry.feed_title,
            ago(entry.published_at, Utc::now())
        );
        let hover = p.surface;
        let clicked = entry.clone();
        div()
            .id(ElementId::NamedInteger("entry".into(), entry.id as u64))
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(4.))
            .px(px(18.))
            .py(px(11.))
            .border_b_1()
            .border_color(p.divider)
            .cursor_pointer()
            .when(selected, |d| {
                d.bg(p.surface).child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(3.))
                        .bg(p.accent),
                )
            })
            .when(!selected, |d| d.hover(move |s| s.bg(hover)))
            .child(
                div()
                    .text_size(px(14.5))
                    .line_height(relative(1.35))
                    .line_clamp(2)
                    .text_ellipsis()
                    .when(read, |d| d.text_color(p.text3))
                    .when(!read, |d| d.font_weight(FontWeight::MEDIUM))
                    .child(entry.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(if read { p.text3 } else { p.text2 })
                    .child(div().min_w_0().truncate().child(meta))
                    .when(entry.starred, |d| {
                        d.child(icon(Icon::Bookmark, px(13.), p.text3))
                    })
                    .child(div().flex_1())
                    .when_some(lang_badge(entry.lang.as_deref()), |d, lang| {
                        d.child(badge(lang, p))
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select(clicked.clone(), cx)))
    }

    /// Mensaje en el hueco de la lista: cargando, error o vacía.
    fn notice(&self, p: &Palette) -> Option<impl IntoElement> {
        let text = match &self.load {
            Load::Loading if self.entries.is_empty() => "Cargando…".to_owned(),
            Load::Failed(error) => error.clone(),
            Load::Idle if self.entries.is_empty() => "No hay artículos".to_owned(),
            _ => return None,
        };
        Some(
            div()
                .px(px(18.))
                .py(px(16.))
                .text_size(px(13.5))
                .text_color(p.text2)
                .child(text),
        )
    }

    fn more_button(&self, p: &Palette, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if (self.entries.len() as u64) >= self.total || self.entries.is_empty() {
            return None;
        }
        let loading = matches!(self.load, Load::Loading);
        let offset = self.entries.len() as u32;
        let hover = p.surface;
        Some(
            div()
                .id("more")
                .flex_none()
                .py(px(14.))
                .flex()
                .justify_center()
                .text_size(px(13.5))
                .text_color(p.text2)
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(if loading {
                    "Cargando…"
                } else {
                    "Cargar más"
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !matches!(this.load, Load::Loading) {
                        this.fetch(offset, cx);
                    }
                })),
        )
    }
}

impl Render for EntryList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let rows: Vec<_> = self
            .entries
            .iter()
            .map(|entry| self.row(entry, &p, cx).into_any_element())
            .collect();
        let title = match &self.search {
            Some(search) => div()
                .flex_1()
                .min_w_0()
                .child(Input::new(&search.input).small().cleanable(true))
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_baseline()
                .gap(px(6.))
                .child(
                    div()
                        .truncate()
                        .text_size(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(self.source.title()),
                )
                .when(self.total > 0, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(px(13.))
                            .text_color(p.text3)
                            .child(self.total.to_string()),
                    )
                })
                .into_any_element(),
        };
        div()
            .w(px(360.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .min_w_0()
            .border_r_1()
            .border_color(p.divider)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                if keys.key == "escape" && this.search.is_some() {
                    this.close_search(window, cx);
                }
            }))
            .child(
                bar(&p)
                    .pl(px(18.))
                    .pr(px(10.))
                    .child(title)
                    .child(icon_button("search", Icon::Search, &p).on_click(
                        cx.listener(|this, _, window, cx| this.toggle_search(window, cx)),
                    ))
                    .child(
                        icon_button("refresh", Icon::Refresh, &p).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.refresh(cx);
                                cx.emit(EntryListEvent::Refreshed);
                            },
                        )),
                    )
                    .when(self.source.mark_all_scope().is_some(), |d| {
                        d.child(
                            icon_button("done-all", Icon::DoneAll, &p)
                                .on_click(cx.listener(|this, _, _, cx| this.mark_all_read(cx))),
                        )
                    }),
            )
            .child(
                div()
                    .id("entries")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .children(self.notice(&p))
                    .children(self.more_button(&p, cx))
                    .overflow_y_scrollbar(),
            )
    }
}
