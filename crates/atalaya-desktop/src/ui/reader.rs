//! Columna 3: el artículo abierto (L5, L6), con `GET /api/entries/{id}`.
//!
//! Se abre en español si la traducción está lista. Si no, muestra lo ya
//! traducido (con lo que falta en gris) y la barra «Traduciendo… N de M»; sin
//! nada traducido, el original en gris. Vuelve a pedir el artículo cada pocos
//! segundos hasta que cambia solo (ADR 0015). La barra tiene el selector
//! «Español» / «Original» y los botones de leído, guardar y abrir en el
//! navegador. En el PC no hay deslizamientos (L4): eso es del móvil.

use std::rc::Rc;
use std::time::Duration;

use atalaya_api::{Entry, EntrySummary, EntryUpdate, Id, Progress, TranslationState};
use chrono::Utc;
use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, App, Context, ElementId, Entity, EventEmitter, FontWeight, Pixels,
    SharedString, Task, Window, div, px, relative,
};
use gpui_component::scroll::ScrollableElement;
use gpui_component::text::TextView;
use gpui_component::tooltip::Tooltip;

use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::format::{ago_phrase, code_in_pre, progress_fraction, progress_text, split_pending};
use crate::ui::fulltext::FullText;
use crate::ui::widgets::{MenuAction, bar, icon, icon_button, more_menu};

/// Cada cuánto se vuelve a pedir un artículo o un paper que se está traduciendo.
pub const POLL_EVERY: Duration = Duration::from_secs(3);

pub enum ReaderEvent {
    /// Cambió leído o guardado: la lista y el menú deben reflejarlo.
    Changed(EntrySummary),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Spanish,
    Original,
}

enum Load {
    Loading,
    Ready(Box<Entry>),
    Failed(String),
}

/// Lo que se ve en «Español»: la traducción completa, o lo traducido hasta
/// ahora mientras se traduce (ADR 0015). `None` si todavía no hay nada en
/// español.
fn spanish_html(entry: &Entry) -> Option<&str> {
    match entry.translation {
        TranslationState::Done => Some(entry.translated_content.as_deref().unwrap_or_default()),
        TranslationState::Pending => entry.translated_content.as_deref(),
        TranslationState::NotNeeded | TranslationState::Failed => None,
    }
}

pub struct Reader {
    /// La fila elegida: con ella se pinta la cabecera mientras llega el resto.
    summary: Option<EntrySummary>,
    load: Load,
    /// Lo que eligió el selector. Solo cuenta si hay traducción.
    lang: Lang,
    /// Fallo al marcar leído o guardar, que se enseña bajo la barra.
    action_error: Option<String>,
    _fetch: Task<()>,
    /// El texto completo, si el artículo es un paper de arXiv (#90).
    full_text: Entity<FullText>,
}

impl EventEmitter<ReaderEvent> for Reader {}

