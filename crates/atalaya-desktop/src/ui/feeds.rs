//! Pantalla de feeds (F1–F6), la 5 de `maquetas-pc.html`. En medio, los
//! feeds agrupados por carpeta; a la derecha, un panel: añadir un feed,
//! editar el elegido o renombrar y borrar una carpeta.
//!
//! Tras cada cambio emite [`FeedsEvent::Changed`] para que el menú lateral
//! se recargue.

use atalaya_api::{Category, DiscoveredFeed, Feed, FeedUpdate, Id, NewFeed};
use chrono::{DateTime, Utc};
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription, Task,
    Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::switch::Switch;

use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::format::ago;
use crate::ui::widgets::{bar, icon, icon_button};

pub enum FeedsEvent {
    /// Cambiaron los feeds o las carpetas.
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Add,
    Feed(Id),
    Folder(Id),
}

enum Load {
    Loading,
    Ready,
    Failed(String),
}

pub struct FeedsScreen {
    feeds: Vec<Feed>,
    categories: Vec<Category>,
    load: Load,
    panel: Panel,
    /// Dirección que se escribe en «Añadir feed».
    url: Entity<InputState>,
    found: Vec<DiscoveredFeed>,
    chosen: usize,
    /// Nombre del feed o de la carpeta que se edita.
    name: Entity<InputState>,
    folder: Option<Id>,
    folder_open: bool,
    /// Nombre de la carpeta nueva, cuando se eligió «Nueva carpeta…».
    new_folder: Entity<InputState>,
    creating_folder: bool,
    crawler: bool,
    /// Se pidió quitar o borrar y falta confirmarlo.
    confirming: bool,
    busy: bool,
    message: Option<Result<String, String>>,
    _load: Task<()>,
    _action: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FeedsEvent> for FeedsScreen {}

impl FeedsScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("theverge.com"));
        let name = cx.new(|cx| InputState::new(window, cx));
        let new_folder =
            cx.new(|cx| InputState::new(window, cx).placeholder("Nombre de la carpeta"));
        let subscriptions = vec![
            cx.subscribe_in(&url, window, |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.discover(window, cx);
                }
            }),
            cx.subscribe_in(
                &new_folder,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.create_folder(window, cx);
                    }
                },
            ),
        ];
        Self {
            feeds: Vec::new(),
            categories: Vec::new(),
            load: Load::Loading,
            panel: Panel::Add,
            url,
            found: Vec::new(),
            chosen: 0,
            name,
            folder: None,
            folder_open: false,
            new_folder,
            creating_folder: false,
            crawler: false,
            confirming: false,
            busy: false,
            message: None,
            _load: Task::ready(()),
            _action: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    /// Vuelve a pedir los feeds y las carpetas.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.load = Load::Failed(error);
                cx.notify();
                return;
            }
        };
        if self.feeds.is_empty() {
            self.load = Load::Loading;
        }
        cx.notify();
        self._load = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    Ok::<_, String>((client.feeds()?, client.categories()?))
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok((feeds, categories)) => {
                        this.feeds = feeds;
                        this.categories = categories;
                        this.load = Load::Ready;
                        this.fix_panel();
                    }
                    Err(error) => this.load = Load::Failed(error),
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// Si lo que se editaba ya no existe, vuelve a «Añadir feed». Y si no hay
    /// carpeta elegida, elige la primera.
    fn fix_panel(&mut self) {
        let gone = match self.panel {
            Panel::Add => false,
            Panel::Feed(id) => !self.feeds.iter().any(|f| f.id == id),
            Panel::Folder(id) => !self.categories.iter().any(|c| c.id == id),
        };
        if gone {
            self.panel = Panel::Add;
        }
        if !self.categories.iter().any(|c| Some(c.id) == self.folder) {
            self.folder = self.categories.first().map(|c| c.id);
        }
    }

    fn reset_panel(&mut self, panel: Panel) {
        self.panel = panel;
        self.folder_open = false;
        self.confirming = false;
        self.message = None;
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_panel(Panel::Add);
        self.found.clear();
        self.chosen = 0;
        self.crawler = false;
        self.folder = self.categories.first().map(|c| c.id);
        self.url.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn open_feed(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(feed) = self.feeds.iter().find(|f| f.id == id) else {
            return;
        };
        let title = feed.title.clone();
        self.folder = Some(feed.category.id);
        self.crawler = feed.crawler;
        self.reset_panel(Panel::Feed(id));
        self.name
            .update(cx, |input, cx| input.set_value(title, window, cx));
        cx.notify();
    }

    fn open_folder(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(category) = self.categories.iter().find(|c| c.id == id) else {
            return;
        };
        let title = category.title.clone();
        self.reset_panel(Panel::Folder(id));
        self.name
            .update(cx, |input, cx| input.set_value(title, window, cx));
        cx.notify();
    }

    /// Lanza una petición al servicio en segundo plano. Al acabar, `done`
    /// recibe el resultado con la ventana a mano.
    fn run<T: Send + 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        request: impl FnOnce(&crate::client::ApiClient) -> Result<T, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<T, String>, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.message = Some(Err(error));
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.message = None;
        cx.notify();
        self._action = cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { request(&client) }).await;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                done(this, result, window, cx);
                cx.notify();
            })
            .ok();
        });
    }

    fn discover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url.read(cx).value().trim().to_owned();
        if url.is_empty() || self.busy {
            return;
        }
        self.found.clear();
        self.chosen = 0;
        self.run(
            window,
            cx,
            move |client| client.discover(&url),
            |this, result, _, _| match result {
                Ok(found) if found.is_empty() => {
                    this.message = Some(Err("No se encontró ningún feed en esa dirección".into()));
                }
                Ok(found) => this.found = found,
                Err(error) => this.message = Some(Err(error)),
            },
        );
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = self.found.get(self.chosen) else {
            return;
        };
        let Some(category_id) = self.folder else {
            self.message = Some(Err("Elige una carpeta".into()));
            cx.notify();
            return;
        };
        let new = NewFeed {
            feed_url: found.feed_url.clone(),
            category_id,
            crawler: self.crawler,
        };
        self.run(
            window,
            cx,
            move |client| client.add_feed(&new),
            |this, result, window, cx| match result {
                Ok(Some(feed)) => {
                    let id = feed.id;
                    this.feeds.push(feed);
                    this.found.clear();
                    this.url
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.open_feed(id, window, cx);
                    this.message = Some(Ok("Feed añadido".into()));
                    this.reload(cx);
                    cx.emit(FeedsEvent::Changed);
                }
                // El servicio sigue creándolo aunque no haya respondido a tiempo (#48).
                Ok(None) => {
                    this.found.clear();
                    this.url
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.message = Some(Ok(
                        "El feed se está creando: Miniflux descarga sus artículos. \
                         Aparecerá en la lista en unos minutos."
                            .into(),
                    ));
                    this.reload(cx);
                    cx.emit(FeedsEvent::Changed);
                }
                Err(error) => this.message = Some(Err(error)),
            },
        );
    }

    fn save_feed(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let update = FeedUpdate {
            title: Some(self.name.read(cx).value().trim().to_owned()),
            category_id: self.folder,
            crawler: Some(self.crawler),
        };
        self.run(
            window,
            cx,
            move |client| client.update_feed(id, &update),
            |this, result, _, cx| match result {
                Ok(feed) => {
                    if let Some(old) = this.feeds.iter_mut().find(|f| f.id == feed.id) {
                        *old = feed;
                    }
                    this.message = Some(Ok("Cambios guardados".into()));
                    this.reload(cx);
                    cx.emit(FeedsEvent::Changed);
                }
                Err(error) => this.message = Some(Err(error)),
            },
        );
    }

    fn delete_feed(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        self.confirming = false;
        self.run(
            window,
            cx,
            move |client| client.delete_feed(id),
            move |this, result, window, cx| match result {
                Ok(()) => {
                    this.feeds.retain(|f| f.id != id);
                    this.open_add(window, cx);
                    this.message = Some(Ok("Feed quitado".into()));
                    this.reload(cx);
                    cx.emit(FeedsEvent::Changed);
                }
                Err(error) => this.message = Some(Err(error)),
            },
        );
    }

    fn create_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.new_folder.read(cx).value().trim().to_owned();
        if title.is_empty() {
            return;
        }
        self.run(
            window,
            cx,
            move |client| client.create_category(&title),
            |this, result, window, cx| match result {
                Ok(category) => {
                    this.folder = Some(category.id);
                    this.categories.push(category);
                    this.creating_folder = false;
                    this.new_folder
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.message = Some(Ok("Carpeta creada".into()));
                    this.reload(cx);
                    cx.emit(FeedsEvent::Changed);
                }
                Err(error) => this.message = Some(Err(error)),
            },
        );
    }

    fn rename_folder(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.name.read(cx).value().trim().to_owned();
        self.run(
            window,
            cx,
            move |client| client.rename_category(id, &title),
            |this, result, _, cx| match result {
                Ok(category) => {
                    if let Some(old) = this.categories.iter_mut().find(|c| c.id == category.id) {
                        *old = category;
                    }
                    this.message = Some(Ok("Carpeta renombrada".into()));
                    this.reload(cx);
                    cx.emit(FeedsEvent::Changed);
                }
                Err(error) => this.message = Some(Err(error)),
            },
        );
    }

    fn delete_folder(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        self.confirming = false;
        self.run(
            window,
            cx,
            move |client| client.delete_category(id),
            move |this, result, window, cx| match result {
                Ok(()) => {
                    this.categories.retain(|c| c.id != id);
                    this.open_add(window, cx);
                    this.message = Some(Ok("Carpeta borrada".into()));
                    this.reload(cx);
                    cx.emit(FeedsEvent::Changed);
                }
                Err(error) => this.message = Some(Err(capitalize(&error))),
            },
        );
    }

    // -----------------------------------------------------------------------
    // Columna de feeds
    // -----------------------------------------------------------------------

    fn feed_list(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        match &self.load {
            Load::Loading => rows.push(note("Cargando…", p).into_any_element()),
            Load::Failed(error) => rows.push(note(error.clone(), p).into_any_element()),
            Load::Ready => {
                for category in &self.categories {
                    rows.push(self.folder_header(category, p, cx).into_any_element());
                    for feed in self.feeds.iter().filter(|f| f.category.id == category.id) {
                        rows.push(self.feed_row(feed, p, cx).into_any_element());
                    }
                }
            }
        }
        div()
            .w(px(360.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.divider)
            .child(
                bar(p)
                    .pl(px(18.))
                    .pr(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_baseline()
                            .text_size(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Feeds")
                            .child(
                                div()
                                    .ml(px(6.))
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(p.text3)
                                    .child(self.feeds.len().to_string()),
                            ),
                    )
                    .child(
                        icon_button("add-feed", Icon::Add, p)
                            .on_click(cx.listener(|this, _, window, cx| this.open_add(window, cx))),
                    ),
            )
            .child(
                div()
                    .id("feed-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }

    /// Cabecera de una carpeta (`.fd-folder`), con el lápiz para editarla.
    fn folder_header(
        &self,
        category: &Category,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = category.id;
        let active = self.panel == Panel::Folder(id);
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(20.))
            .pt(px(16.))
            .pb(px(6.))
            .text_size(px(12.5))
            .text_color(if active { p.text } else { p.text3 })
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .child(category.title.to_uppercase()),
            )
            .child(
                div()
                    .id(gpui::ElementId::NamedInteger(
                        "edit-folder".into(),
                        id as u64,
                    ))
                    .cursor_pointer()
                    .rounded(px(4.))
                    .p(px(2.))
                    .child(icon(Icon::Edit, px(16.), p.text3))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.open_folder(id, window, cx)),
                    ),
            )
    }

    /// Una fila de feed (`.fd`).
    fn feed_row(&self, feed: &Feed, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let id = feed.id;
        let selected = self.panel == Panel::Feed(id);
        let hover = p.surface;
        let detail = match &feed.error {
            Some(_) => div()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_color(p.text)
                .font_weight(FontWeight::MEDIUM)
                .child(icon(Icon::Warn, px(13.), p.text))
                .child(div().truncate().child(failed_text(feed.checked_at))),
            None => {
                let mut text = domain(&feed.site_url);
                if feed.crawler {
                    text.push_str(" · artículo completo");
                }
                div().truncate().child(text)
            }
        };
        div()
            .id(gpui::ElementId::NamedInteger("feed".into(), id as u64))
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(20.))
            .py(px(10.))
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
            .relative()
            .child(icon(Icon::Rss, px(16.), p.text3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(15.))
                            .truncate()
                            .child(feed.title.clone()),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .text_size(px(12.5))
                            .text_color(p.text3)
                            .child(detail),
                    ),
            )
            .child(icon(Icon::ChevronRight, px(16.), p.text3))
            .on_click(cx.listener(move |this, _, window, cx| this.open_feed(id, window, cx)))
    }

    // -----------------------------------------------------------------------
    // Panel de la derecha
    // -----------------------------------------------------------------------

    fn panel(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, form): (SharedString, Vec<AnyElement>) = match self.panel {
            Panel::Add => ("Añadir feed".into(), self.add_form(p, cx)),
            Panel::Feed(id) => match self.feeds.iter().find(|f| f.id == id) {
                Some(feed) => (feed.title.clone().into(), self.feed_form(feed, p, cx)),
                None => ("Añadir feed".into(), self.add_form(p, cx)),
            },
            Panel::Folder(id) => match self.categories.iter().find(|c| c.id == id) {
                Some(category) => ("Carpeta".into(), self.folder_form(category, p, cx)),
                None => ("Añadir feed".into(), self.add_form(p, cx)),
            },
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                bar(p).pl(px(40.)).child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_size(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title),
                ),
            )
            .child(
                div()
                    .id("feed-panel")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .max_w(px(520.))
                            .px(px(40.))
                            .py(px(24.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .children(form)
                            .children(self.message_line(p)),
                    ),
            )
    }

    fn message_line(&self, p: &Palette) -> Option<AnyElement> {
        let (text, color) = match self.message.as_ref()? {
            Ok(text) => (text.clone(), p.text2),
            Err(error) => (error.clone(), p.text),
        };
        Some(
            div()
                .text_size(px(13.))
                .text_color(color)
                .child(text)
                .into_any_element(),
        )
    }

    fn add_form(&self, p: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut form = vec![
            div()
                .child(label("Dirección de la web o del feed", p))
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(div().flex_1().child(Input::new(&self.url).cleanable(true)))
                        .child(
                            secondary(
                                "discover",
                                if self.busy && self.found.is_empty() {
                                    "Buscando…"
                                } else {
                                    "Buscar"
                                },
                                p,
                            )
                            .on_click(cx.listener(|this, _, window, cx| this.discover(window, cx))),
                        ),
                )
                .child(help(
                    "Basta con la dirección de la web: Atalaya busca sus feeds.",
                    p,
                ))
                .into_any_element(),
        ];
        if self.found.is_empty() {
            return form;
        }
        let options = self.found.iter().enumerate().map(|(i, found)| {
            let on = i == self.chosen;
            div()
                .id(gpui::ElementId::NamedInteger("found".into(), i as u64))
                .flex()
                .items_start()
                .gap(px(12.))
                .px(px(14.))
                .py(px(12.))
                .cursor_pointer()
                .when(i + 1 < self.found.len(), |d| {
                    d.border_b_1().border_color(p.divider)
                })
                .child(radio(on, p))
                .child(
                    div()
                        .min_w_0()
                        .child(div().text_size(px(14.5)).child(found.title.clone()))
                        .child(
                            div()
                                .mt(px(3.))
                                .text_size(px(12.5))
                                .text_color(p.text3)
                                .child(feed_facts(found.lang.as_deref(), found.articles_per_day)),
                        )
                        .child(
                            div()
                                .mt(px(2.))
                                .text_size(px(12.))
                                .text_color(p.text3)
                                .truncate()
                                .child(found.feed_url.clone()),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.chosen = i;
                    cx.notify();
                }))
        });
        form.push(
            div()
                .child(label("Feeds encontrados", p))
                .child(
                    div()
                        .border_1()
                        .border_color(p.divider)
                        .rounded(px(12.))
                        .overflow_hidden()
                        .children(options),
                )
                .into_any_element(),
        );
        form.push(self.folder_picker(p, cx));
        form.push(self.crawler_toggle(p, cx));
        form.push(
            primary(
                "add",
                if self.busy {
                    "Añadiendo…"
                } else {
                    "Añadir"
                },
                p,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                if !this.busy {
                    this.add(window, cx);
                }
            }))
            .into_any_element(),
        );
        form
    }

    fn feed_form(&self, feed: &Feed, p: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let id = feed.id;
        let mut facts = vec![
            stat("Dirección", feed.feed_url.clone(), p),
            stat(
                "Última actualización",
                feed.checked_at.map_or("nunca".into(), since),
                p,
            ),
            stat(
                "Idioma",
                feed.lang
                    .as_deref()
                    .map_or("no se sabe aún".into(), language),
                p,
            ),
            stat(
                "Artículos",
                feed.articles_per_day
                    .map_or("no se sabe aún".into(), frequency),
                p,
            ),
        ];
        if let Some(error) = &feed.error {
            facts.push(stat(
                "Error",
                format!("{} ({} seguidos)", error.message, error.count),
                p,
            ));
        }
        vec![
            div()
                .child(label("Nombre", p))
                .child(Input::new(&self.name))
                .into_any_element(),
            self.folder_picker(p, cx),
            self.crawler_toggle(p, cx),
            div().children(facts).into_any_element(),
            primary(
                "save-feed",
                if self.busy {
                    "Guardando…"
                } else {
                    "Guardar cambios"
                },
                p,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if !this.busy {
                    this.save_feed(id, window, cx);
                }
            }))
            .into_any_element(),
            self.danger_zone(
                "Quitar feed",
                format!(
                    "¿Quitar «{}»? Miniflux borra también sus artículos.",
                    feed.title
                ),
                p,
                cx,
                move |this, window, cx| this.delete_feed(id, window, cx),
            ),
        ]
    }

    fn folder_form(
        &self,
        category: &Category,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let id = category.id;
        let count = self.feeds.iter().filter(|f| f.category.id == id).count();
        let contents = match count {
            0 => "Está vacía.".to_owned(),
            1 => "Tiene 1 feed. Para borrarla, muévelo o quítalo antes.".to_owned(),
            n => format!("Tiene {n} feeds. Para borrarla, muévelos o quítalos antes."),
        };
        vec![
            div()
                .child(label("Nombre", p))
                .child(Input::new(&self.name))
                .child(help(contents, p))
                .into_any_element(),
            primary(
                "save-folder",
                if self.busy {
                    "Guardando…"
                } else {
                    "Renombrar"
                },
                p,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if !this.busy {
                    this.rename_folder(id, window, cx);
                }
            }))
            .into_any_element(),
            self.danger_zone(
                "Borrar carpeta",
                format!("¿Borrar la carpeta «{}»?", category.title),
                p,
                cx,
                move |this, window, cx| this.delete_folder(id, window, cx),
            ),
        ]
    }

    /// El botón de quitar o borrar; al pulsarlo, pide confirmación.
    fn danger_zone(
        &self,
        text: &'static str,
        question: String,
        p: &Palette,
        cx: &mut Context<Self>,
        confirm: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        if !self.confirming {
            return danger("danger", text, p)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.confirming = true;
                    this.message = None;
                    cx.notify();
                }))
                .into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().text_size(px(14.)).child(question))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(primary("confirm", text, p).on_click(
                        cx.listener(move |this, _, window, cx| confirm(this, window, cx)),
                    ))
                    .child(danger("cancel", "Cancelar", p).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.confirming = false;
                            cx.notify();
                        },
                    ))),
            )
            .into_any_element()
    }

    /// Elegir carpeta: un campo que se despliega con la lista de carpetas.
    fn folder_picker(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .categories
            .iter()
            .find(|c| Some(c.id) == self.folder)
            .map_or("Elige una carpeta".to_owned(), |c| c.title.clone());
        let field = field_box(p)
            .id("folder-picker")
            .cursor_pointer()
            .when(self.folder_open, |d| d.border_color(p.accent))
            .child(div().flex_1().truncate().child(current))
            .child(icon(Icon::ExpandMore, px(16.), p.text2))
            .on_click(cx.listener(|this, _, _, cx| {
                this.folder_open = !this.folder_open;
                cx.notify();
            }));
        let options = self.folder_open.then(|| {
            let hover = p.surface;
            div()
                .mt(px(4.))
                .border_1()
                .border_color(p.divider)
                .rounded(px(10.))
                .overflow_hidden()
                .children(self.categories.iter().map(|c| {
                    let id = c.id;
                    let on = Some(id) == self.folder;
                    div()
                        .id(gpui::ElementId::NamedInteger(
                            "folder-option".into(),
                            id as u64,
                        ))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .h(px(38.))
                        .px(px(14.))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .when(on, |d| d.font_weight(FontWeight::MEDIUM))
                        .child(div().flex_1().truncate().child(c.title.clone()))
                        .when(on, |d| d.child(icon(Icon::Check, px(16.), p.text2)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.folder = Some(id);
                            this.folder_open = false;
                            this.creating_folder = false;
                            cx.notify();
                        }))
                }))
                .child(
                    div()
                        .id("folder-new")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .h(px(38.))
                        .px(px(14.))
                        .border_t_1()
                        .border_color(p.divider)
                        .cursor_pointer()
                        .text_color(p.text2)
                        .hover(move |s| s.bg(hover))
                        .child(icon(Icon::Add, px(16.), p.text2))
                        .child("Nueva carpeta…")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.folder_open = false;
                            this.creating_folder = true;
                            this.new_folder
                                .update(cx, |input, cx| input.focus(window, cx));
                            cx.notify();
                        })),
                )
        });
        let create =
            self.creating_folder.then(|| {
                div()
                    .mt(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().flex_1().child(Input::new(&self.new_folder)))
                    .child(secondary("folder-create", "Crear", p).on_click(
                        cx.listener(|this, _, window, cx| this.create_folder(window, cx)),
                    ))
            });
        div()
            .child(label("Carpeta", p))
            .child(field)
            .children(options)
            .children(create)
            .into_any_element()
    }

    fn crawler_toggle(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(
                div()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(15.))
                            .child("Descargar el artículo completo"),
                    )
                    .child(help("Para feeds que solo traen un resumen", p)),
            )
            .child(
                Switch::new("crawler")
                    .checked(self.crawler)
                    .color(p.accent)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.crawler = *checked;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}

