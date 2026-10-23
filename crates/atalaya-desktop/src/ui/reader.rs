//! Columna 3: el artículo abierto (L5, L6), con `GET /api/entries/{id}`.
//!
//! Se abre en español si la traducción está lista. Si no, muestra el original
//! en gris con el aviso «Traduciendo…» y vuelve a pedir el artículo cada
//! pocos segundos hasta que cambia solo. La barra tiene el selector
//! «Español» / «Original» y los botones de leído, guardar y abrir en el
//! navegador. En el PC no hay deslizamientos (L4): eso es del móvil.

use std::time::Duration;

use atalaya_api::{Entry, EntrySummary, EntryUpdate, Id, TranslationState};
use chrono::Utc;
use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, Context, ElementId, EventEmitter, FontWeight, Pixels, SharedString,
    Task, Window, div, px, relative,
};
use gpui_component::scroll::ScrollableElement;
use gpui_component::text::TextView;
use gpui_component::tooltip::Tooltip;

use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::format::ago;
use crate::ui::widgets::{bar, icon, icon_button};

/// Cada cuánto se vuelve a pedir un artículo que se está traduciendo.
const POLL_EVERY: Duration = Duration::from_secs(3);

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

pub struct Reader {
    /// La fila elegida: con ella se pinta la cabecera mientras llega el resto.
    summary: Option<EntrySummary>,
    load: Load,
    /// Lo que eligió el selector. Solo cuenta si hay traducción.
    lang: Lang,
    /// Fallo al marcar leído o guardado, que se enseña bajo la barra.
    action_error: Option<String>,
    _fetch: Task<()>,
}

impl EventEmitter<ReaderEvent> for Reader {}

impl Default for Reader {
    fn default() -> Self {
        Self {
            summary: None,
            load: Load::Loading,
            lang: Lang::Spanish,
            action_error: None,
            _fetch: Task::ready(()),
        }
    }
}

impl Reader {
    pub fn show(&mut self, entry: EntrySummary, cx: &mut Context<Self>) {
        let id = entry.id;
        let unread = !entry.read;
        self.summary = Some(entry);
        self.load = Load::Loading;
        self.lang = Lang::Spanish;
        self.action_error = None;
        self.fetch(id, Duration::ZERO, cx);
        // Abrir marca como leído; el botón de la barra sirve para deshacerlo.
        // Sin red no se intenta: solo daría un error al leer la copia.
        let online = super::client(cx).is_ok_and(|client| !client.is_offline());
        if unread && online {
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

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.summary = None;
        self.load = Load::Loading;
        self._fetch = Task::ready(());
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
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.update_entry(id, &update) })
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

    fn set_summary(&mut self, summary: EntrySummary, cx: &mut Context<Self>) {
        if let Load::Ready(entry) = &mut self.load {
            entry.summary.read = summary.read;
            entry.summary.starred = summary.starred;
        }
        self.summary = Some(summary);
        cx.notify();
    }

    fn top_bar(&self, summary: &EntrySummary, p: &Palette, cx: &mut Context<Self>) -> gpui::Div {
        let translation = match &self.load {
            Load::Ready(entry) => Some(entry.translation),
            _ => None,
        };
        let translated = translation == Some(TranslationState::Done);
        let on = if translated {
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
                    .when(!translated, |d| d.opacity(0.45))
                    .child(self.segment("Español", Lang::Spanish, on, translated, p, cx))
                    .child(self.segment("Original", Lang::Original, on, translated, p, cx)),
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
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
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
        parts.push(format!("hace {}", ago(summary.published_at, Utc::now())));
        parts.join(" · ")
    }

    fn article(&self, summary: &EntrySummary, p: &Palette, body_size: Pixels) -> impl IntoElement {
        let entry = match &self.load {
            Load::Ready(entry) => Some(entry.as_ref()),
            _ => None,
        };
        let translation = entry.map(|e| e.translation);
        let spanish = translation == Some(TranslationState::Done) && self.lang == Lang::Spanish;
        let pending = translation == Some(TranslationState::Pending);
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
            (_, Some(entry)) => {
                let html = if spanish {
                    entry.translated_content.clone().unwrap_or_default()
                } else {
                    entry.content.clone()
                };
                let id = SharedString::from(format!("body-{}-{}", summary.id, spanish));
                div()
                    .text_size(body_size)
                    .line_height(relative(1.65))
                    .when(pending, |d| d.text_color(p.text3))
                    .child(TextView::html(id, html).selectable(true))
                    .into_any_element()
            }
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
                        .child(body),
                ),
            )
            .overflow_y_scrollbar()
    }
}

/// Barra fina de avance bajo la barra superior mientras se traduce. No se
/// sabe cuánto falta, así que un tramo recorre la barra de lado a lado.
fn progress(p: &Palette) -> impl IntoElement {
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

/// Aviso flotante abajo y en el centro (`.toast`).
fn toast(p: &Palette) -> impl IntoElement {
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
        column
            .child(
                self.top_bar(&summary, &p, cx)
                    .when(pending, |d| d.border_b_0()),
            )
            .when(pending, |d| d.child(progress(&p)))
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
            .when(pending, |d| d.child(toast(&p)))
    }
}
