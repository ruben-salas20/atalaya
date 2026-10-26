//! Desplazamiento suave con la rueda en los lectores (#89).
//!
//! GPUI aplica cada paso de la rueda (`ScrollDelta::Lines`) de golpe, y el
//! `ScrollBounce` de gpui-base, que es el área de `overflow_y_scrollbar`, lo
//! hace igual en fase de captura. Aquí un listener de captura, pintado antes
//! que el área, se queda con esos pasos: los suma al destino y anima el
//! desplazamiento hasta él en 120 ms. El touchpad (`ScrollDelta::Pixels`) no
//! se toca, porque ya es suave: solo para una animación a medias.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    App, DispatchPhase, Div, ElementId, ScrollDelta, ScrollHandle, ScrollWheelEvent, Stateful,
    Window, canvas, div, point, px,
};
use gpui_base::{InteractiveElementExt as _, ScrollBounce};
use gpui_component::scroll::{Scrollbar, ScrollbarAxis};

/// Lo que dura la animación de un paso de rueda.
const DURATION: Duration = Duration::from_millis(120);
/// Cada cuánto se repinta mientras se anima (unos 60 fotogramas por segundo).
const FRAME: Duration = Duration::from_millis(16);

/// Una animación de desplazamiento de `from` a `to`, en el sentido de GPUI:
/// 0 arriba, cada vez más negativo hacia abajo.
#[derive(Default)]
pub struct WheelAnimation {
    from: f32,
    to: f32,
    start: Option<Instant>,
}

impl WheelAnimation {
    pub fn is_running(&self) -> bool {
        self.start.is_some()
    }

    /// Suma `delta` al destino de la animación. Si ya hay una en curso, parte
    /// de donde está ahora y el destino acumula los pasos; si no, parte de
    /// `current`. El destino nunca pasa de `min` (el fondo, negativo) ni de 0.
    pub fn step(&mut self, current: f32, delta: f32, min: f32, now: Instant) {
        let at = self.value(now).unwrap_or(current);
        let base = if self.is_running() { self.to } else { current };
        self.from = at;
        self.to = (base + delta).clamp(min, 0.0);
        self.start = Some(now);
    }

    /// La posición en `now`, o `None` si no hay animación. Al llegar al final
    /// da el destino y la deja parada.
    pub fn value(&mut self, now: Instant) -> Option<f32> {
        let start = self.start?;
        let t = now.saturating_duration_since(start).as_secs_f32() / DURATION.as_secs_f32();
        if t >= 1.0 {
            self.start = None;
            return Some(self.to);
        }
        Some(self.from + (self.to - self.from) * ease_out_cubic(t))
    }

    /// Para la animación en el sitio en el que esté.
    pub fn cancel(&mut self) {
        self.start = None;
    }
}

/// La curva de la animación: sale rápida y frena al llegar, sin tirones.
pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Lo que comparten el área y su bucle de animación.
#[derive(Default)]
struct Wheel {
    anim: WheelAnimation,
    /// Hay un bucle moviendo el área.
    driving: bool,
    /// La última posición que puso el bucle. Si el área tiene otra, alguien
    /// más la movió (la barra, por ejemplo) y la animación se para.
    last: f32,
}

/// El desplazamiento de un lector: el `ScrollHandle` del área y el estado de
/// la animación de la rueda.
#[derive(Clone, Default)]
pub struct SmoothScroll {
    handle: ScrollHandle,
    wheel: Rc<RefCell<Wheel>>,
}

impl SmoothScroll {
    /// Vuelve arriba, al abrir otro artículo.
    pub fn reset(&self) {
        self.handle.set_offset(point(px(0.), px(0.)));
        self.wheel.borrow_mut().anim.cancel();
    }

