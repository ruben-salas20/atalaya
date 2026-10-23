//! Columna 1: el menú lateral (L1), con los números de `GET /api/sidebar`.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use atalaya_api::{EntryQuery, Id, Sidebar as SidebarData, View};
use gpui::prelude::*;
use gpui::{Context, ElementId, EventEmitter, FontWeight, SharedString, Task, Window, div, px};

use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::app::Screen;
use crate::ui::widgets::icon;

/// De dónde sale la lista de artículos.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    View(View),
    Category { id: Id, title: SharedString },
    Feed { id: Id, title: SharedString },
}

impl Source {
    pub fn title(&self) -> SharedString {
        match self {
            Source::View(View::All) => "Todos".into(),
            Source::View(View::Unread) => "Sin leer".into(),
            Source::View(View::Starred) => "Guardados".into(),
            Source::Category { title, .. } | Source::Feed { title, .. } => title.clone(),
        }
    }

    /// Consulta de la primera página. Una carpeta o un feed muestran todos
    /// sus artículos, leídos o no.
    pub fn query(&self) -> EntryQuery {
        match self {
            Source::View(view) => EntryQuery {
                view: *view,
                ..Default::default()
            },
            Source::Category { id, .. } => EntryQuery {
                category_id: Some(*id),
                ..Default::default()
            },
            Source::Feed { id, .. } => EntryQuery {
                feed_id: Some(*id),
                ..Default::default()
            },
        }
    }
}

pub enum SidebarEvent {
    SourceSelected(Source),
    ScreenOpened(Screen),
}

/// Qué entrada del menú está marcada.
#[derive(Debug, Clone, PartialEq)]
enum Active {
    Source(Source),
    Screen(Screen),
}

enum Sync {
    Loading,
    /// Cuándo respondió el servicio por última vez al pedir el menú.
    Done(Instant),
    /// Sin red: el menú y las listas salen de la copia en disco (L8).
    Offline,
    Failed(String),
}

/// Cada cuánto se repinta «Sincronizado (N min)» para que avance solo.
const SYNC_TICK: Duration = Duration::from_secs(15);

/// Texto de S2: «Sincronizado (N min)», con los minutos enteros, o
/// «Sincronizado (ahora)» antes del primer minuto.
fn synced_label(elapsed: Duration) -> String {
    match elapsed.as_secs() / 60 {
        0 => "Sincronizado (ahora)".to_owned(),
        minutes => format!("Sincronizado ({minutes} min)"),
    }
}

pub struct Sidebar {
    data: Option<SidebarData>,
    active: Active,
    collapsed: HashSet<Id>,
    sync: Sync,
    _load: Task<()>,
    _tick: Task<()>,
}

impl EventEmitter<SidebarEvent> for Sidebar {}

