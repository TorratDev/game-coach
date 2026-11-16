//! Spike: a libretro core's video in a Rune `NativeSurface`, with a live read of its system RAM
//! beside it. `cargo run -- <core.so|.dll> <rom>`
mod libretro;

use rune::native_surface::{ByteFrame, ColorInfo, Format};
use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, Key, KeyEvent, NativeSurface, SurfaceFit, Tx, Waker, col, el, row, text};
use std::fmt::Write;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};

struct State {
    surface: NativeSurface,
    /// Written by the core's thread, copied into `panel` by `poll`.
    shared: Arc<Mutex<String>>,
    panel: String,
}

fn state() -> State {
    State { surface: NativeSurface::new(), shared: Arc::default(), panel: "usage: game-coach <core> <rom>".into() }
}

fn hex_dump(ram: &[u8]) -> String {
    let mut s = format!("system RAM, {} bytes\n", ram.len());
    for (i, line) in ram.chunks(8).take(24).enumerate() {
        let _ = write!(s, "{:04x} ", i * 8);
        for b in line {
            let _ = write!(s, " {b:02x}");
        }
        s.push('\n');
    }
    s
}

fn setup(s: &mut State, wake: Waker) {
    s.surface.set_waker(wake.clone());
    let [core, rom] = rune::extra_args() else { return };
    let (surface, shared, shared_err) = (s.surface.clone(), s.shared.clone(), s.shared.clone());
    let wake_ram = wake.clone();
    std::thread::spawn(move || {
        let mut id = 0;
        let frame = move |bgra: Vec<u8>, width, height| {
            id += 1;
            let _ = surface.push_bytes(ByteFrame { width, height, format: Format::Bgra, color: ColorInfo::default(), planes: [bgra, Vec::new()] }, id);
        };
        let ram = |ram: &[u8]| {
            *shared.lock().unwrap() = hex_dump(ram);
            wake_ram();
        };
        if let Err(e) = libretro::run(core, rom, frame, ram) {
            *shared_err.lock().unwrap() = e;
            wake();
        }
    });
}

fn poll(s: &mut State, _: f64, _: &mut Tx) -> bool {
    let shared = s.shared.lock().unwrap();
    let changed = !shared.is_empty() && *shared != s.panel;
    if changed {
        s.panel.clone_from(&shared);
    }
    changed
}

/// Arrows = d-pad, X = A, Z = B, A/S = L/R, Enter = Start, Backspace = Select.
fn key(e: &KeyEvent, down: bool) -> bool {
    let id = match &e.key {
        Key::Up => 4,
        Key::Down => 5,
        Key::Left => 6,
        Key::Right => 7,
        Key::Enter => 3,
        Key::Backspace => 2,
        Key::Text(t) => match t.to_ascii_lowercase().as_str() {
            "z" => 0,
            "x" => 8,
            "a" => 10,
            "s" => 11,
            _ => return false,
        },
        _ => return false,
    };
    if down { libretro::JOYPAD.fetch_or(1 << id, Relaxed) } else { libretro::JOYPAD.fetch_and(!(1 << id), Relaxed) };
    true
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let game = el().key("game").w(720.0).h(480.0).native_surface(&s.surface, SurfaceFit::Fit);
    let panel = col().key("panel").w(280.0).padding(12.0).child(text(s.panel.as_str()).font(type_::BODY.mono()).color(t.text));
    row().w_full().h_full().fill(t.bg).on_key_up(|_: &mut State, e, _| key(e, false)).child(game).child(panel)
}

fn main() {
    rune::run(AppSpec { setup: Some(setup), poll: Some(poll), on_key: Some(|_, e, _| key(e, true)), ..AppSpec::new("Game coach spike", (1000.0, 480.0), state, view) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn holding_x_holds_the_a_button_until_release() {
        let mut rt = Runtime::new(state(), view, (1000.0, 480.0), Env { system_dark: true, scale: 1.0 });
        rt.on_key = Some(|_, e, _| key(e, true));
        rt.frame(0.0);
        let x = || KeyEvent { key: Key::Text("x".into()), mods: Default::default() };
        rt.key(x());
        rt.frame(1.0 / 60.0); // keys are queued until the next frame
        assert_eq!(libretro::JOYPAD.load(Relaxed) & (1 << 8), 1 << 8);
        assert!(rt.key_up(x()), "the root hears releases");
        rt.frame(1.0 / 60.0);
        assert_eq!(libretro::JOYPAD.load(Relaxed) & (1 << 8), 0);
    }
}