    /// Un área con desplazamiento vertical y barra para `child`. Quien la
    /// usa le pone el tamaño y el hueco (`flex_1`, `min_h_0`). `id` debe ser
    /// único dentro de la vista.
    pub fn vertical(&self, id: &'static str, child: impl IntoElement) -> Stateful<Div> {
        let root = ElementId::from(id);
        let area = div()
            .id((root.clone(), "area"))
            .size_full()
            .flex()
            .flex_col()
            .track_scroll(&self.handle)
            .overflow_y_scroll()
            // Un gesto horizontal no debe mover esta área en vertical.
            .lock_scroll_axis()
            .child(
                div()
                    .id((root.clone(), "content"))
                    .flex_none()
                    .h_auto()
                    .min_h_full()
                    .child(child),
            );
        div()
            .id(root.clone())
            .relative()
            .size_full()
            // Va primero: su listener de captura se ejecuta antes que el del
            // área, y al parar la propagación el área no aplica el paso.
            .child(self.wheel_listener())
            .child(ScrollBounce::new(
                (root.clone(), "bounce"),
                &self.handle,
                area,
            ))
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::new(&self.handle)
                        .id((root.clone(), "scrollbar"))
                        .axis(ScrollbarAxis::Vertical)
                        .viewport_from_layout(),
                ),
            )
    }

    /// Un listener de captura en toda el área, que se queda con los pasos de
    /// la rueda en vertical.
    fn wheel_listener(&self) -> impl IntoElement {
        let handle = self.handle.clone();
        let wheel = self.wheel.clone();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let handle = handle.clone();
                let wheel = wheel.clone();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || !bounds.contains(&event.position) {
                        return;
                    }
                    if !matches!(event.delta, ScrollDelta::Lines(_)) {
                        // Touchpad: lo lleva GPUI, y una animación a medias se para.
                        wheel.borrow_mut().anim.cancel();
                        return;
                    }
                    let delta = event.delta.pixel_delta(window.line_height());
                    if delta.y == px(0.) || delta.x.abs() > delta.y.abs() {
                        return;
                    }
                    cx.stop_propagation();

                    let max = handle.max_offset().y.max(px(0.));
                    let current = handle.offset().y.as_f32();
                    let start_driver = {
                        let mut state = wheel.borrow_mut();
                        state.last = current;
                        state
                            .anim
                            .step(current, delta.y.as_f32(), -max.as_f32(), Instant::now());
                        let start = !state.driving;
                        state.driving = true;
                        start
                    };
                    if start_driver {
                        drive(window, cx, handle.clone(), wheel.clone());
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}

/// Mueve el área cada `FRAME` mientras haya animación.
fn drive(window: &Window, cx: &App, handle: ScrollHandle, wheel: Rc<RefCell<Wheel>>) {
    window
        .spawn(cx, async move |cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                match cx.update(|window, _| tick(window, &handle, &wheel)) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        })
        .detach();
}

/// Un fotograma de la animación. Devuelve si sigue.
fn tick(window: &mut Window, handle: &ScrollHandle, wheel: &RefCell<Wheel>) -> bool {
    let mut state = wheel.borrow_mut();
    if (handle.offset().y.as_f32() - state.last).abs() > 0.5 {
        state.anim.cancel();
    }
    if let Some(y) = state.anim.value(Instant::now()) {
        let mut offset = handle.offset();
        offset.y = px(y);
        handle.set_offset(offset);
        state.last = y;
        window.refresh();
    }
    state.driving = state.anim.is_running();
    state.driving
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn la_curva_empieza_en_0_acaba_en_1_y_sube_siempre() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!((ease_out_cubic(0.5) - 0.875).abs() < 1e-6);
        let mut last = 0.0;
        for i in 1..=100 {
            let v = ease_out_cubic(i as f32 / 100.0);
            assert!(v >= last, "la curva baja en {i}");
            last = v;
        }
    }

    #[test]
    fn un_paso_llega_a_su_destino_en_120_ms() {
        let t0 = Instant::now();
        let mut anim = WheelAnimation::default();
        anim.step(0.0, -40.0, -1000.0, t0);
        assert!(anim.is_running());
        assert_eq!(anim.value(t0), Some(0.0));
        // A la mitad del tiempo ya va por delante de la mitad del camino.
        let half = anim.value(t0 + ms(60)).unwrap();
        assert!((half - -35.0).abs() < 1e-4, "{half}");
        assert_eq!(anim.value(t0 + ms(120)), Some(-40.0));
        assert!(!anim.is_running());
        assert_eq!(anim.value(t0 + ms(200)), None);
    }

    #[test]
    fn los_pasos_que_llegan_durante_la_animacion_se_acumulan() {
        let t0 = Instant::now();
        let mut anim = WheelAnimation::default();
        anim.step(0.0, -40.0, -1000.0, t0);
        let t1 = t0 + ms(60);
        let before = anim.value(t1).unwrap();
        // El segundo paso no salta: sigue desde donde iba la animación.
        anim.step(before, -40.0, -1000.0, t1);
        assert!((anim.value(t1).unwrap() - before).abs() < 1e-4);
        // El destino suma los dos pasos.
        assert_eq!(anim.value(t1 + ms(120)), Some(-80.0));
    }

    #[test]
    fn el_destino_no_pasa_del_principio_ni_del_final() {
        let t0 = Instant::now();
        let mut anim = WheelAnimation::default();
        anim.step(-950.0, -100.0, -1000.0, t0);
        assert_eq!(anim.value(t0 + ms(120)), Some(-1000.0));

        let mut anim = WheelAnimation::default();
        anim.step(-10.0, 50.0, -1000.0, t0);
        assert_eq!(anim.value(t0 + ms(120)), Some(0.0));
    }

    #[test]
    fn cancelar_para_en_el_sitio() {
        let t0 = Instant::now();
        let mut anim = WheelAnimation::default();
        anim.step(0.0, -40.0, -1000.0, t0);
        let at = anim.value(t0 + ms(60)).unwrap();
        anim.cancel();
        assert!(!anim.is_running());
        assert_eq!(anim.value(t0 + ms(70)), None);
        assert!(at < 0.0 && at > -40.0);
    }
}