impl Reader {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            summary: None,
            load: Load::Loading,
            lang: Lang::Spanish,
            action_error: None,
            _fetch: Task::ready(()),
            full_text: cx.new(|_| FullText::default()),
        }
    }

    pub fn show(&mut self, entry: EntrySummary, cx: &mut Context<Self>) {
        let id = entry.id;
        let unread = !entry.read;
        self.summary = Some(entry);
        self.load = Load::Loading;
        self.lang = Lang::Spanish;
        self.action_error = None;
        self.full_text.update(cx, |full, cx| full.open(None, cx));
        self.fetch(id, Duration::ZERO, cx);
        // Abrir marca como leído; el botón de la barra sirve para deshacerlo.
        // Sin red se guarda el cambio y se manda al volver la red.
        if unread {
            self.update(
                EntryUpdate {
                    read: Some(true),
                    starred: None,
                },
                cx,
            );
        }
        cx.notify();
    }

    /// Otro sitio (la lista) cambió leído o guardado: si es el artículo
    /// abierto, los botones de la barra lo reflejan.
    pub fn sync(&mut self, summary: &EntrySummary, cx: &mut Context<Self>) {
        if self.summary.as_ref().map(|s| s.id) == Some(summary.id) {
            self.set_summary(summary.clone(), cx);
        }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.summary = None;
        self.load = Load::Loading;
        self._fetch = Task::ready(());
        self.full_text.update(cx, |full, cx| full.open(None, cx));
        cx.notify();
    }

    /// Pide el artículo tras `delay`. Si sigue traduciéndose, se vuelve a
    /// llamar sola. Cambiar de artículo sustituye la tarea y la cancela.
    fn fetch(&mut self, id: Id, delay: Duration, cx: &mut Context<Self>) {
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.load = Load::Failed(error);
                return;
            }
        };
        self._fetch = cx.spawn(async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let result = cx.background_spawn(async move { client.entry(id) }).await;
            this.update(cx, |this, cx| {
                if this.summary.as_ref().map(|s| s.id) != Some(id) {
                    return;
                }
                match result {
                    Ok(entry) => {
                        if entry.translation == TranslationState::Pending {
                            this.fetch(id, POLL_EVERY, cx);
                        }
                        // El título puede llegar traducido después de la lista.
                        this.summary = Some(entry.summary.clone());
                        // Un paper de arXiv ofrece su texto completo (#90).
                        let arxiv_id = entry.arxiv_id.clone();
                        this.full_text
                            .update(cx, |full, cx| full.ensure(arxiv_id, cx));
                        this.load = Load::Ready(Box::new(entry));
                    }
                    // Un fallo al repetir no borra lo que ya se ve.
                    Err(error) => match this.load {
                        Load::Ready(_) => this.fetch(id, POLL_EVERY, cx),
                        _ => this.load = Load::Failed(error),
                    },
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// Marca leído o guardado en el servicio. El lector cambia al momento y
    /// vuelve atrás si el servicio falla; la lista y el menú se enteran con
    /// la respuesta.
    fn update(&mut self, update: EntryUpdate, cx: &mut Context<Self>) {
        let Some(before) = self.summary.clone() else {
            return;
        };
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.action_error = Some(error);
                cx.notify();
                return;
            }
        };
        // Se anota en la cola ya, en el orden en que el usuario lo pide.
        client.queue_change(&before, &update);
        let mut after = before.clone();
        if let Some(read) = update.read {
            after.read = read;
        }
        if let Some(starred) = update.starred {
            after.starred = starred;
        }
        self.set_summary(after, cx);
        self.action_error = None;

        // Sin guardar la tarea: pulsar leído y guardar seguidos no debe
        // cancelar la primera petición.
        let id = before.id;
        let current = before.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.update_entry(&current, &update) })
                .await;
            this.update(cx, |this, cx| {
                let current = this.summary.as_ref().map(|s| s.id) == Some(id);
                let summary = match result {
                    Ok(summary) => summary,
                    Err(error) => {
                        if current {
                            this.action_error = Some(error);
                        }
                        before
                    }
                };
                if current {
                    this.set_summary(summary.clone(), cx);
                }
                cx.emit(ReaderEvent::Changed(summary));
            })
            .ok();
        })
        .detach();
    }

    /// «Volver a traducir» (#91): borra la traducción del artículo y, si es un
    /// paper de arXiv, la de su paper, y vuelve a pedir el artículo. Con un
    /// 409 (se está traduciendo) no borra nada y lo dice.
    fn retranslate(&mut self, cx: &mut Context<Self>) {
        let Load::Ready(entry) = &self.load else {
            return;
        };
        let id = entry.summary.id;
        let arxiv_id = entry.arxiv_id.clone();
        let client = match super::client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.action_error = Some(error);
                cx.notify();
                return;
            }
        };
        self.action_error = None;
        cx.spawn(async move |this, cx| {
            let paper = arxiv_id.clone();
            let forgotten = cx
                .background_spawn(
                    async move { client.forget_translations(Some(id), paper.as_deref()) },
                )
                .await;
            this.update(cx, |this, cx| {
                if this.summary.as_ref().map(|s| s.id) != Some(id) {
                    return;
                }
                // Lo que ya se borró se vuelve a pedir, aunque otra cosa haya fallado.
                if forgotten.entry || forgotten.paper {
                    this.fetch(id, Duration::ZERO, cx);
                    this.full_text
                        .update(cx, |full, cx| full.open(arxiv_id, cx));
                }
                this.action_error = forgotten.error;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// La opción «Volver a traducir» del menú, que vuelve a este lector.
    fn retranslate_action(cx: &mut Context<Self>) -> MenuAction {
        let reader = cx.weak_entity();
        Rc::new(move |cx: &mut App| {
            reader.update(cx, |reader, cx| reader.retranslate(cx)).ok();
        })
    }

    fn set_summary(&mut self, summary: EntrySummary, cx: &mut Context<Self>) {
        if let Load::Ready(entry) = &mut self.load {
            entry.summary.read = summary.read;
            entry.summary.starred = summary.starred;
        }
        self.summary = Some(summary);
        cx.notify();
    }

    fn top_bar(&self, summary: &EntrySummary, p: &Palette, cx: &mut Context<Self>) -> gpui::Div {
        // «Español» se puede elegir también con la traducción a medias.
        let has_spanish = matches!(&self.load, Load::Ready(entry) if spanish_html(entry).is_some());
        let on = if has_spanish {
            self.lang
        } else {
            Lang::Original
        };

        let read = summary.read;
        let starred = summary.starred;
        let url = summary.url.clone();
        bar(p)
            .px(px(12.))
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .bg(p.surface)
                    .rounded(px(16.))
                    .p(px(3.))
                    .when(!has_spanish, |d| d.opacity(0.45))
                    .child(self.segment("Español", Lang::Spanish, on, has_spanish, p, cx))
                    .child(self.segment("Original", Lang::Original, on, has_spanish, p, cx)),
            )
            .child(div().flex_1())
            .child(
                icon_button("read", if read { Icon::Unread } else { Icon::Check }, p)
                    .tooltip(move |window, cx| {
                        let text = if read {
                            "Marcar como no leído"
                        } else {
                            "Marcar como leído"
                        };
                        Tooltip::new(text).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update(
                            EntryUpdate {
                                read: Some(!read),
                                ..Default::default()
                            },
                            cx,
                        )
                    })),
            )
            .child(
                icon_button(
                    "star",
                    if starred {
                        Icon::Bookmark
                    } else {
                        Icon::BookmarkOutline
                    },
                    p,
                )
                .tooltip(move |window, cx| {
                    let text = if starred {
                        "Quitar de guardados"
                    } else {
                        "Guardar"
                    };
                    Tooltip::new(text).build(window, cx)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.update(
                        EntryUpdate {
                            starred: Some(!starred),
                            ..Default::default()
                        },
                        cx,
                    )
                })),
            )
            .child(
                icon_button("open", Icon::Open, p)
                    .tooltip(|window, cx| Tooltip::new("Abrir en el navegador").build(window, cx))
                    .on_click({
                        let url = url.clone();
                        move |_, _, cx| cx.open_url(&url)
                    }),
            )
            .child(more_menu("more", url, Some(Self::retranslate_action(cx))))
    }

    fn segment(
        &self,
        label: &'static str,
        lang: Lang,
        on: Lang,
        enabled: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = lang == on;
        div()
            .id(label)
            .text_size(px(13.))
            .px(px(12.))
            .py(px(5.))
            .rounded(px(13.))
            .when(selected, |d| {
                d.bg(p.accent)
                    .text_color(p.on_accent)
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!selected, |d| d.text_color(p.text2))
            .when(enabled && !selected, |d| d.cursor_pointer())
            .child(label)
            .when(enabled, |d| {
                d.on_click(cx.listener(move |this, _, _, cx| {
                    this.lang = lang;
                    cx.notify();
                }))
            })
    }

    /// Fila de cabecera: fuente, autor y cuánto hace.
    fn source_line(summary: &EntrySummary, author: Option<&str>) -> String {
        let mut parts = vec![summary.feed_title.clone()];
        if let Some(author) = author.filter(|a| !a.trim().is_empty()) {
            parts.push(author.trim().to_owned());
        }
        parts.push(ago_phrase(summary.published_at, Utc::now()));
        parts.join(" · ")
    }

    fn article(&self, summary: &EntrySummary, p: &Palette, body_size: Pixels) -> impl IntoElement {
        let entry = match &self.load {
            Load::Ready(entry) => Some(entry.as_ref()),
            _ => None,
        };
        let translation = entry.map(|e| e.translation);
        let pending = translation == Some(TranslationState::Pending);
        // Con `Pending`, solo lo traducido hasta ahora (ADR 0015).
        let shown_spanish = entry
            .and_then(spanish_html)
            .filter(|_| self.lang == Lang::Spanish);
        let spanish = shown_spanish.is_some();
        let title = if spanish || !summary.title_translated || entry.is_none() {
            summary.title.clone()
        } else {
            summary.original_title.clone()
        };

        let header = div()
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(p.text2)
                    .mb(px(10.))
                    .child(Self::source_line(
                        summary,
                        entry.and_then(|e| e.author.as_deref()),
                    )),
            )
            .child(
                div()
                    .text_size(px(28.))
                    .line_height(relative(1.25))
                    .font_weight(FontWeight::MEDIUM)
                    .mb(px(14.))
                    .child(title),
            );

        let note = match translation {
            Some(TranslationState::Done) if spanish => Some((Icon::Translate, "Traducido")),
            Some(TranslationState::Failed) => Some((Icon::Warn, "No se pudo traducir")),
            _ => None,
        }
        .map(|(glyph, text)| {
            div().flex().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(p.text3)
                    .border_1()
                    .border_color(p.divider)
                    .rounded(px(14.))
                    .pl(px(8.))
                    .pr(px(10.))
                    .py(px(3.))
                    .mb(px(22.))
                    .child(icon(glyph, px(14.), p.text3))
                    .child(text),
            )
        });

        let body = match (&self.load, entry) {
            (_, Some(entry)) => match shown_spanish {
                // Traducción a medias: cada grupo pendiente sale aparte, en gris.
                Some(html) if pending => div()
                    .text_size(body_size)
                    .line_height(relative(1.65))
                    .children(
                        split_pending(html)
                            .into_iter()
                            .enumerate()
                            .map(|(ix, piece)| {
                                let id =
                                    SharedString::from(format!("body-{}-partial-{ix}", summary.id));
                                let view =
                                    TextView::html(id, code_in_pre(&piece.html)).selectable(true);
                                if piece.pending {
                                    view.text_color(p.text3).into_any_element()
                                } else {
                                    view.into_any_element()
                                }
                            }),
                    )
                    .into_any_element(),
                _ => {
                    let html = code_in_pre(shown_spanish.unwrap_or(entry.content.as_str()));
                    let id = SharedString::from(format!("body-{}-{}", summary.id, spanish));
                    // El color va en el propio visor: el del contenedor no llega a su texto.
                    let view = TextView::html(id, html).selectable(true);
                    let view = if pending {
                        view.text_color(p.text3)
                    } else {
                        view
                    };
                    div()
                        .text_size(body_size)
                        .line_height(relative(1.65))
                        .child(view)
                        .into_any_element()
                }
            },
            (Load::Failed(error), None) => div()
                .text_size(px(14.))
                .text_color(p.text2)
                .child(error.clone())
                .into_any_element(),
            _ => div()
                .text_size(px(14.))
                .text_color(p.text3)
                .child("Cargando…")
                .into_any_element(),
        };

        div()
            .id(ElementId::NamedInteger("article".into(), summary.id as u64))
            .flex_1()
            .min_h_0()
            .child(
                div().py(px(32.)).child(
                    div()
                        .max_w(px(640.))
                        .mx_auto()
                        .px(px(32.))
                        .child(header)
                        .children(note)
                        .child(body)
                        // Vacío si el artículo no es un paper de arXiv.
                        .child(self.full_text.clone()),
                ),
            )
            .overflow_y_scrollbar()
    }
}

