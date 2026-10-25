//! Pantalla de ajustes (S3), la 3 de `maquetas-pc.html`: ocupa todo lo que
//! queda a la derecha del menú lateral.
//!
//! «Probar» usa lo que hay escrito, aunque no esté guardado. «Guardar»
//! escribe el archivo y sustituye la [`super::Connection`]; la ventana lo
//! nota y recarga el menú y la lista. El tamaño de letra se guarda en cuanto
//! se elige. La dirección de los gestos es solo del móvil: aquí no está.

use std::sync::Arc;

use atalaya_api::Status;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Subscription, Task, Window, div, px};
use gpui_component::Sizable;
use gpui_component::input::{Input, InputState};
use gpui_component::select::{Select, SelectEvent, SelectState};

use crate::client::ApiClient;
use crate::palette::Palette;
use crate::settings::{FONT_SIZES, FontSize, Settings};
use crate::ui::Connection;
use crate::ui::feeds::capitalize;
use crate::ui::updates::{UpdateStatus, Updates, settings_text};
use crate::ui::widgets::{bar, pill};
use crate::update::CURRENT;

type SizeList = Vec<SharedString>;

enum Probe {
    Idle,
    Running,
    Done(Result<Status, String>),
}

pub struct SettingsScreen {
    /// Lo último que se leyó del archivo o se guardó en él.
    saved: Settings,
    url: Entity<InputState>,
    key: Entity<InputState>,
    font_size: Entity<SelectState<SizeList>>,
    updates: Entity<Updates>,
    probe: Probe,
    /// Resultado de la última escritura: `Ok` con un aviso o `Err`.
    notice: Option<Result<String, String>>,
    _probe: Task<()>,
    _subscriptions: Vec<Subscription>,
}

fn size_label(size: u32) -> SharedString {
    format!("{size} px").into()
}

impl SettingsScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, updates: Entity<Updates>) -> Self {
        let (saved, notice) = match Settings::read() {
            Ok(s) => (s, None),
            Err(error) => (Settings::default(), Some(Err(error))),
        };
        let url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://atalaya.rubensalas.dev")
                .default_value(saved.service_url.clone())
        });
        let key = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .default_value(saved.api_key.clone())
        });
        let sizes: SizeList = FONT_SIZES.iter().copied().map(size_label).collect();
        let selected = FONT_SIZES
            .iter()
            .position(|s| *s == saved.font_size)
            .map(gpui_component::IndexPath::new);
        let font_size = cx.new(|cx| SelectState::new(sizes, selected, window, cx));
        let subscriptions = vec![
            cx.subscribe(&font_size, |this, _, event: &SelectEvent<SizeList>, cx| {
                let SelectEvent::Confirm(Some(label)) = event else {
                    return;
                };
                if let Some(size) = FONT_SIZES
                    .iter()
                    .copied()
                    .find(|s| size_label(*s) == *label)
                {
                    this.set_font_size(size, cx);
                }
            }),
            // La sección «Versión» se repinta cuando cambia la actualización.
            cx.observe(&updates, |_, _, cx| cx.notify()),
        ];
        Self {
            saved,
            url,
            key,
            font_size,
            updates,
            probe: Probe::Idle,
            notice,
            _probe: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    /// Los ajustes con lo que hay escrito en los campos.
    fn typed(&self, cx: &Context<Self>) -> Settings {
        // Lo que no está en esta pantalla, como los paneles plegados, sale
        // del archivo: puede haber cambiado desde que se abrió.
        let mut settings = Settings::read().unwrap_or_else(|_| self.saved.clone());
        settings.service_url = self.url.read(cx).value().trim().to_owned();
        settings.api_key = self.key.read(cx).value().trim().to_owned();
        settings.font_size = self.saved.font_size;
        settings
    }

    fn probe(&mut self, cx: &mut Context<Self>) {
        let typed = match self.typed(cx).validate() {
            Ok(s) => s,
            Err(error) => {
                self.probe = Probe::Done(Err(error));
                cx.notify();
                return;
            }
        };
        self.probe = Probe::Running;
        cx.notify();
        self._probe = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { ApiClient::new(&typed)?.status() })
                .await;
            this.update(cx, |this, cx| {
                this.probe = Probe::Done(result);
                cx.notify();
            })
            .ok();
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let typed = match self.typed(cx).validate() {
            Ok(s) => s,
            Err(error) => {
                self.notice = Some(Err(error));
                cx.notify();
                return;
            }
        };
        if let Err(error) = typed.save() {
            self.notice = Some(Err(error));
            cx.notify();
            return;
        }
        let connection = ApiClient::new(&typed).map(Arc::new);
        self.saved = typed;
        self.probe = Probe::Idle;
        self.notice = Some(Ok("Guardado".into()));
        cx.set_global(Connection(connection));
        cx.notify();
    }

    fn set_font_size(&mut self, size: u32, cx: &mut Context<Self>) {
        cx.set_global(FontSize(size));
        match Settings::update(|s| s.font_size = size) {
            Ok(_) => {
                self.saved.font_size = size;
                self.notice = None;
            }
            Err(error) => self.notice = Some(Err(error)),
        }
        cx.notify();
    }

    fn status_text(&self) -> SharedString {
        match &self.probe {
            Probe::Idle => "Sin probar".into(),
            Probe::Running => "Probando…".into(),
            Probe::Done(Ok(status)) if status.miniflux => {
                format!("Conectado · servicio {}", status.version).into()
            }
            Probe::Done(Ok(_)) => "El servicio responde, pero no llega a Miniflux".into(),
            Probe::Done(Err(error)) => capitalize(error).into(),
        }
    }
}

