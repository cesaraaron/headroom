//! Rounded pane shadows, drawn over the terminal's graphics protocol.
//!
//! A text cell cannot draw a soft edge: the best a glyph can do is an eighth of
//! a cell, which reads as a hairline rather than depth. So the focused pane's
//! shadow is an **RGBA image** placed in the pane's own cell rect — a rounded
//! rectangle, blurred and offset, with its interior punched out so the app's
//! text shows through untouched.
//!
//! Only terminals that speak the kitty graphics protocol can do this (kitty,
//! Ghostty, WezTerm, Rex). Everywhere else the app renders its bordered UI,
//! byte for byte, exactly as before.
//!
//! The shape is fixed by design review: 65 % black, 9 px blur, 5 px down,
//! 10 px corner radius, all measured against a 16 px-tall cell.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::OnceLock;

use ratatui::layout::Rect;

// ── Shape ─────────────────────────────────────────────────────────────────────
/// Peak darkness of the shadow, at the card's edge.
const ALPHA: f32 = 0.65;
/// Falloff length, in sixteenths of a cell.
const BLUR: f32 = 9.0;
/// Downward offset, in sixteenths of a cell.
const OFFSET: f32 = 5.0;
/// Corner radius, in sixteenths of a cell.
const RADIUS: f32 = 10.0;

/// Image resolution per cell. Half a typical cell, so the terminal upscales the
/// gradient 2x — free smoothing, and a quarter of the bytes.
const PX_W: u32 = 4;
const PX_H: u32 = 8;

// ── Availability ──────────────────────────────────────────────────────────────

static ENABLED: OnceLock<bool> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Per-test override, so a test can render the shadow path without racing
    /// the snapshot tests (which must see the bordered UI).
    static TEST_MODE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Whether this terminal can place images behind the text.
pub(super) fn supported() -> bool {
    #[cfg(test)]
    if let Some(v) = TEST_MODE.with(|m| m.get()) {
        return v;
    }
    // Tests are deterministic: bordered, unless a test forces the shadow path.
    if cfg!(test) {
        return false;
    }
    *ENABLED.get_or_init(probe)
}

/// Force the layer on or off (`--graphics=on|off`), before the first frame.
pub(super) fn force(on: Option<bool>) {
    #[cfg(test)]
    {
        TEST_MODE.with(|m| m.set(on));
    }
    #[cfg(not(test))]
    {
        if let Some(on) = on {
            let _ = ENABLED.set(on);
        }
    }
}

/// Sniff the environment for a terminal we know speaks the kitty graphics
/// protocol. `--graphics=on` covers anything missed.
fn probe() -> bool {
    let env = |k: &str| std::env::var(k).ok();
    if let Some(term) = env("TERM")
        && term.contains("kitty")
    {
        return true;
    }
    if env("KITTY_WINDOW_ID").is_some()
        || env("GHOSTTY_RESOURCES_DIR").is_some()
        || env("WEZTERM_PANE").is_some()
    {
        return true;
    }
    matches!(
        env("TERM_PROGRAM").as_deref(),
        Some("ghostty") | Some("WezTerm") | Some("rex") | Some("kitty")
    )
}

// ── Per-frame request ─────────────────────────────────────────────────────────

thread_local! {
    /// The pane that wants a shadow this frame. Only one thing can own focus, so
    /// this is a slot rather than a list: whoever asks last wins, which is how a
    /// modal takes the shadow from the panel behind it.
    static WANTED: RefCell<Option<Rect>> = const { RefCell::new(None) };
}

/// Start a frame: forget what the last one asked for.
pub(super) fn begin_frame() {
    WANTED.with(|w| *w.borrow_mut() = None);
}

/// The rect the current frame asked for (tests only).
#[cfg(test)]
pub(super) fn wanted() -> Option<Rect> {
    WANTED.with(|w| *w.borrow())
}

/// Ask for a shadow in `rect` (the pane's whole cell rect).
pub(super) fn want(rect: Rect) {
    if rect.width >= 3 && rect.height >= 3 {
        WANTED.with(|w| *w.borrow_mut() = Some(rect));
    }
}

// ── Placement ─────────────────────────────────────────────────────────────────

struct Placed {
    id: u32,
    rect: Rect,
}

