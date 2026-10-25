//! Actualización de la app (ADR 0012): el estado, el aviso de arriba de la
//! pantalla de lectura y la sección «Versión» de los ajustes. El trabajo de
//! red y de disco está en `update.rs`; aquí solo se espera y se pinta.
//!
//! `Updates` se crea al abrir la ventana. Mira si toca comprobar (como mucho
//! cada 6 horas) y el aviso y los ajustes lo observan para repintarse.

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{Context, Entity, Subscription, Task, div, px};
use gpui_component::text::TextView;

use crate::assets::Icon;
use crate::palette::Palette;
use crate::ui::feeds::capitalize;
use crate::ui::widgets::{icon_button, pill};
use crate::update::{self, Check, Release};

pub enum UpdateStatus {
    /// Todavía no se ha mirado, o no hay nada que decir.
    Idle,
    /// Buscando la última versión (solo con «Buscar actualizaciones»).
    Checking,
    /// La última versión publicada es la que está instalada.
    Current,
    Available(Release),
    Downloading(Release),
    /// Lista para usarse: falta reiniciar.
    Installed(Release),
    /// La comprobación falló. Solo se enseña en los ajustes.
    CheckFailed(String),
    /// La instalación falló. Se enseña también en el aviso.
    InstallFailed(String),
}

pub struct Updates {
    /// Dónde está el programa. Se mira al arrancar: después de instalar, Linux
    /// marca el `current_exe` como «(deleted)» y ya no sirve para reiniciar.
    exe: Option<PathBuf>,
    status: UpdateStatus,
    /// Rubén cerró el aviso. Vuelve con la próxima versión que se encuentre
    /// o al pulsar «Actualizar» en los ajustes.
    dismissed: bool,
    _task: Task<()>,
}

impl Updates {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            exe: std::env::current_exe().ok(),
            status: UpdateStatus::Idle,
            dismissed: false,
            _task: Task::ready(()),
        };
        this.open(cx);
        this
    }

    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    pub fn dismissed(&self) -> bool {
        self.dismissed
    }

    /// Al abrir la app. Lo último que se encontró se muestra al momento, sin
    /// red. La comprobación en la red solo se hace si la última es de hace
    /// 6 horas o más. Si falla, no molesta: se intenta en la próxima apertura.
    fn open(&mut self, cx: &mut Context<Self>) {
        let saved = update::load_check();
        if let Some(check) = &saved
            && check.release.offers_update(update::current())
        {
            self.status = UpdateStatus::Available(check.release.clone());
        }
        let due = saved
            .as_ref()
            .is_none_or(|check| update::check_due(check.checked_at, update::now_secs()));
        if due {
            self.check(false, cx);
        }
    }

    /// «Buscar actualizaciones» de los ajustes: mira ya.
    pub fn check_now(&mut self, cx: &mut Context<Self>) {
        self.check(true, cx);
    }

    /// `forced` viene de los ajustes: ahí se enseña «Buscando…» y el error.
    fn check(&mut self, forced: bool, cx: &mut Context<Self>) {
        if matches!(
            self.status,
            UpdateStatus::Checking | UpdateStatus::Downloading(_)
        ) {
            return;
        }
        if forced {
            self.status = UpdateStatus::Checking;
            cx.notify();
        }
        self._task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let release = update::fetch_latest()?;
                    update::save_check(&Check {
                        checked_at: update::now_secs(),
                        release: release.clone(),
                    });
                    Ok::<Release, String>(release)
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_check(result, forced);
                cx.notify();
            })
            .ok();
        });
    }

    fn finish_check(&mut self, result: Result<Release, String>, forced: bool) {
        // Una comprobación que acaba tarde no pisa una instalación en marcha.
        if matches!(
            self.status,
            UpdateStatus::Downloading(_) | UpdateStatus::Installed(_)
        ) {
            return;
        }
        match result {
            Ok(release) if release.offers_update(update::current()) => {
                if !matches!(self.status, UpdateStatus::Available(_)) {
                    self.dismissed = false;
                }
                self.status = UpdateStatus::Available(release);
            }
            Ok(_) => self.status = UpdateStatus::Current,
            Err(error) if forced => self.status = UpdateStatus::CheckFailed(error),
            Err(_) => {}
        }
    }

    /// «Actualizar»: baja la versión, la comprueba y la instala. Todo en
    /// segundo plano; la ventana sigue respondiendo.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        let UpdateStatus::Available(release) = &self.status else {
            return;
        };
        let release = release.clone();
        let installing = release.clone();
        self.status = UpdateStatus::Downloading(release.clone());
        self.dismissed = false;
        cx.notify();
        let exe = self.exe.clone();
        self._task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let exe = exe.ok_or("No se sabe dónde está el programa")?;
                    update::install(&release, &exe)
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(()) => UpdateStatus::Installed(installing),
                    Err(error) => UpdateStatus::InstallFailed(error),
                };
                cx.notify();
            })
            .ok();
        });
    }

    /// «Reiniciar»: abre la copia nueva y cierra esta.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.status, UpdateStatus::Installed(_)) {
            return;
        }
        let Some(exe) = &self.exe else {
            self.status = UpdateStatus::InstallFailed("No se sabe dónde está el programa".into());
            cx.notify();
            return;
        };
        match std::process::Command::new(exe).spawn() {
            Ok(_) => cx.quit(),
            Err(error) => {
                self.status = UpdateStatus::InstallFailed(format!("No se pudo reiniciar: {error}"));
                cx.notify();
            }
        }
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.dismissed = true;
        cx.notify();
    }
}