/// Título de un apartado (`.set h3`).
fn heading(text: &'static str, p: &Palette) -> impl IntoElement {
    div()
        .mt(px(22.))
        .mb(px(6.))
        .text_size(px(11.5))
        .text_color(p.text3)
        .child(text.to_uppercase())
}

/// Una fila de ajuste (`.f`): nombre, debajo una nota opcional, y a la
/// derecha el control.
fn field(
    label: &'static str,
    note: Option<gpui::AnyElement>,
    control: impl IntoElement,
    p: &Palette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .py(px(10.))
        .border_b_1()
        .border_color(p.divider)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(div().text_size(px(14.5)).child(label))
                .children(note.map(|n| {
                    div()
                        .mt(px(2.))
                        .text_size(px(12.5))
                        .text_color(p.text2)
                        .child(n)
                })),
        )
        .child(div().flex_none().child(control))
}

impl Render for SettingsScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let ok = matches!(&self.probe, Probe::Done(Ok(s)) if s.miniflux);
        let status = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .when(ok, |d| {
                d.child(div().flex_none().size(px(7.)).rounded_full().bg(p.text2))
            })
            .child(div().min_w_0().child(self.status_text()))
            .into_any_element();
        let updates = self.updates.read(cx);
        let update_action = match updates.status() {
            UpdateStatus::Available(_) => Some(
                pill("update-install", "Actualizar", &p)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.updates.update(cx, |updates, cx| updates.install(cx));
                    }))
                    .into_any_element(),
            ),
            UpdateStatus::Installed(_) => Some(
                pill("update-restart", "Reiniciar", &p)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.updates.update(cx, |updates, cx| updates.restart(cx));
                    }))
                    .into_any_element(),
            ),
            _ => None,
        };
        let update_note = div()
            .child(SharedString::from(settings_text(updates.status())))
            .into_any_element();
        let notice = self.notice.as_ref().map(|n| {
            let (text, color) = match n {
                Ok(text) => (text.clone(), p.text2),
                Err(error) => (error.clone(), p.text),
            };
            div()
                .mt(px(10.))
                .text_size(px(12.5))
                .text_color(color)
                .child(text)
        });
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                bar(&p).pl(px(40.)).child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Ajustes"),
                ),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .max_w(px(640.))
                            .px(px(40.))
                            .pt(px(8.))
                            .pb(px(32.))
                            .child(heading("Servicio Atalaya", &p))
                            .child(field(
                                "Dirección",
                                None,
                                Input::new(&self.url).small().w(px(260.)),
                                &p,
                            ))
                            .child(field(
                                "Clave de la API",
                                None,
                                Input::new(&self.key).small().mask_toggle().w(px(260.)),
                                &p,
                            ))
                            .child(field(
                                "Estado",
                                Some(status),
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        pill("probe", "Probar", &p)
                                            .on_click(cx.listener(|this, _, _, cx| this.probe(cx))),
                                    )
                                    .child(
                                        pill("save", "Guardar", &p)
                                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                                    ),
                                &p,
                            ))
                            .children(notice)
                            .child(heading("Lectura", &p))
                            .child(field(
                                "Tamaño de letra",
                                None,
                                div()
                                    .w(px(110.))
                                    .child(Select::new(&self.font_size).small()),
                                &p,
                            ))
                            .child(heading("Versión", &p))
                            .child(field("Versión instalada", None, CURRENT, &p))
                            .child(field(
                                "Actualizaciones",
                                Some(update_note),
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        pill("check-updates", "Buscar actualizaciones", &p)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.updates.update(cx, |updates, cx| {
                                                    updates.check_now(cx)
                                                });
                                            })),
                                    )
                                    .children(update_action),
                                &p,
                            )),
                    ),
            )
    }
}