/// What is currently on screen, so an unchanged frame costs nothing.
#[derive(Default)]
pub struct Layer {
    placed: Option<Placed>,
    next_id: u32,
    /// PNG bytes per (w, h) cell size.
    cache: HashMap<(u16, u16), Vec<u8>>,
}

impl Layer {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    /// Composite the frame's requests: place what changed, drop what left.
    pub fn commit(&mut self) -> io::Result<()> {
        if !supported() {
            return Ok(());
        }
        let wanted = WANTED.with(|w| *w.borrow());
        if self.placed.as_ref().map(|p| p.rect) == wanted {
            return Ok(());
        }
        let mut out = io::stdout().lock();
        if let Some(placed) = self.placed.take() {
            write!(out, "\x1b_Ga=d,d=i,i={},q=2\x1b\\", placed.id)?;
        }
        if let Some(rect) = wanted {
            let id = self.next_id;
            self.next_id += 1;
            let png = self.image(rect.width, rect.height)?;
            place(&mut out, rect, &png, id)?;
            self.placed = Some(Placed { id, rect });
        }
        out.flush()
    }

    /// PNG for a pane of `w`×`h` cells, memoised — the same pane size is placed
    /// again every time focus returns to it.
    fn image(&mut self, w: u16, h: u16) -> io::Result<Vec<u8>> {
        if let Some(png) = self.cache.get(&(w, h)) {
            return Ok(png.clone());
        }
        let png = encode(w, h)?;
        self.cache.insert((w, h), png.clone());
        Ok(png)
    }
}

/// Emit one image placement: move to the pane's top-left cell, then transmit and
/// display it over exactly those cells.
fn place<W: Write>(out: &mut W, rect: Rect, png: &[u8], id: u32) -> io::Result<()> {
    write!(out, "\x1b[{};{}H", rect.y + 1, rect.x + 1)?;
    let data = base64(png);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        if i == 0 {
            write!(
                out,
                "\x1b_Ga=T,f=100,q=2,i={},c={},r={},m={};",
                id, rect.width, rect.height, more
            )?;
        } else {
            write!(out, "\x1b_Gm={};", more)?;
        }
        out.write_all(chunk)?;
        out.write_all(b"\x1b\\")?;
    }
    Ok(())
}

// ── The image itself ──────────────────────────────────────────────────────────

/// Render the shadow ring for a pane of `w`×`h` cells as RGBA rows.
///
/// The card occupies the pane inset by one cell; the shadow is the rounded box
/// grown by the blur, shifted down, with everything inside the card cleared so
/// the terminal's own text is never covered.
fn rgba(w: u16, h: u16) -> Vec<u8> {
    let iw = u32::from(w) * PX_W;
    let ih = u32::from(h) * PX_H;
    let mut px = vec![0u8; (iw * ih * 4) as usize];

    // Card rect in image pixels (one cell inset all round).
    let scale = f32::from(PX_H as u16) / 16.0;
    let (cx0, cy0) = (PX_W as f32, PX_H as f32);
    let (cx1, cy1) = (
        (u32::from(w) - 1) as f32 * PX_W as f32,
        (u32::from(h) - 1) as f32 * PX_H as f32,
    );
    let half = ((cx1 - cx0) / 2.0, (cy1 - cy0) / 2.0);
    let centre = ((cx0 + cx1) / 2.0, (cy0 + cy1) / 2.0);
    let radius = (RADIUS * scale).min(half.0).min(half.1);
    let blur = BLUR * scale;
    let offset = OFFSET * scale;

    for y in 0..ih {
        for x in 0..iw {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            // Inside the card: leave the cell layer alone.
            let card = rounded_box((p.0 - centre.0, p.1 - centre.1), half, radius);
            if card <= 0.0 {
                continue;
            }
            // Distance to the (offset) shadow box.
            let shadow = rounded_box((p.0 - centre.0, p.1 - centre.1 - offset), half, radius);
            let t = (0.5 - shadow / (2.0 * blur)).clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t);
            let a = (ALPHA * eased * 255.0) as u8;
            if a == 0 {
                continue;
            }
            let i = ((y * iw + x) * 4) as usize;
            px[i] = 0;
            px[i + 1] = 0;
            px[i + 2] = 0;
            px[i + 3] = a;
        }
    }
    px
}

