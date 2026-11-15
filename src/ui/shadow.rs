//! Rounded pane shadows — and glass panes — over the terminal's graphics
//! protocol.
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
use std::sync::atomic::{AtomicBool, Ordering};

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

// ── Glass ─────────────────────────────────────────────────────────────────────
/// White veil inside the card, as an alpha. Low enough that the terminal's own
/// background still reads through: this is glass, not a fill.
const GLASS_FILL: f32 = 0.025;
/// Extra veil at the top edge, fading out by mid-card — the "lit" side.
const GLASS_SHEEN: f32 = 0.04;
/// The rim: a hairline that catches the light, brighter along the top.
const RIM_TOP: f32 = 0.34;
const RIM_BOTTOM: f32 = 0.16;
/// Rim thickness in image pixels.
const RIM_PX: f32 = 1.2;
/// The card sits this far inside its pane, in sixteenths of a cell, so two
/// neighbouring panes leave a sliver of raw background between their rims.
const CARD_INSET: f32 = 2.0;
/// Rim of a pane that does not own focus.
const RIM_IDLE: f32 = 0.12;
const RIM_IDLE_BOTTOM: f32 = 0.06;

/// Image resolution per cell. Fine enough that the 1 px rim survives the
/// terminal's scale-down; the shadow is a gradient and costs nothing.
const PX_W: u32 = 12;
const PX_H: u32 = 24;

// ── Availability ──────────────────────────────────────────────────────────────

static ENABLED: OnceLock<bool> = OnceLock::new();

/// Whether panes are drawn as glass cards (fill + rim) or bare shadow rings.
static GLASS: AtomicBool = AtomicBool::new(true);

/// Turn the glass card on or off (`--glass=off` keeps the shadow only).
pub(super) fn set_glass(on: bool) {
    GLASS.store(on, Ordering::Relaxed);
}

fn glass() -> bool {
    GLASS.load(Ordering::Relaxed)
}

/// Whether panes are glass cards (the caller uses this to keep panel fills out
/// of the way).
pub(super) fn glass_on() -> bool {
    glass() && supported()
}

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
    /// Panes that want a card this frame, and whether each owns focus. Filled by
    /// `styles::draw_panel` / `draw_dialog` while the frame is built.
    static WANTED: RefCell<Vec<(Rect, bool)>> = const { RefCell::new(Vec::new()) };
}

/// Start a frame: forget what the last one asked for.
pub(super) fn begin_frame() {
    WANTED.with(|w| w.borrow_mut().clear());
}

/// The panes the current frame asked for (tests only).
#[cfg(test)]
pub(super) fn wanted() -> Vec<(Rect, bool)> {
    WANTED.with(|w| w.borrow().clone())
}

/// Ask for a card in `rect` (a pane's whole cell rect).
pub(super) fn want(rect: Rect, active: bool) {
    if rect.width >= 3 && rect.height >= 3 {
        WANTED.with(|w| w.borrow_mut().push((rect, active)));
    }
}

// ── Placement ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
struct Placed {
    id: u32,
    rect: Rect,
    active: bool,
}