impl Render for FeedsScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .child(self.feed_list(&p, cx))
            .child(self.panel(&p, cx))
    }
}

// ---------------------------------------------------------------------------
// Piezas del formulario, con las medidas de las maquetas
// ---------------------------------------------------------------------------

fn note(text: impl Into<SharedString>, p: &Palette) -> impl IntoElement {
    div()
        .px(px(20.))
        .py(px(16.))
        .text_size(px(13.))
        .text_color(p.text3)
        .child(text.into())
}

/// Etiqueta encima de un campo (`.fl`).
fn label(text: &'static str, p: &Palette) -> impl IntoElement {
    div()
        .mb(px(6.))
        .text_size(px(12.5))
        .text_color(p.text2)
        .child(text)
}

/// Ayuda debajo de un campo (`.help`).
fn help(text: impl Into<SharedString>, p: &Palette) -> impl IntoElement {
    div()
        .mt(px(6.))
        .text_size(px(12.5))
        .text_color(p.text3)
        .child(text.into())
}

/// Caja de un campo (`.fi`).
fn field_box(p: &Palette) -> gpui::Div {
    div()
        .h(px(46.))
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(14.))
        .border_1()
        .border_color(p.divider)
        .bg(p.surface)
        .rounded(px(10.))
        .text_size(px(15.))
}