/// Barra fina de avance bajo la barra superior mientras se traduce. También
/// la usa el lector de papers. No se
/// sabe cuánto falta, así que un tramo recorre la barra de lado a lado.
pub fn progress(p: &Palette) -> impl IntoElement {
    div()
        .h(px(2.))
        .flex_none()
        .relative()
        .overflow_hidden()
        .bg(p.surface2)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(relative(0.35))
                .bg(p.accent)
                .with_animation(
                    "progress",
                    Animation::new(Duration::from_millis(1400)).repeat(),
                    |d, t| d.left(relative(t * 1.35 - 0.35)),
                ),
        )
}

/// Barra fina con el avance real de la traducción y su texto, bajo la
/// cabecera: «Traduciendo… 12 de 52». Solo con `progress` del servicio.
pub fn translation_progress(progress: Progress, p: &Palette) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .px(px(16.))
        .py(px(6.))
        .child(
            div().flex_1().h(px(2.)).bg(p.surface2).child(
                div()
                    .h_full()
                    .w(relative(progress_fraction(progress)))
                    .bg(p.accent),
            ),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(12.5))
                .text_color(p.text2)
                .child(progress_text(progress)),
        )
}

/// Aviso flotante abajo y en el centro (`.toast`).
pub fn toast(p: &Palette) -> impl IntoElement {
    div()
        .absolute()
        .bottom(px(22.))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .bg(p.accent)
                .text_color(p.on_accent)
                .text_size(px(13.))
                .px(px(16.))
                .py(px(8.))
                .rounded(px(18.))
                .child(icon(Icon::Translate, px(16.), p.on_accent))
                .child("Traduciendo…"),
        )
}

impl Render for Reader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let column = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .relative();
        let Some(summary) = self.summary.clone() else {
            return column.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.text3)
                    .child("Elige un artículo de la lista"),
            );
        };
        let pending =
            matches!(&self.load, Load::Ready(e) if e.translation == TranslationState::Pending);
        // Con `progress` se muestra el avance real; sin él, la barra de siempre.
        let counted = match &self.load {
            Load::Ready(e) if e.translation == TranslationState::Pending => e.progress,
            _ => None,
        };
        column
            .child(
                self.top_bar(&summary, &p, cx)
                    .when(pending, |d| d.border_b_0()),
            )
            .when(pending, |d| match counted {
                Some(counted) => d.child(translation_progress(counted, &p)),
                None => d.child(progress(&p)),
            })
            .when_some(self.action_error.clone(), |d, error| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(16.))
                        .py(px(8.))
                        .text_size(px(13.))
                        .text_color(p.text2)
                        .bg(p.surface)
                        .child(error),
                )
            })
            .child(self.article(&summary, &p, crate::settings::font_size(cx)))
            .when(pending && counted.is_none(), |d| d.child(toast(&p)))
    }
}