impl Sidebar {
    pub fn new(initial: Source, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            data: None,
            active: Active::Source(initial),
            collapsed: HashSet::new(),
            sync: Sync::Loading,
            _load: Task::ready(()),
            _tick: cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(SYNC_TICK).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }),
        };
        this.reload(cx);
        this
    }

    /// Vuelve a pedir los números al servicio.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.sync = Sync::Failed(error);
                cx.notify();
                return;
            }
        };
        self._load = cx.spawn(async move |this, cx| {
            let fetch = client.clone();
            let result = cx.background_spawn(async move { fetch.sidebar() }).await;
            let offline = client.is_offline();
            if result.is_ok() && !offline {
                // Cada vez que se sincroniza, los guardados nuevos bajan a
                // disco. No se espera: tarda lo que tarde traducirlos.
                cx.background_spawn(async move { client.cache_starred() })
                    .detach();
            }
            this.update(cx, |this, cx| {
                match result {
                    Ok(data) => {
                        this.data = Some(data);
                        this.sync = if offline {
                            Sync::Offline
                        } else {
                            Sync::Done(Instant::now())
                        };
                    }
                    Err(error) => this.sync = Sync::Failed(error),
                }
                cx.notify();
            })
            .ok();
        });
    }

    fn select(&mut self, source: Source, cx: &mut Context<Self>) {
        self.active = Active::Source(source.clone());
        cx.emit(SidebarEvent::SourceSelected(source));
        cx.notify();
    }

    fn open(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.active = Active::Screen(screen);
        cx.emit(SidebarEvent::ScreenOpened(screen));
        cx.notify();
    }

    fn toggle(&mut self, category: Id, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&category) {
            self.collapsed.insert(category);
        }
        cx.notify();
    }

    /// Una fila del menú (`.s-item`). Sin nada delante (`leading`) es la de
    /// un feed dentro de su carpeta: más baja, con sangría y en gris.
    fn item(
        &self,
        id: impl Into<ElementId>,
        leading: Option<gpui::AnyElement>,
        label: SharedString,
        count: Option<u64>,
        active: bool,
        p: &Palette,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = p.surface;
        let sub = leading.is_none();
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .h(px(if sub { 30. } else { 34. }))
            .pl(px(if sub { 44. } else { 14. }))
            .pr(px(12.))
            .mx(px(8.))
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(if sub { p.text2 } else { p.text })
            .when(active, |d| d.bg(p.surface2).font_weight(FontWeight::MEDIUM))
            .when(!active, |d| d.hover(move |s| s.bg(hover)))
            .children(leading)
            .child(div().flex_1().min_w_0().truncate().child(label))
            .when_some(count, |d, n| {
                d.child(
                    div()
                        .flex_none()
                        .text_size(px(12.5))
                        .text_color(p.text2)
                        .child(n.to_string()),
                )
            })
    }

    fn view_item(
        &self,
        view: View,
        glyph: Icon,
        count: Option<u64>,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let source = Source::View(view);
        let active = self.active == Active::Source(source.clone());
        self.item(
            SharedString::from(format!("view-{view:?}")),
            Some(icon(glyph, px(20.), p.text2).into_any_element()),
            source.title(),
            count,
            active,
            p,
        )
        .on_click(cx.listener(move |this, _, _, cx| this.select(source.clone(), cx)))
    }

    fn screen_item(
        &self,
        screen: Screen,
        glyph: Icon,
        label: &'static str,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.active == Active::Screen(screen);
        self.item(
            SharedString::from(format!("screen-{screen:?}")),
            Some(icon(glyph, px(20.), p.text2).into_any_element()),
            label.into(),
            None,
            active,
            p,
        )
        .on_click(cx.listener(move |this, _, _, cx| this.open(screen, cx)))
    }

    fn categories(&self, p: &Palette, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let Some(data) = &self.data else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for category in &data.categories {
            let collapsed = self.collapsed.contains(&category.id);
            let source = Source::Category {
                id: category.id,
                title: category.title.clone().into(),
            };
            let active = self.active == Active::Source(source.clone());
            let cat_id = category.id;
            let toggle = div()
                .id(ElementId::NamedInteger("toggle".into(), cat_id as u64))
                .flex_none()
                .child(icon(
                    if collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ExpandMore
                    },
                    px(20.),
                    p.text2,
                ))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle(cat_id, cx);
                }));
            rows.push(
                self.item(
                    ElementId::NamedInteger("category".into(), cat_id as u64),
                    Some(toggle.into_any_element()),
                    source.title(),
                    Some(category.unread),
                    active,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.select(source.clone(), cx)))
                .into_any_element(),
            );
            if collapsed {
                continue;
            }
            for feed in &category.feeds {
                let source = Source::Feed {
                    id: feed.id,
                    title: feed.title.clone().into(),
                };
                let active = self.active == Active::Source(source.clone());
                rows.push(
                    self.item(
                        ElementId::NamedInteger("feed".into(), feed.id as u64),
                        None,
                        source.title(),
                        Some(feed.unread),
                        active,
                        p,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.select(source.clone(), cx)))
                    .into_any_element(),
                );
            }
        }
        rows
    }

    fn sync_line(&self, p: &Palette) -> impl IntoElement {
        let text: SharedString = match &self.sync {
            Sync::Loading => "Sincronizando…".into(),
            Sync::Done(at) => synced_label(at.elapsed()).into(),
            Sync::Offline => "Sin conexión".into(),
            Sync::Failed(error) => error.clone().into(),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(22.))
            .pt(px(6.))
            .pb(px(2.))
            .text_size(px(12.))
            .text_color(p.text3)
            .child(div().flex_none().size(px(7.)).rounded_full().bg(p.text2))
            .child(div().min_w_0().line_clamp(2).text_ellipsis().child(text))
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let counts = self.data.as_ref().map(|d| (d.total, d.unread, d.starred));
        let categories = self.categories(&p, cx);
        div()
            .w(px(240.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(p.side)
            .border_r_1()
            .border_color(p.divider)
            .pt(px(14.))
            .pb(px(10.))
            .child(
                div()
                    .px(px(20.))
                    .pt(px(4.))
                    .pb(px(14.))
                    .text_size(px(18.))
                    .font_weight(FontWeight::MEDIUM)
                    .child("Atalaya"),
            )
            .child(self.view_item(View::All, Icon::List, counts.map(|c| c.0), &p, cx))
            .child(self.view_item(View::Unread, Icon::Unread, counts.map(|c| c.1), &p, cx))
            .child(self.view_item(
                View::Starred,
                Icon::BookmarkOutline,
                counts.map(|c| c.2),
                &p,
                cx,
            ))
            .child(
                div()
                    .px(px(22.))
                    .pt(px(14.))
                    .pb(px(6.))
                    .text_size(px(11.5))
                    .text_color(p.text3)
                    .child("CARPETAS"),
            )
            .child(
                div()
                    .id("categories")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(categories),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(p.divider)
                    .pt(px(8.))
                    .child(self.screen_item(Screen::Feeds, Icon::Rss, "Feeds", &p, cx))
                    .child(self.screen_item(Screen::Settings, Icon::Tune, "Ajustes", &p, cx))
                    .child(self.sync_line(&p)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_in_whole_minutes() {
        assert_eq!(
            synced_label(Duration::from_secs(20)),
            "Sincronizado (ahora)"
        );
        assert_eq!(
            synced_label(Duration::from_secs(5 * 60 + 59)),
            "Sincronizado (5 min)"
        );
        assert_eq!(
            synced_label(Duration::from_secs(90 * 60)),
            "Sincronizado (90 min)"
        );
    }
}