fn radio(on: bool, p: &Palette) -> impl IntoElement {
    div()
        .flex_none()
        .mt(px(1.))
        .size(px(20.))
        .rounded_full()
        .border_2()
        .border_color(if on { p.accent } else { p.text3 })
        .flex()
        .items_center()
        .justify_center()
        .when(on, |d| {
            d.child(div().size(px(10.)).rounded_full().bg(p.accent))
        })
}

/// Una línea de datos del feed (`.stat`).
fn stat(name: &'static str, value: String, p: &Palette) -> AnyElement {
    div()
        .flex()
        .justify_between()
        .gap(px(16.))
        .py(px(6.))
        .border_b_1()
        .border_color(p.divider)
        .text_size(px(13.5))
        .child(div().flex_none().text_color(p.text2).child(name))
        .child(div().min_w_0().truncate().child(value))
        .into_any_element()
}

/// Botón principal (`.primary`).
fn primary(id: &'static str, text: &'static str, p: &Palette) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(160.))
        .h(px(46.))
        .flex_none()
        .rounded(px(23.))
        .bg(p.accent)
        .text_color(p.on_accent)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(15.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .child(text)
}

/// Botón con borde (`.danger`).
fn danger(id: &'static str, text: &'static str, p: &Palette) -> gpui::Stateful<gpui::Div> {
    let hover = p.surface;
    div()
        .id(id)
        .w(px(160.))
        .h(px(46.))
        .flex_none()
        .rounded(px(23.))
        .border_1()
        .border_color(p.divider)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(15.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(text)
}

/// Botón pequeño junto a un campo.
fn secondary(id: &'static str, text: &'static str, p: &Palette) -> gpui::Stateful<gpui::Div> {
    danger(id, text, p).w(px(110.)).h(px(36.))
}

// ---------------------------------------------------------------------------
// Textos
// ---------------------------------------------------------------------------

/// Primera letra en mayúscula: el servicio da algunos errores en minúscula.
pub(crate) fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// «hace 2 h», o «ahora».
fn since(when: DateTime<Utc>) -> String {
    match ago(when, Utc::now()) {
        text if text == "ahora" => text,
        text => format!("hace {text}"),
    }
}

/// Lo que dice la fila de un feed que falla (F6).
fn failed_text(checked_at: Option<DateTime<Utc>>) -> String {
    match checked_at {
        Some(when) => format!("No se pudo actualizar · {}", since(when)),
        None => "No se pudo actualizar".into(),
    }
}

/// `https://www.theverge.com/tech` → `theverge.com`.
fn domain(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host).to_owned()
}

