//! The least libretro frontend that shows video and reads system RAM. No audio, no input.
use std::ffi::{CString, c_char, c_uint, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::time::{Duration, Instant};

const SET_PIXEL_FORMAT: c_uint = 10;
const GET_CAN_DUPE: c_uint = 3;
const MEMORY_SYSTEM_RAM: c_uint = 2;

#[repr(C)]
struct GameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}
#[repr(C)]
#[derive(Default)]
struct AvInfo {
    geometry: [c_uint; 4],
    aspect: f32,
    fps: f64,
    sample_rate: f64,
}

// libretro callbacks carry no user pointer, so one core per process.
static PIXEL_FORMAT: AtomicU32 = AtomicU32::new(0);
type Sink = Box<dyn FnMut(Vec<u8>, u32, u32) + Send>;
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

unsafe extern "C" fn environment(cmd: c_uint, data: *mut c_void) -> bool {
    match cmd {
        SET_PIXEL_FORMAT => PIXEL_FORMAT.store(unsafe { *(data as *const u32) }, Relaxed),
        GET_CAN_DUPE => unsafe { *(data as *mut bool) = true },
        _ => return false,
    }
    true
}
unsafe extern "C" fn video(data: *const c_void, w: c_uint, h: c_uint, pitch: usize) {
    if data.is_null() || w == 0 || h == 0 {
        return; // a duped frame
    }
    let fmt = PIXEL_FORMAT.load(Relaxed);
    let bpp = if fmt == 1 { 4 } else { 2 };
    // The last row is only guaranteed to hold `w` pixels, not a full pitch.
    let src = unsafe { std::slice::from_raw_parts(data as *const u8, pitch * (h as usize - 1) + w as usize * bpp) };
    if let Some(sink) = SINK.lock().unwrap().as_mut() {
        sink(to_bgra(fmt, src, w as usize, h as usize, pitch), w, h);
    }
}
unsafe extern "C" fn audio_sample(_: i16, _: i16) {}
unsafe extern "C" fn audio_batch(_: *const i16, frames: usize) -> usize {
    frames
}
unsafe extern "C" fn input_poll() {}
unsafe extern "C" fn input_state(_: c_uint, _: c_uint, _: c_uint, _: c_uint) -> i16 {
    0
}

/// libretro pixel formats (0 = 0RGB1555, 1 = XRGB8888, 2 = RGB565) to tightly packed BGRA.
pub fn to_bgra(fmt: u32, src: &[u8], w: usize, h: usize, pitch: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 4);
    let up = |v: u16, bits: u32| ((v << (8 - bits)) | (v >> (2 * bits - 8))) as u8;
    for row in src.chunks(pitch).take(h) {
        if fmt == 1 {
            for p in row[..w * 4].chunks_exact(4) {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        } else {
            for p in row[..w * 2].chunks_exact(2) {
                let p = u16::from_le_bytes([p[0], p[1]]);
                let (r, g) = if fmt == 2 { (up(p >> 11, 5), up((p >> 5) & 63, 6)) } else { (up((p >> 10) & 31, 5), up((p >> 5) & 31, 5)) };
                out.extend_from_slice(&[up(p & 31, 5), g, r, 255]);
            }
        }
    }
    out
}

/// Load `core`, run `rom` forever on this thread. `frame` gets each BGRA picture;
/// `ram` gets the core's system RAM about ten times a second.
pub fn run(core: &str, rom: &str, frame: impl FnMut(Vec<u8>, u32, u32) + Send + 'static, mut ram: impl FnMut(&[u8])) -> Result<(), String> {
    let rom_bytes = std::fs::read(rom).map_err(|e| format!("{rom}: {e}"))?;
    let rom_path = CString::new(rom).map_err(|e| e.to_string())?;
    *SINK.lock().unwrap() = Some(Box::new(frame));
    unsafe {
        let lib = libloading::Library::new(core).map_err(|e| format!("{core}: {e}"))?;
        macro_rules! sym {
            ($name:literal, $ty:ty) => {
                *lib.get::<$ty>($name).map_err(|e| e.to_string())?
            };
        }
        sym!(b"retro_set_environment", unsafe extern "C" fn(unsafe extern "C" fn(c_uint, *mut c_void) -> bool))(environment);
        sym!(b"retro_set_video_refresh", unsafe extern "C" fn(unsafe extern "C" fn(*const c_void, c_uint, c_uint, usize)))(video);
        sym!(b"retro_set_audio_sample", unsafe extern "C" fn(unsafe extern "C" fn(i16, i16)))(audio_sample);
        sym!(b"retro_set_audio_sample_batch", unsafe extern "C" fn(unsafe extern "C" fn(*const i16, usize) -> usize))(audio_batch);
        sym!(b"retro_set_input_poll", unsafe extern "C" fn(unsafe extern "C" fn()))(input_poll);
        sym!(b"retro_set_input_state", unsafe extern "C" fn(unsafe extern "C" fn(c_uint, c_uint, c_uint, c_uint) -> i16))(input_state);
        sym!(b"retro_init", unsafe extern "C" fn())();
        let game = GameInfo { path: rom_path.as_ptr(), data: rom_bytes.as_ptr().cast(), size: rom_bytes.len(), meta: std::ptr::null() };
        if !sym!(b"retro_load_game", unsafe extern "C" fn(*const GameInfo) -> bool)(&game) {
            return Err(format!("the core rejected {rom}"));
        }
        let mut av = AvInfo::default();
        sym!(b"retro_get_system_av_info", unsafe extern "C" fn(*mut AvInfo))(&mut av);
        let step = Duration::from_secs_f64(1.0 / if av.fps > 1.0 { av.fps } else { 60.0 });
        let retro_run = sym!(b"retro_run", unsafe extern "C" fn());
        let mem_data = sym!(b"retro_get_memory_data", unsafe extern "C" fn(c_uint) -> *mut c_void);
        let mem_size = sym!(b"retro_get_memory_size", unsafe extern "C" fn(c_uint) -> usize);
        let mut next = Instant::now();
        for n in 0u64.. {
            retro_run();
            if n % 6 == 0 {
                let (p, len) = (mem_data(MEMORY_SYSTEM_RAM), mem_size(MEMORY_SYSTEM_RAM));
                if !p.is_null() && len > 0 {
                    ram(std::slice::from_raw_parts(p as *const u8, len));
                }
            }
            next += step;
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::to_bgra;

    #[test]
    fn every_pixel_format_becomes_bgra_and_pitch_padding_is_dropped() {
        // Pure red, one pixel, with two bytes of row padding after it.
        assert_eq!(to_bgra(1, &[0, 0, 255, 0, 9, 9], 1, 1, 6), [0, 0, 255, 255]);
        assert_eq!(to_bgra(2, &0xF800u16.to_le_bytes(), 1, 1, 2), [0, 0, 255, 255]);
        assert_eq!(to_bgra(0, &0x7C00u16.to_le_bytes(), 1, 1, 2), [0, 0, 255, 255]);
        // RGB565 white, two rows of one pixel, pitch 4: padding skipped on every row.
        assert_eq!(to_bgra(2, &[255, 255, 7, 7, 255, 255], 1, 2, 4), [255; 8]);
    }
}
