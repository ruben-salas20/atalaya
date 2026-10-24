//! Columna 2 de la pantalla 4: el buscador, los filtros y los resultados
//! (P1–P3) o, con el campo vacío, las búsquedas recientes (P6).
//!
//! Mientras se escribe, pide cómo se buscará en inglés tras 0,6 s sin teclear.
//! Busca al pulsar Intro o al elegir una reciente. Con una búsqueda hecha,
//! cambiar un filtro vuelve a buscar.

use std::collections::HashMap;
use std::time::Duration;

use atalaya_api::{Id, PaperField, PaperPage, PaperQuery, PaperSort, PaperSummary};
use gpui::prelude::*;
use gpui::{
    Context, ElementId, Entity, EventEmitter, FontWeight, SharedString, Subscription, Task, Window,
    div, px, relative,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::tooltip::Tooltip;

use super::format::{
    field_label, paper_date, recent_label, results_header, short_authors, sort_label,
};
use super::recent::{self, RecentSearch};
use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::widgets::{badge, icon};

/// Pausa al escribir antes de pedir la búsqueda en inglés, como en Android.
const PREVIEW_AFTER: Duration = Duration::from_millis(600);
const PAGE_SIZE: u32 = 20;

const FIELDS: [PaperField; 4] = [
    PaperField::All,
    PaperField::Title,
    PaperField::Author,
    PaperField::Abstract,
];
const SORTS: [PaperSort; 2] = [PaperSort::Recent, PaperSort::Relevance];

pub enum PaperSearchEvent {
    Selected(PaperSummary),
}

enum Load {
    Idle,
    Searching,
    LoadingMore,
    Failed(String),
}

pub struct PaperSearch {
    input: Entity<InputState>,
    field: PaperField,
    sort: PaperSort,
    /// «Se buscará en inglés: …». `None` sin texto o al buscar por autor.
    english: Option<String>,
    recents: Vec<RecentSearch>,
    /// La búsqueda que dio `papers`. `None` si aún no se buscó.
    submitted: Option<PaperQuery>,
    english_query: String,
    total: u64,
    papers: Vec<PaperSummary>,
    selected: Option<String>,
    /// Lo guardado desde que se abrió la app: arXiv → artículo de Miniflux.
    /// Así la fila muestra el marcador sin repetir la búsqueda.
    saved: HashMap<String, Id>,
    load: Load,
    _preview: Task<()>,
    _search: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaperSearchEvent> for PaperSearch {}

impl PaperSearch {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Buscar papers"));
        let subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.schedule_preview(cx);
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.search(cx),
                _ => {}
            },
        )];
        Self {
            input,
            field: PaperField::All,
            sort: PaperSort::Recent,
            english: None,
            recents: recent::load(),
            submitted: None,
            english_query: String::new(),
            total: 0,
            papers: Vec::new(),
            selected: None,
            saved: HashMap::new(),
            load: Load::Idle,
            _preview: Task::ready(()),
            _search: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Se guardó un paper desde el lector: su fila muestra el marcador.
    pub fn mark_saved(&mut self, arxiv_id: &str, entry_id: Id, cx: &mut Context<Self>) {
        self.saved.insert(arxiv_id.to_owned(), entry_id);
        for paper in self.papers.iter_mut().filter(|p| p.arxiv_id == arxiv_id) {
            paper.saved_entry_id = Some(entry_id);
        }
        cx.notify();
    }

    fn text(&self, cx: &Context<Self>) -> String {
        self.input.read(cx).value().trim().to_owned()
    }

    fn schedule_preview(&mut self, cx: &mut Context<Self>) {
        let q = self.text(cx);
        if q.is_empty() || self.field == PaperField::Author {
            self.english = None;
            self._preview = Task::ready(());
            return;
        }
        let Ok(client) = crate::ui::client(cx) else {
            return;
        };
        self._preview = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PREVIEW_AFTER).await;
            let asked = q.clone();
            let result = cx
                .background_spawn(async move { client.translate_paper_query(&asked) })
                .await;
            this.update(cx, |this, cx| {
                // Sin la vista previa se puede buscar igual: un fallo no se enseña.
                if let Ok(t) = result
                    && this.text(cx) == q
                    && this.field != PaperField::Author
                {
                    this.english = Some(t.english);
                    cx.notify();
                }
            })
            .ok();
        });
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let q = self.text(cx);
        if q.is_empty() {
            return;
        }
        recent::add_recent(
            &mut self.recents,
            RecentSearch {
                q: q.clone(),
                field: self.field,
            },
        );
        if let Err(error) = recent::save(&self.recents) {
            eprintln!("atalaya: {error}");
        }
        let query = PaperQuery {
            q,
            field: self.field,
            sort: self.sort,
            limit: Some(PAGE_SIZE),
            offset: None,
        };
        self.submitted = Some(query.clone());
        self.papers.clear();
        self.total = 0;
        self.english_query.clear();
        self.fetch(query, cx);
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.submitted.clone() else {
            return;
        };
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let query = PaperQuery {
            offset: Some(self.papers.len() as u32),
            ..query
        };
        self.fetch(query, cx);
    }

    fn fetch(&mut self, query: PaperQuery, cx: &mut Context<Self>) {
        let client = match crate::ui::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.load = Load::Failed(error);
                cx.notify();
                return;
            }
        };
        let more = query.offset.is_some_and(|o| o > 0);
        self.load = if more {
            Load::LoadingMore
        } else {
            Load::Searching
        };
        cx.notify();
        self._search = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.search_papers(&query) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(PaperPage {
                        english_query,
                        total,
                        papers,
                    }) => {
                        let known: Vec<String> =
                            this.papers.iter().map(|p| p.arxiv_id.clone()).collect();
                        let fresh: Vec<PaperSummary> = papers
                            .into_iter()
                            .filter(|p| !known.contains(&p.arxiv_id))
                            .map(|mut p| {
                                if let Some(id) = this.saved.get(&p.arxiv_id) {
                                    p.saved_entry_id = Some(*id);
                                }
                                p
                            })
                            .collect();
                        // Si arXiv no da nada nuevo, la lista se da por acabada.
                        this.total = if more && fresh.is_empty() {
                            this.papers.len() as u64
                        } else {
                            total
                        };
                        this.english_query = english_query;
                        this.papers.extend(fresh);
                        this.load = Load::Idle;
                    }
                    Err(error) => this.load = Load::Failed(error),
                }
                cx.notify();
            })
            .ok();
        });
    }

    fn set_field(&mut self, field: PaperField, cx: &mut Context<Self>) {
        if self.field == field {
            return;
        }
        self.field = field;
        self.english = None;
        self.schedule_preview(cx);
        self.research(cx);
    }

    fn set_sort(&mut self, sort: PaperSort, cx: &mut Context<Self>) {
        if self.sort == sort {
            return;
        }
        self.sort = sort;
        self.research(cx);
    }

    /// Con una búsqueda a la vista, un filtro nuevo la repite.
    fn research(&mut self, cx: &mut Context<Self>) {
        if self.submitted.is_some() && !self.text(cx).is_empty() {
            self.search(cx);
        } else {
            cx.notify();
        }
    }

    fn use_recent(&mut self, recent: RecentSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_value(recent.q.clone(), window, cx)
        });
        self.field = recent.field;
        self.english = None;
        self.search(cx);
        self.schedule_preview(cx);
    }

    fn remove_recent(&mut self, recent: &RecentSearch, cx: &mut Context<Self>) {
        self.recents.retain(|r| r != recent);
        if let Err(error) = recent::save(&self.recents) {
            eprintln!("atalaya: {error}");
        }
        cx.notify();
    }

    fn select(&mut self, paper: PaperSummary, cx: &mut Context<Self>) {
        self.selected = Some(paper.arxiv_id.clone());
        cx.emit(PaperSearchEvent::Selected(paper));
        cx.notify();
    }

    fn search_bar(&self, p: &Palette) -> impl IntoElement {
        div().pt(px(10.)).px(px(12.)).flex().child(
            div()
                .flex_1()
                .min_w_0()
                .h(px(42.))
                .rounded(px(21.))
                .bg(p.surface)
                .flex()
                .items_center()
                .gap(px(10.))
                .pl(px(14.))
                .pr(px(6.))
                .child(icon(Icon::Search, px(16.), p.text3))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(15.))
                        .child(Input::new(&self.input).appearance(false).cleanable(true)),
                ),
        )
    }

    fn chip(
        id: impl Into<ElementId>,
        label: &'static str,
        on: bool,
        p: &Palette,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = p.surface;
        div()
            .id(id)
            .flex_none()
            .text_size(px(13.))
            .px(px(11.))
            .py(px(5.))
            .rounded(px(15.))
            .border_1()
            .when(on, |d| {
                d.bg(p.accent)
                    .border_color(p.accent)
                    .text_color(p.on_accent)
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!on, |d| {
                d.border_color(p.divider)
                    .text_color(p.text2)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
            })
            .child(label)
    }

    fn filters(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let fields = FIELDS.map(|field| {
            Self::chip(
                SharedString::from(format!("field-{field:?}")),
                field_label(field),
                self.field == field,
                p,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_field(field, cx)))
        });
        let sorts = SORTS.map(|sort| {
            Self::chip(
                SharedString::from(format!("sort-{sort:?}")),
                sort_label(sort),
                self.sort == sort,
                p,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_sort(sort, cx)))
        });
        let row = || {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .px(px(16.))
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .when_some(self.english.clone(), |d, english| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(18.))
                        .pt(px(8.))
                        .text_size(px(12.5))
                        .text_color(p.text3)
                        .child(icon(Icon::Translate, px(13.), p.text3))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(format!("Se buscará en inglés: «{english}»")),
                        ),
                )
            })
            .child(row().pt(px(10.)).pb(px(4.)).children(fields))
            .child(row().pt(px(4.)).pb(px(10.)).children(sorts))
    }

    fn recents(&self, p: &Palette, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        if self.recents.is_empty() {
            return vec![
                div()
                    .px(px(20.))
                    .py(px(18.))
                    .text_size(px(13.5))
                    .text_color(p.text3)
                    .child("Escribe en español y pulsa Intro. Se buscará en arXiv en inglés.")
                    .into_any_element(),
            ];
        }
        let mut rows = vec![
            div()
                .px(px(20.))
                .pt(px(18.))
                .pb(px(4.))
                .text_size(px(12.5))
                .text_color(p.text3)
                .child("BÚSQUEDAS RECIENTES")
                .into_any_element(),
        ];
        let hover = p.surface;
        for (i, recent) in self.recents.iter().enumerate() {
            let used = recent.clone();
            let removed = recent.clone();
            rows.push(
                div()
                    .id(ElementId::NamedInteger("recent".into(), i as u64))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(14.))
                    .pl(px(20.))
                    .pr(px(10.))
                    .py(px(7.))
                    .text_size(px(15.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .child(icon(Icon::History, px(20.), p.text3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(recent_label(recent)),
                    )
                    .child(
                        div()
                            .id(ElementId::NamedInteger("recent-x".into(), i as u64))
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .hover(|s| s.bg(p.surface2))
                            .child(icon(Icon::Close, px(18.), p.text3))
                            .tooltip(|window, cx| {
                                Tooltip::new("Quitar de recientes").build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.remove_recent(&removed, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.use_recent(used.clone(), window, cx)
                    }))
                    .into_any_element(),
            );
        }
        rows
    }

    fn row(&self, paper: &PaperSummary, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected.as_deref() == Some(paper.arxiv_id.as_str());
        let meta = format!(
            "{} · {}",
            short_authors(&paper.authors),
            paper_date(paper.published_at)
        );
        let hover = p.surface;
        let clicked = paper.clone();
        div()
            .id(SharedString::from(format!("paper-{}", paper.arxiv_id)))
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
                    .font_weight(FontWeight::MEDIUM)
                    .child(paper.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(p.text2)
                    .child(div().min_w_0().truncate().child(meta))
                    .when(paper.saved_entry_id.is_some(), |d| {
                        d.child(icon(Icon::Bookmark, px(13.), p.text3))
                    })
                    .child(div().flex_1())
                    .child(badge(paper.category.to_uppercase(), p)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select(clicked.clone(), cx)))
    }

    fn results(&self, p: &Palette, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let notice = |text: String| {
            div()
                .px(px(18.))
                .py(px(16.))
                .text_size(px(13.5))
                .text_color(p.text2)
                .child(text)
                .into_any_element()
        };
        match &self.load {
            Load::Searching => return vec![notice("Buscando en arXiv…".into())],
            Load::Failed(error) if self.papers.is_empty() => return vec![notice(error.clone())],
            Load::Idle if self.papers.is_empty() => {
                let q = match self.submitted.as_ref() {
                    Some(s) if self.english_query.is_empty() => s.q.clone(),
                    _ => self.english_query.clone(),
                };
                return vec![notice(format!("No hay papers para «{q}»"))];
            }
            _ => {}
        }
        let mut rows = vec![
            div()
                .px(px(16.))
                .py(px(10.))
                .border_b_1()
                .border_color(p.divider)
                .text_size(px(12.5))
                .line_height(relative(1.4))
                .text_color(p.text3)
                .child(results_header(self.total, &self.english_query))
                .into_any_element(),
        ];
        rows.extend(
            self.papers
                .iter()
                .map(|paper| self.row(paper, p, cx).into_any_element()),
        );
        if let Load::Failed(error) = &self.load {
            rows.push(notice(error.clone()));
        }
        if (self.papers.len() as u64) < self.total {
            let loading = matches!(self.load, Load::LoadingMore);
            let hover = p.surface;
            rows.push(
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
                    .on_click(cx.listener(|this, _, _, cx| this.load_more(cx)))
                    .into_any_element(),
            );
        }
        rows
    }
}

impl Render for PaperSearch {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        // Con el campo vacío, las recientes; si no, los resultados de la
        // última búsqueda hasta que se pulse Intro otra vez.
        let show_results = self.submitted.is_some() && !self.text(cx).is_empty();
        let body = if show_results {
            self.results(&p, cx)
        } else {
            self.recents(&p, cx)
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
            .child(self.search_bar(&p))
            .child(self.filters(&p, cx))
            .child(
                div()
                    .id("papers")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(p.divider)
                    .children(body)
                    .overflow_y_scrollbar(),
            )
    }
}
