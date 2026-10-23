//! Ratón virtual para un compositor wlroots (sway) sin pantalla.
//!
//! `wlrctl` crea un ratón nuevo en cada orden y lo borra al acabar, así que
//! el clic llega antes de que la app registre el ratón. Este programa mantiene
//! uno vivo y lee órdenes por la entrada estándar, una por línea:
//!
//! - `move X Y`: lleva el puntero a la posición X, Y (píxeles de la salida).
//! - `click [left|right|middle]`: pulsa y suelta un botón.
//! - `scroll N`: gira la rueda N pasos; positivo baja.
//! - `sleep MS`: espera.

use std::io::BufRead;
use std::time::{Duration, Instant};

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ZwlrVirtualPointerV1);

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

fn main() {
    let (width, height) = parse_size(std::env::args().nth(1).as_deref().unwrap_or("1440x900"));
    let conn = Connection::connect_to_env().expect("no hay WAYLAND_DISPLAY al que conectar");
    let (globals, mut queue) = registry_queue_init::<State>(&conn).expect("registro de Wayland");
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).expect("el compositor no tiene seat");
    let manager: ZwlrVirtualPointerManagerV1 = globals
        .bind(&qh, 1..=2, ())
        .expect("el compositor no tiene zwlr_virtual_pointer_manager_v1");
    let pointer = manager.create_virtual_pointer(Some(&seat), &qh, ());
    let mut state = State;
    queue.roundtrip(&mut state).expect("roundtrip");

    let start = Instant::now();
    let now = || start.elapsed().as_millis() as u32;

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["move", x, y] => {
                let (x, y): (u32, u32) = (x.parse().unwrap_or(0), y.parse().unwrap_or(0));
                pointer.motion_absolute(now(), x, y, width, height);
                pointer.frame();
            }
            ["click", rest @ ..] => {
                let button = match rest.first().copied() {
                    Some("right") => BTN_RIGHT,
                    Some("middle") => BTN_MIDDLE,
                    _ => BTN_LEFT,
                };
                pointer.button(now(), button, wl_pointer::ButtonState::Pressed);
                pointer.frame();
                conn.flush().ok();
                std::thread::sleep(Duration::from_millis(60));
                pointer.button(now(), button, wl_pointer::ButtonState::Released);
                pointer.frame();
            }
            ["scroll", steps] => {
                // Un paso de rueda real: 15 unidades y un «discrete», que es lo que esperan las apps.
                let steps: i32 = steps.parse().unwrap_or(0);
                for _ in 0..steps.unsigned_abs() {
                    let sign = steps.signum();
                    pointer.axis_source(wl_pointer::AxisSource::Wheel);
                    pointer.axis_discrete(now(), wl_pointer::Axis::VerticalScroll, 15.0 * sign as f64, sign);
                    pointer.frame();
                    queue.roundtrip(&mut state).expect("roundtrip");
                }
            }
            ["sleep", ms] => std::thread::sleep(Duration::from_millis(ms.parse().unwrap_or(0))),
            [] => {}
            _ => eprintln!("orden desconocida: {line}"),
        }
        queue.roundtrip(&mut state).expect("roundtrip");
    }
    pointer.destroy();
    queue.roundtrip(&mut state).ok();
}

fn parse_size(s: &str) -> (u32, u32) {
    let (w, h) = s.split_once('x').unwrap_or(("1440", "900"));
    (w.parse().unwrap_or(1440), h.parse().unwrap_or(900))
}