/// Lo que dice la sección «Versión» de los ajustes, bajo «Actualizaciones».
pub fn settings_text(status: &UpdateStatus) -> String {
    match status {
        UpdateStatus::Idle => "Se busca al abrir la app, como mucho cada 6 horas".into(),
        UpdateStatus::Checking => "Buscando…".into(),
        UpdateStatus::Current => "Estás al día".into(),
        UpdateStatus::Available(release) => format!("Hay una versión nueva: {}", release.version),
        UpdateStatus::Downloading(release) => format!("Descargando la {}…", release.version),
        UpdateStatus::Installed(release) => {
            format!("Actualizada. Reinicia para usar la {}", release.version)
        }
        UpdateStatus::CheckFailed(error) | UpdateStatus::InstallFailed(error) => capitalize(error),
    }
}

/// Aviso discreto arriba de la pantalla de lectura. No ocupa sitio si no hay
/// nada que decir.
pub struct UpdateBanner {
    updates: Entity<Updates>,
    notes_open: bool,
    _observe: Subscription,
}

impl UpdateBanner {
    pub fn new(updates: Entity<Updates>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&updates, |_, _, cx| cx.notify());
        Self {
            updates,
            notes_open: false,
            _observe: observe,
        }
    }
}

impl Render for UpdateBanner {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::of(window);
        let updates = self.updates.read(cx);
        if updates.dismissed() {
            return div().into_any_element();
        }
        let (message, action, notes) = match updates.status() {
            UpdateStatus::Available(release) => (
                format!("Atalaya {} disponible", release.version),
                Some(
                    pill("banner-install", "Actualizar", &p)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.updates.update(cx, |updates, cx| updates.install(cx));
                        }))
                        .into_any_element(),
                ),
                Some(release.notes.clone()),
            ),
            UpdateStatus::Downloading(release) => (
                format!("Descargando Atalaya {}…", release.version),
                None,
                None,
            ),
            UpdateStatus::Installed(release) => (
                format!("Actualizada. Reinicia para usar la {}", release.version),
                Some(
                    pill("banner-restart", "Reiniciar", &p)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.updates.update(cx, |updates, cx| updates.restart(cx));
                        }))
                        .into_any_element(),
                ),
                None,
            ),
            UpdateStatus::InstallFailed(error) => {
                (format!("No se pudo actualizar: {error}"), None, None)
            }
            UpdateStatus::Idle
            | UpdateStatus::Checking
            | UpdateStatus::Current
            | UpdateStatus::CheckFailed(_) => {
                return div().into_any_element();
            }
        };
        // Sin novedades no hay desplegable.
        let notes = notes.filter(|text| !text.trim().is_empty());
        let notes_open = self.notes_open;
        let notes_toggle = notes.as_ref().map(|_| {
            pill(
                "banner-notes",
                if notes_open { "Ocultar" } else { "Novedades" },
                &p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.notes_open = !this.notes_open;
                cx.notify();
            }))
        });
        let close =
            icon_button("banner-close", Icon::Close, &p).on_click(cx.listener(|this, _, _, cx| {
                this.updates.update(cx, |updates, cx| updates.dismiss(cx));
            }));
        div()
            .flex_none()
            .px(px(18.))
            .py(px(8.))
            .bg(p.surface)
            .border_b_1()
            .border_color(p.divider)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .text_color(p.text2)
                            .child(message),
                    )
                    .children(notes_toggle)
                    .children(action)
                    .child(close),
            )
            .when_some(notes.filter(|_| notes_open), |d, notes| {
                d.child(
                    div()
                        .id("update-notes")
                        .mt(px(8.))
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .text_size(px(13.))
                        .text_color(p.text2)
                        .child(TextView::markdown("update-notes-text", notes).selectable(true)),
                )
            })
            .into_any_element()
    }
}