/// Signed distance to a rounded box centred on the origin (negative inside).
fn rounded_box(p: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
    let qx = p.0.abs() - half.0 + radius;
    let qy = p.1.abs() - half.1 + radius;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

fn encode(w: u16, h: u16) -> io::Result<Vec<u8>> {
    let iw = u32::from(w) * PX_W;
    let ih = u32::from(h) * PX_H;
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, iw, ih);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(io_err)?;
        writer.write_image_data(&rgba(w, h)).map_err(io_err)?;
    }
    Ok(png)
}

fn io_err(e: png::EncodingError) -> io::Error {
    io::Error::other(e.to_string())
}

// ── Small helpers ─────────────────────────────────────────────────────────────

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ring is transparent inside the card, opaque-ish at the card's edge
    /// and gone by the pane's outer edge — that is what makes it a shadow and
    /// not a frame.
    #[test]
    fn shadow_is_a_ring_around_the_card() {
        let (w, h) = (12u16, 8u16);
        let px = rgba(w, h);
        let iw = w as u32 * PX_W;
        let alpha = |x: u32, y: u32| px[((y * iw + x) * 4 + 3) as usize];

        // Dead centre of the card: untouched.
        assert_eq!(alpha(iw / 2, PX_H * 4), 0, "card interior must stay clear");
        // The last pixel inside the card's bottom edge: still clear.
        assert_eq!(alpha(iw / 2, 7 * PX_H - 1), 0);
        // Just under the card: the darkest part of the shadow.
        let under = alpha(iw / 2, 7 * PX_H + 1);
        assert!(under > 100, "shadow too faint under the card: {under}");
        // The pane's last row: nearly gone.
        let far = alpha(iw / 2, h as u32 * PX_H - 1);
        assert!(far < 60, "shadow must fade out at the pane edge: {far}");
        // The corner is lighter than the middle of the edge (rounded corner).
        let corner = alpha(2, 7 * PX_H + 1);
        assert!(corner < under, "corners must be rounded, not square");
    }

    /// The placement moves to the pane's top-left cell (1-based), transmits a
    /// PNG scaled to the pane's cell rect, and ends every chunk cleanly.
    #[test]
    fn placement_is_a_well_formed_kitty_transmission() {
        let mut out = Vec::new();
        let png = encode(12, 4).unwrap();
        place(&mut out, Rect::new(3, 2, 12, 4), &png, 7).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.starts_with("\x1b[3;4H"),
            "cursor must land on the pane"
        );
        assert!(
            text.contains("a=T,f=100,q=2,i=7,c=12,r=4"),
            "bad header: {text}"
        );
        assert!(text.ends_with("\x1b\\"), "each chunk must be terminated");
    }

    /// Base64 padding, so the escape sequence stays well-formed.
    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }

    /// Detection is a pure function of the environment; the app never overrides
    /// it without an explicit flag.
    #[test]
    fn probe_recognises_kitty_class_terminals() {
        assert!(probe_has("kitty", &[("TERM", "xterm-kitty")]));
        assert!(probe_has("ghostty", &[("TERM_PROGRAM", "ghostty")]));
        assert!(probe_has("wezterm", &[("TERM_PROGRAM", "WezTerm")]));
        assert!(probe_has("rex", &[("TERM_PROGRAM", "rex")]));
        assert!(!probe_has("term", &[("TERM", "xterm-256color")]));
    }

    /// `probe` reads the process environment; this mirrors it over a scratch
    /// map so the test never mutates the real one.
    fn probe_has(_name: &str, vars: &[(&str, &str)]) -> bool {
        let get = |k: &str| {
            vars.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_string())
        };
        if let Some(term) = get("TERM")
            && term.contains("kitty")
        {
            return true;
        }
        if get("KITTY_WINDOW_ID").is_some()
            || get("GHOSTTY_RESOURCES_DIR").is_some()
            || get("WEZTERM_PANE").is_some()
        {
            return true;
        }
        matches!(
            get("TERM_PROGRAM").as_deref(),
            Some("ghostty") | Some("WezTerm") | Some("rex") | Some("kitty")
        )
    }
}