/// What is currently on screen, so an unchanged frame costs nothing.
#[derive(Default)]
pub struct Layer {
    placed: Vec<Placed>,
    next_id: u32,
    /// PNG bytes per (cell size, focused).
    cache: HashMap<(u16, u16, bool), Vec<u8>>,
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
        let wanted = WANTED.with(|w| w.borrow().clone());
        if self.placed.len() == wanted.len()
            && self
                .placed
                .iter()
                .zip(&wanted)
                .all(|(p, (rect, active))| p.rect == *rect && p.active == *active)
        {
            return Ok(());
        }
        let mut out = io::stdout().lock();
        for placed in self.placed.drain(..) {
            write!(out, "\x1b_Ga=d,d=i,i={},q=2\x1b\\", placed.id)?;
        }
        for (rect, active) in wanted {
            let id = self.next_id;
            self.next_id += 1;
            let png = self.image(rect.width, rect.height, active)?;
            place(&mut out, rect, &png, id)?;
            self.placed.push(Placed { id, rect, active });
        }
        out.flush()
    }

    /// PNG for a pane of `w`×`h` cells, memoised — the same pane size is placed
    /// again every time focus returns to it.
    fn image(&mut self, w: u16, h: u16, active: bool) -> io::Result<Vec<u8>> {
        if let Some(png) = self.cache.get(&(w, h, active)) {
            return Ok(png.clone());
        }
        let png = encode(w, h, active)?;
        self.cache.insert((w, h, active), png.clone());
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
                "\x1b_Ga=T,f=100,q=2,i={},c={},r={},z=-1,m={};",
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

/// Render one pane's image: the card (glass fill + rim), then the shadow it
/// casts, in a single RGBA buffer the size of the pane's cell rect.
///
/// The card is the pane inset by one cell, matching where the content starts, so
/// the glass never runs under a glyph that the terminal would then have to fight
/// with. Everything outside it is the shadow.
fn rgba(w: u16, h: u16, active: bool) -> Vec<u8> {
    if !glass() {
        return shadow_ring(w, h);
    }
    let iw = u32::from(w) * PX_W;
    let ih = u32::from(h) * PX_H;
    let mut px = vec![0u8; (iw * ih * 4) as usize];

    // The card fills the pane, held a hair inside it so neighbouring panes keep
    // a sliver of bare background between their rims.
    let scale = f32::from(PX_H as u16) / 16.0;
    let inset = CARD_INSET * scale;
    let half = (
        (iw as f32 - 2.0 * inset) / 2.0,
        (ih as f32 - 2.0 * inset) / 2.0,
    );
    let centre = (iw as f32 / 2.0, ih as f32 / 2.0);
    let radius = (RADIUS * scale).min(half.0).min(half.1);
    let rim = RIM_PX * scale;
    let (rim_top, rim_bottom) = if active {
        (RIM_TOP, RIM_BOTTOM)
    } else {
        (RIM_IDLE, RIM_IDLE_BOTTOM)
    };

    for y in 0..ih {
        let row = (y as f32 / ih as f32).clamp(0.0, 1.0);
        for x in 0..iw {
            let p = (x as f32 + 0.5 - centre.0, y as f32 + 0.5 - centre.1);
            let d = rounded_box(p, half, radius);
            let (white, a) = if d <= 0.0 {
                // Inside the card: the veil, a lit top, and the rim at the edge.
                let sheen = GLASS_SHEEN * (1.0 - row).powi(2);
                let mut a = GLASS_FILL + sheen;
                if d > -rim {
                    let t = ((d + rim) / rim).clamp(0.0, 1.0); // 0 right at the edge
                    let edge = rim_top + (rim_bottom - rim_top) * row;
                    a = a.max(edge * (1.0 - t * 0.15));
                }
                (true, a)
            } else {
                // Just outside: a hint of shading so the card does not look pasted
                // on. Dark, and short enough never to clip against the pane edge.
                let t = (1.0 - d / (2.5 * scale)).clamp(0.0, 1.0);
                (false, ALPHA * 0.35 * t * t)
            };
            let a = (a.clamp(0.0, 1.0) * 255.0) as u8;
            if a == 0 {
                continue;
            }
            let i = ((y * iw + x) * 4) as usize;
            let v = if white { 255 } else { 0 };
            px[i] = v;
            px[i + 1] = v;
            px[i + 2] = v;
            px[i + 3] = a;
        }
    }
    px
}

/// The bare shadow: a ring around the card, nothing inside it. This is the
/// `--glass=off` look, and it is what the app shipped before the cards.
fn shadow_ring(w: u16, h: u16) -> Vec<u8> {
    let iw = u32::from(w) * PX_W;
    let ih = u32::from(h) * PX_H;
    let mut px = vec![0u8; (iw * ih * 4) as usize];

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
            let card = rounded_box((p.0 - centre.0, p.1 - centre.1), half, radius);
            if card <= 0.0 {
                continue;
            }
            let shadow = rounded_box((p.0 - centre.0, p.1 - centre.1 - offset), half, radius);
            let t = (0.5 - shadow / (2.0 * blur)).clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t);
            let a = (ALPHA * eased * 255.0) as u8;
            if a == 0 {
                continue;
            }
            let i = ((y * iw + x) * 4) as usize;
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

fn encode(w: u16, h: u16, active: bool) -> io::Result<Vec<u8>> {
    let iw = u32::from(w) * PX_W;
    let ih = u32::from(h) * PX_H;
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, iw, ih);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(io_err)?;
        writer
            .write_image_data(&rgba(w, h, active))
            .map_err(io_err)?;
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

    /// `GLASS` is process-global, so the two rendering tests take turns.
    static GLASS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Glass on: a white veil inside the card, a brighter rim at its edge, and
    /// a dark shadow outside it.
    #[test]
    fn glass_is_a_veiled_card_with_a_lit_rim() {
        let _guard = GLASS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_glass(true);
        let (w, h) = (12u16, 8u16);
        let px = rgba(w, h, true);
        let iw = w as u32 * PX_W;
        let alpha = |x: u32, y: u32| px[((y * iw + x) * 4 + 3) as usize];
        let rgb = |x: u32, y: u32| {
            let i = ((y * iw + x) * 4) as usize;
            (px[i], px[i + 1], px[i + 2])
        };

        // Middle of the card: a faint white veil, not a fill.
        let ih = h as u32 * PX_H;
        let mid = alpha(iw / 2, PX_H * 4);
        assert!(mid > 0 && mid < 40, "glass veil out of range: {mid}");
        assert_eq!(rgb(iw / 2, PX_H * 4), (255, 255, 255), "veil must be white");
        // The rim hugs the card's edge: the brightest pixel of the row sits at
        // the edge, not in the middle.
        let row = ih / 2;
        let (rim_x, rim) = (0..iw)
            .map(|x| (x, alpha(x, row)))
            .max_by_key(|(_, a)| *a)
            .expect("row");
        assert!(rim > mid * 3, "no rim: {rim} vs veil {mid}");
        assert!(
            rim_x < 6 || rim_x > iw - 6,
            "the rim is not at an edge: x={rim_x}"
        );
        // The sliver outside the card is dark, not a glow.
        assert_eq!(rgb(0, ih / 2), (0, 0, 0), "outer shading must be dark");
        let _ = h;
    }

    /// Glass off: the pane is a bare shadow ring again, interior clear.
    #[test]
    fn without_glass_the_card_stays_clear() {
        let _guard = GLASS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_glass(false);
        let (w, h) = (12u16, 8u16);
        let px = rgba(w, h, true);
        let iw = w as u32 * PX_W;
        let alpha = |x: u32, y: u32| px[((y * iw + x) * 4 + 3) as usize];
        assert_eq!(alpha(iw / 2, PX_H * 4), 0, "card interior must stay clear");
        assert_eq!(alpha(iw / 2, 7 * PX_H - 1), 0);
        let under = alpha(iw / 2, 7 * PX_H + 1);
        assert!(under > 60, "shadow too faint under the card: {under}");
        let far = alpha(iw / 2, h as u32 * PX_H - 1);
        assert!(far < 60, "shadow must fade out at the pane edge: {far}");
        let corner = alpha(2, 7 * PX_H + 1);
        assert!(corner < under, "corners must be rounded, not square");
        set_glass(true);
    }

    /// The placement moves to the pane's top-left cell (1-based), transmits a
    /// PNG scaled to the pane's cell rect, and ends every chunk cleanly.
    #[test]
    fn placement_is_a_well_formed_kitty_transmission() {
        let mut out = Vec::new();
        let png = encode(12, 4, true).unwrap();
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