/// Nombre del idioma en español, o el código en mayúsculas si no se conoce.
fn language(code: &str) -> String {
    match code {
        "es" => "Español",
        "en" => "Inglés",
        "fr" => "Francés",
        "de" => "Alemán",
        "it" => "Italiano",
        "pt" => "Portugués",
        "ca" => "Catalán",
        "nl" => "Neerlandés",
        "ru" => "Ruso",
        "zh" => "Chino",
        "ja" => "Japonés",
        "ko" => "Coreano",
        other => return other.to_uppercase(),
    }
    .to_owned()
}

/// Cuánto publica: «unos 30 artículos al día» (F2).
fn frequency(per_day: f64) -> String {
    let per_week = (per_day * 7.0).round() as u64;
    match per_day.round() as u64 {
        n if n >= 2 => format!("unos {n} artículos al día"),
        1 => "un artículo al día".into(),
        _ if per_week >= 2 => format!("unos {per_week} artículos a la semana"),
        _ if per_week == 1 => "un artículo a la semana".into(),
        _ => "menos de un artículo a la semana".into(),
    }
}

/// «Inglés · unos 30 artículos al día», con lo que se sepa.
fn feed_facts(lang: Option<&str>, per_day: Option<f64>) -> String {
    let parts: Vec<String> = [lang.map(language), per_day.map(frequency)]
        .into_iter()
        .flatten()
        .collect();
    if parts.is_empty() {
        "Idioma y frecuencia desconocidos".into()
    } else {
        capitalize(&parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains() {
        assert_eq!(domain("https://www.theverge.com/tech"), "theverge.com");
        assert_eq!(domain("http://simonwillison.net/"), "simonwillison.net");
        assert_eq!(domain("simonwillison.net"), "simonwillison.net");
    }

    #[test]
    fn frequencies() {
        assert_eq!(frequency(30.2), "unos 30 artículos al día");
        assert_eq!(frequency(1.1), "un artículo al día");
        assert_eq!(frequency(0.4), "unos 3 artículos a la semana");
        assert_eq!(frequency(0.01), "menos de un artículo a la semana");
    }

    #[test]
    fn facts() {
        assert_eq!(
            feed_facts(Some("en"), Some(12.0)),
            "Inglés · unos 12 artículos al día"
        );
        assert_eq!(feed_facts(None, Some(1.0)), "Un artículo al día");
        assert_eq!(feed_facts(Some("xx"), None), "XX");
    }
}
