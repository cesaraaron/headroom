use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

// ── Theme registry ──────────────────────────────────────────────────────────
// All UI chrome colors live in a `Theme` so palettes are runtime-selectable.
// Pattern mirrors `OPAQUE_BACKGROUND` below: set from the UI thread, read
// while rendering (never on the audio thread). Index 0 is the legacy palette,
// value-for-value, so every existing snapshot passes unchanged.

/// Everything a palette owns. Pedal liveries are deliberately NOT here —
/// stompbox colors are fixed real-world liveries (`PEDAL_*` below).
pub(super) struct Theme {
    /// Structural chrome: panel, modal, and tile frames, plus the amp/cab
    /// grille before it is shaded. Deliberately separate from `accent`, so a
    /// palette can sink its frames into the background while focus stays hot.
    pub border: Color,
    /// Focus and selection: the focused knob's frame, dial, label and value, the
    /// reversed chips, the selected chain stage, and list highlights.
    pub accent: Color,
    pub amber: Color,
    pub dim: Color,
    pub chrome: Color,
    pub grid: Color,
    pub hot: Color,
    pub safe: Color,
    pub warn: Color,
    pub off: Color,
}

/// The palette of the project this one is forked from: warm amber-orange frames
/// and grille (`#C86400`), with a brighter orange for focus and selection
/// (`#FF8C00`) so the active control sits a step above the chrome around it.
/// This is the default palette.
const RUST: Theme = Theme {
    border: Color::Rgb(200, 100, 0),
    accent: Color::Rgb(255, 140, 0),
    amber: Color::Rgb(255, 200, 60),
    dim: Color::Rgb(80, 80, 80),
    chrome: Color::Rgb(180, 180, 190),
    grid: Color::Rgb(118, 118, 128),
    hot: Color::Rgb(220, 30, 30),
    safe: Color::Rgb(40, 180, 40),
    warn: Color::Rgb(220, 180, 0),
    off: Color::Rgb(50, 50, 50),
};

/// The cooler alternate: one teal for both frames and focus, value-for-value the
/// palette this project shipped before `border` was split out of `accent`.
const TEAL: Theme = Theme {
    border: Color::Rgb(0x5F, 0xB3, 0xB3),
    accent: Color::Rgb(0x5F, 0xB3, 0xB3),
    amber: Color::Rgb(255, 200, 60),
    dim: Color::Rgb(80, 80, 80),
    chrome: Color::Rgb(180, 180, 190),
    grid: Color::Rgb(118, 118, 128),
    hot: Color::Rgb(220, 30, 30),
    safe: Color::Rgb(40, 180, 40),
    warn: Color::Rgb(220, 180, 0),
    off: Color::Rgb(50, 50, 50),
};

/// Built-in palettes: (id, theme). The ids are stable and must not be reordered
/// — a saved `ui.conf` stores the index, so reordering would silently repaint
/// everyone's UI. The fresh-install default is [`DEFAULT_THEME`], not index 0.
static THEMES: [(&str, Theme); 2] = [("teal", TEAL), ("rust", RUST)];

/// Palette a fresh install starts on: `rust`. Chosen by index rather than by
/// moving it to 0 so the ids stay pinned to their palettes.
const DEFAULT_THEME: u8 = 1;

/// The active palette id; [`DEFAULT_THEME`] until `ui.conf` says otherwise.
static THEME_INDEX: AtomicU8 = AtomicU8::new(DEFAULT_THEME);

/// The active palette.
pub(super) fn theme() -> &'static Theme {
    &THEMES[(THEME_INDEX.load(Ordering::Relaxed) as usize) % THEMES.len()].1
}

/// Name of the active palette (for the switch toast + conf write-back).
pub(super) fn theme_name() -> &'static str {
    THEMES[(THEME_INDEX.load(Ordering::Relaxed) as usize) % THEMES.len()].0
}

/// Select a palette by index; out-of-range clamps (forward-compat), so the
/// stored value is always canonical — a conf of e.g. 47 resolves to a real
/// theme instead of drifting when theme #2 lands.
pub(super) fn set_theme(index: u8) {
    THEME_INDEX.store((index as usize % THEMES.len()) as u8, Ordering::Relaxed);
}

/// Current palette index (for the switch toast + conf write-back).
pub(super) fn theme_index() -> u8 {
    THEME_INDEX.load(Ordering::Relaxed)
}

/// Advance to the next palette, wrapping (`F3`). The store clamps, so the
/// written conf is always a canonical index.
pub(super) fn cycle_theme() {
    set_theme(theme_index().wrapping_add(1));
}

// ── Focus animation preference ────────────────────────────────────────────────
// Panel focus changes cross-fade (see `super::fade`). On by default; `F6` turns
// it off for a snappier UI or a terminal that cannot keep up, and `ui.conf`
// remembers the choice across runs. Set from the UI thread, read while rendering
// (never on the audio thread).
static ANIMATIONS: AtomicBool = AtomicBool::new(true);

pub(super) fn animations() -> bool {
    ANIMATIONS.load(Ordering::Relaxed)
}

pub(super) fn set_animations(on: bool) {
    ANIMATIONS.store(on, Ordering::Relaxed);
}

/// Flip the animation preference (`F6`); returns the new state.
pub(super) fn toggle_animations() -> bool {
    let on = !animations();
    set_animations(on);
    on
}

// ── Panel frame look ──────────────────────────────────────────────────────────
// A small ladder of cumulative panel-border looks, cycled live with `F5` so a
// change can be compared against earlier rungs without a rebuild. `rounded` is
// the default: soft corners read as a card rather than a box.
//
//   0  classic         plain/thick frames exactly as shipped
//   1  rounded         rounded corner glyphs on the panels and knobs (default)
//
// A heavy rung was tried and dropped: box-drawing arcs exist only in the light
// weight, so a heavy frame can never round its corners, and pairing `━` with
// `╭` left the corners visibly finer than the straights. Rounded wins; keep the
// frame one uniform weight. Floating active panes (a light edge / a lifted fill)
// were also tried and dropped — the app is happy with focus shown in the border
// colour alone.
//
// Runtime-only, like the mains-hum A/B: never written to `ui.conf`.
static FRAME_LEVEL: AtomicU8 = AtomicU8::new(DEFAULT_FRAME_LEVEL);

/// Fresh-install panel look: rounded corners.
const DEFAULT_FRAME_LEVEL: u8 = 1;

/// Highest rung on the frame ladder. Kept next to [`frame_set`] so adding a
/// look means bumping this and handling the new level in one place.
const FRAME_MAX: u8 = 1;

pub(super) fn frame_level() -> u8 {
    FRAME_LEVEL.load(Ordering::Relaxed).min(FRAME_MAX)
}

pub(super) fn set_frame_level(level: u8) {
    FRAME_LEVEL.store(level.min(FRAME_MAX), Ordering::Relaxed);
}

/// Step to the next rung, wrapping (`F5`). Returns the new level.
pub(super) fn cycle_frame() -> u8 {
    let next = next_frame_level(frame_level());
    set_frame_level(next);
    next
}

/// Pure rung stepping, so the ladder can be tested without mutating the process
/// global (which snapshot tests share).
fn next_frame_level(current: u8) -> u8 {
    (current.min(FRAME_MAX) + 1) % (FRAME_MAX + 1)
}

/// Human label for the current rung, for the switch toast and the help screen.
pub(super) fn frame_name() -> &'static str {
    frame_name_at(frame_level())
}

fn frame_name_at(level: u8) -> &'static str {
    match level.min(FRAME_MAX) {
        0 => "classic",
        _ => "rounded",
    }
}

/// Border glyphs for a structural panel. `classic` is whatever that panel
/// shipped with (plain, thick, ...), so level 0 is a byte-for-byte no-op.
pub(super) fn frame_set(classic: border::Set<'static>) -> border::Set<'static> {
    frame_set_at(frame_level(), classic)
}

fn frame_set_at(level: u8, classic: border::Set<'static>) -> border::Set<'static> {
    match level.min(FRAME_MAX) {
        0 => classic,
        _ => border::ROUNDED,
    }
}

/// The glyphs that cap a panel title on its top edge: (`├`, `┤`), matching the
/// light rounded frame so a title never looks heavier than the frame around it.
#[derive(Clone, Copy)]
pub(super) struct Rule {
    pub left: &'static str,
    pub right: &'static str,
}

pub(super) fn frame_rule() -> Rule {
    Rule {
        left: "├",
        right: "┤",
    }
}

// ── Panel frame styling ───────────────────────────────────────────────────────
// A terminal has no alpha channel, so an inactive border recedes with a dark
// shade plus the `DIM` attribute, and the focused region keeps the full chrome
// colour — that contrast is the whole focus affordance.

/// Scale an RGB color, leaving named/indexed colors alone.
pub(super) fn shade(c: Color, factor: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (f32::from(r) * factor) as u8,
            (f32::from(g) * factor) as u8,
            (f32::from(b) * factor) as u8,
        ),
        other => other,
    }
}

/// Blend two colors: `t` 0.0 is `a`, 1.0 is `b`. A terminal has no alpha
/// channel, so every fade in this UI is a color interpolation rather than an
/// opacity ramp. Named/indexed colors cannot be interpolated, so they snap at
/// the halfway point.
pub(super) fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let lerp =
                |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
            Color::Rgb(lerp(ar, br), lerp(ag, bg), lerp(ab, bb))
        }
        _ if t >= 0.5 => b,
        _ => a,
    }
}

/// Border style for a structural panel: the full [`Theme::border`] when the
/// region is active/focused, otherwise a heavily faded border with the
/// terminal's `DIM` attribute, so the focused region stands out.
///
/// `emphasis` 0.0 is the settled unfocused frame and 1.0 the settled focused
/// one, and the color ramps between them while a panel fades. The `BOLD`/`DIM`
/// modifiers stay pinned to the two settled ends (they cannot be interpolated,
/// and switching them mid-ramp would show as a step), so a fading frame is pure
/// color — exactly where the eye is.
pub(super) fn panel_border_style_at(emphasis: f32) -> Style {
    let e = emphasis.clamp(0.0, 1.0);
    if e >= 1.0 {
        Style::default()
            .fg(theme().border)
            .add_modifier(Modifier::BOLD)
    } else if e <= 0.0 {
        Style::default()
            .fg(shade(theme().border, 0.35))
            .add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(mix(shade(theme().border, 0.35), theme().border, e))
    }
}

/// Color for a region's on-border glyphs (titles), matching
/// [`panel_border_style_at`], ramped by the same emphasis.
pub(super) fn panel_edge_color_at(emphasis: f32) -> Color {
    mix(
        shade(theme().border, 0.35),
        theme().border,
        emphasis.clamp(0.0, 1.0),
    )
}

// ── Preferences file ──────────────────────────────────────────────────────────
// `~/.config/headroom/ui.conf` holds the choices that outlive a run: the palette
// and whether focus changes animate. Both live in one file, so a write from
// either switch rewrites the whole set rather than clobbering the other key
// (mirrors the `midi.conf` shape: `key = value`, temp file + rename).

/// Everything `ui.conf` stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct UiPrefs {
    /// Palette index in [`THEMES`].
    pub(super) theme: u8,
    /// Whether a focus change cross-fades.
    pub(super) animations: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME,
            animations: true,
        }
    }
}

/// `~/.config/headroom/ui.conf` (mirrors the `midi.conf` shape).
fn ui_conf_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("headroom").join("ui.conf"))
}

/// Load the saved preferences. A missing or corrupt file means the defaults — a
/// fresh install has no config dir yet, and a forward-dated theme id clamps in
/// `set_theme`.
pub(super) fn load_ui_conf() {
    let Some(path) = ui_conf_path() else {
        return;
    };
    apply_ui_conf(read_ui_conf_at(&path));
}

/// The live preference set, for writing.
pub(super) fn current_ui_prefs() -> UiPrefs {
    UiPrefs {
        theme: theme_index(),
        animations: animations(),
    }
}

pub(super) fn apply_ui_conf(prefs: UiPrefs) {
    set_theme(prefs.theme);
    set_animations(prefs.animations);
}

/// Parse a conf file. Unknown keys and unparsable values are skipped, so a file
/// written by a newer build still loads the keys this one knows, and a file from
/// before a key existed leaves that key at its default.
pub(super) fn read_ui_conf_at(path: &std::path::Path) -> UiPrefs {
    let mut prefs = UiPrefs::default();
    let Ok(text) = std::fs::read_to_string(path) else {
        return prefs;
    };
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        match key.trim() {
            "theme" => {
                if let Ok(i) = value.trim().parse::<u8>() {
                    prefs.theme = i;
                }
            }
            "animations" => {
                if let Some(on) = parse_switch(value.trim()) {
                    prefs.animations = on;
                }
            }
            _ => {}
        }
    }
    prefs
}

/// `on` / `off` is what this writes; the rest are accepted so a hand-edited conf
/// reads the way a person would expect to write it.
fn parse_switch(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" => Some(true),
        "0" | "off" | "false" | "no" => Some(false),
        _ => None,
    }
}

/// Persist the whole preference set atomically (temp file + rename, like
/// `midi.conf`). Every key is written every time, so flipping one switch can
/// never erase the other.
pub(super) fn write_ui_conf() {
    let Some(path) = ui_conf_path() else {
        return;
    };
    let _ = write_ui_conf_at(&path, current_ui_prefs());
}

pub(super) fn write_ui_conf_at(path: &std::path::Path, prefs: UiPrefs) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("conf.tmp");
    std::fs::write(
        &tmp,
        format!(
            "theme = {}\nanimations = {}\n",
            prefs.theme,
            if prefs.animations { "on" } else { "off" }
        ),
    )?;
    std::fs::rename(&tmp, path)
}

// ── Panel background ──────────────────────────────────────────────────────────
// Transparent by default so terminal transparency (e.g. Ghostty) shows through:
// panels use the terminal's own background. `--opaque` restores the legacy solid
// black panels. Set once from `ui::run` before the event loop; only read while
// rendering (never on the audio thread).
static OPAQUE_BACKGROUND: AtomicBool = AtomicBool::new(false);

pub(super) fn set_opaque(opaque: bool) {
    OPAQUE_BACKGROUND.store(opaque, Ordering::Relaxed);
}

/// Background style for panels and modal shells.
pub(super) fn panel_style() -> Style {
    if OPAQUE_BACKGROUND.load(Ordering::Relaxed) {
        Style::default().bg(Color::Black)
    } else {
        Style::default()
    }
}

// ── Pedal body colors (real-world stompbox liveries) ──────────────────────────
pub(super) const PEDAL_GREEN: Color = Color::Rgb(60, 170, 80); // TS-808 Tube Screamer
pub(super) const PEDAL_ORANGE: Color = Color::Rgb(235, 120, 25); // DS-1 Distortion
pub(super) const PEDAL_BLUE: Color = Color::Rgb(70, 130, 230); // Spring Reverb
pub(super) const PEDAL_PURPLE: Color = Color::Rgb(160, 100, 225); // Delay
pub(super) const PEDAL_SILVER: Color = Color::Rgb(165, 165, 180); // Noise Gate
pub(super) const PEDAL_TEAL: Color = Color::Rgb(40, 190, 185); // Parametric EQ
pub(super) const PEDAL_RED: Color = Color::Rgb(215, 60, 55); // Fuzz
pub(super) const PEDAL_GOLD: Color = Color::Rgb(220, 175, 50); // Compressor
pub(super) const PEDAL_LIME: Color = Color::Rgb(150, 200, 60); // Pre-amp EQ
pub(super) const PEDAL_INDIGO: Color = Color::Rgb(122, 124, 224); // Flanger
pub(super) const PEDAL_PINK: Color = Color::Rgb(235, 120, 190); // Chorus
pub(super) const PEDAL_CYAN: Color = Color::Rgb(60, 210, 235); // Whammy / Pitch shifter
pub(super) const PEDAL_YELLOW: Color = Color::Rgb(230, 205, 65); // Phaser (Phase 90)
pub(super) const PEDAL_STEEL: Color = Color::Rgb(120, 140, 165); // ML-2 Metal Core (gunmetal)
pub(super) const PEDAL_ORCHID: Color = Color::Rgb(195, 85, 205); // Wah (orchid purple-magenta)
pub(super) const PEDAL_SAND: Color = Color::Rgb(206, 188, 140); // Graphic EQ (warm sand/tan)
pub(super) const PEDAL_ROSE: Color = Color::Rgb(232, 128, 128); // Tremolo / Vibrato (soft rose)
pub(super) const PEDAL_VIBE: Color = Color::Rgb(225, 70, 150); // Uni-Vibe (deep magenta)
pub(super) const PEDAL_MINT: Color = Color::Rgb(120, 225, 170); // Clean boost (mint)
pub(super) const PEDAL_CHARCOAL: Color = Color::Rgb(105, 100, 95); // ProCo RAT (charcoal)
pub(super) const PEDAL_AMBER: Color = Color::Rgb(215, 150, 75); // Tube Driver (warm amber)
pub(super) const PEDAL_WALNUT: Color = Color::Rgb(150, 115, 80); // Rotary (walnut cabinet)

#[cfg(test)]
mod tests {
    use super::*;

    /// The background switch is the only thing `--opaque` changes: transparent
    /// (terminal default) unless explicitly requested. Resets the flag so no
    /// state leaks into other tests.
    #[test]
    fn panel_background_follows_opaque_flag() {
        set_opaque(false);
        assert_eq!(panel_style(), Style::default());
        set_opaque(true);
        assert_eq!(panel_style(), Style::default().bg(Color::Black));
        set_opaque(false);
    }

    /// `rust` is the fresh-install default: the original project's palette, with
    /// frames and grille in the darker orange and focus one step brighter. The
    /// two must differ, or the split into `border`/`accent` would be pointless.
    #[test]
    fn rust_is_the_default_and_matches_the_original_triples() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            THEMES[DEFAULT_THEME as usize].0, "rust",
            "the startup palette must be rust",
        );
        set_theme(DEFAULT_THEME);
        let t = theme();
        assert_eq!(theme_name(), "rust");
        assert_eq!(t.border, Color::Rgb(200, 100, 0));
        assert_eq!(t.accent, Color::Rgb(255, 140, 0));
        assert_ne!(t.border, t.accent);
        assert_eq!(t.amber, Color::Rgb(255, 200, 60));
        assert_eq!(t.dim, Color::Rgb(80, 80, 80));
        assert_eq!(t.chrome, Color::Rgb(180, 180, 190));
        assert_eq!(t.grid, Color::Rgb(118, 118, 128));
        assert_eq!(t.hot, Color::Rgb(220, 30, 30));
        assert_eq!(t.safe, Color::Rgb(40, 180, 40));
        assert_eq!(t.warn, Color::Rgb(220, 180, 0));
        assert_eq!(t.off, Color::Rgb(50, 50, 50));
    }

    /// `teal` keeps frames and focus equal, so its look is exactly what the app
    /// rendered before the slot split; and every non-chrome color is shared with
    /// `rust`, so a theme switch never moves the text hierarchy or the meters.
    #[test]
    fn themes_share_everything_but_chrome() {
        let teal = &THEMES[0].1;
        let rust = &THEMES[1].1;
        assert_eq!(THEMES[0].0, "teal");
        assert_eq!(THEMES[1].0, "rust");
        assert_eq!(teal.border, Color::Rgb(0x5F, 0xB3, 0xB3));
        assert_eq!(teal.accent, Color::Rgb(0x5F, 0xB3, 0xB3));
        assert_eq!(teal.border, teal.accent, "teal's frames must not shift");
        assert_eq!(rust.amber, teal.amber);
        assert_eq!(rust.dim, teal.dim);
        assert_eq!(rust.chrome, teal.chrome);
        assert_eq!(rust.grid, teal.grid);
        assert_eq!(rust.hot, teal.hot);
        assert_eq!(rust.safe, teal.safe);
        assert_eq!(rust.warn, teal.warn);
        assert_eq!(rust.off, teal.off);
    }

    /// The active palette is a process-global atomic, so the tests that change
    /// it take turns instead of interleaving.
    static THEME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// An out-of-range theme id clamps into the registry (forward-compat for a
    /// conf naming an id this build doesn't know). Restores the default so no
    /// state leaks into other tests.
    #[test]
    fn theme_index_clamps() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_theme(255);
        assert_eq!(theme_name(), "rust", "255 clamps to a real palette");
        assert_eq!(theme().accent, Color::Rgb(255, 140, 0));
        set_theme(DEFAULT_THEME);
        assert_eq!(theme_name(), "rust");
    }

    /// A missing `ui.conf` is the fresh-install case: the default palette and
    /// animations on, no error, no state change.
    #[test]
    fn ui_conf_missing_means_defaults() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_theme(DEFAULT_THEME);
        set_animations(true);
        let dir = std::env::temp_dir().join("headroom-no-such-dir-xyz");
        apply_ui_conf(read_ui_conf_at(&dir.join("ui.conf")));
        assert_eq!(theme_name(), "rust");
        assert!(animations(), "a fresh install animates");
    }

    /// Both preferences round-trip, and the defaults are what a fresh install
    /// writes. Uses a temp path so the real config dir is never touched.
    #[test]
    fn ui_conf_round_trips() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join("headroom-theme-test-ui.conf");
        let _ = std::fs::remove_file(&path);

        // A conf saved before `rust` was the default still means `teal`.
        write_ui_conf_at(
            &path,
            UiPrefs {
                theme: 0,
                animations: false,
            },
        )
        .expect("write conf");
        set_theme(DEFAULT_THEME);
        set_animations(true);
        apply_ui_conf(read_ui_conf_at(&path));
        assert_eq!(theme_name(), "teal");
        assert!(!animations(), "the switch must survive a restart");

        write_ui_conf_at(&path, UiPrefs::default()).expect("write conf");
        set_theme(0);
        set_animations(false);
        apply_ui_conf(read_ui_conf_at(&path));
        assert_eq!(theme_name(), "rust");
        assert!(animations());

        set_theme(DEFAULT_THEME);
        set_animations(true);
        let _ = std::fs::remove_file(&path);
    }

    /// A conf written before the animation switch existed (theme only) keeps the
    /// animation default: an upgrade must not silently turn the fade off.
    #[test]
    fn an_older_conf_leaves_animations_on() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join("headroom-old-ui-conf.conf");
        std::fs::write(&path, "theme = 0\n").expect("write legacy conf");

        set_animations(false);
        apply_ui_conf(read_ui_conf_at(&path));
        assert_eq!(theme_name(), "teal");
        assert!(animations(), "a missing key must fall back to the default");

        set_theme(DEFAULT_THEME);
        set_animations(true);
        let _ = std::fs::remove_file(&path);
    }

    /// Flipping one switch rewrites the whole file, so it can never erase the
    /// other key — the failure mode a two-writer conf invites.
    #[test]
    fn writing_one_preference_preserves_the_other() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join("headroom-ui-conf-preserve.conf");

        write_ui_conf_at(
            &path,
            UiPrefs {
                theme: 0,
                animations: false,
            },
        )
        .expect("write conf");
        set_theme(0);
        set_animations(false);

        // F6 equivalent: flip the switch, then persist the live set.
        toggle_animations();
        write_ui_conf_at(&path, current_ui_prefs()).expect("write conf");

        set_theme(DEFAULT_THEME);
        set_animations(false);
        let loaded = read_ui_conf_at(&path);
        assert_eq!(loaded.theme, 0, "the theme must survive an F6 write");
        assert!(loaded.animations, "the new switch value must be written");

        set_theme(DEFAULT_THEME);
        set_animations(true);
        let _ = std::fs::remove_file(&path);
    }

    /// The file format is plain `key = on|off`, and a hand edit in any of the
    /// usual spellings reads the way the writer meant; junk lines are ignored.
    #[test]
    fn the_conf_parses_common_spellings() {
        let dir = std::env::temp_dir().join("headroom-ui-conf-spellings");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("ui.conf");

        for (raw, expected) in [
            ("on", true),
            ("off", false),
            ("true", true),
            ("0", false),
            ("YES", true),
            ("garbage", true), // unparsable: falls back to the default
        ] {
            std::fs::write(&path, format!("theme = 1\nanimations = {raw}\n")).expect("write");
            assert_eq!(read_ui_conf_at(&path).animations, expected, "{raw}");
        }

        std::fs::write(&path, "not a key value line\ntheme=1\n").expect("write");
        let prefs = read_ui_conf_at(&path);
        assert_eq!(prefs.theme, 1, "junk lines are skipped, not fatal");
        assert!(prefs.animations);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `mix` is the fade's only primitive: pinned at both ends and clamped
    /// outside them, so a settled frame is exactly the color it always was.
    #[test]
    fn mix_is_pinned_and_clamped() {
        let a = Color::Rgb(10, 20, 30);
        let b = Color::Rgb(110, 220, 130);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 0.5), Color::Rgb(60, 120, 80));
        assert_eq!(mix(a, b, -1.0), a, "out of range clamps, never wraps");
        assert_eq!(mix(a, b, 9.0), b);
    }

    /// A panel's border at the two ends of the ramp is exactly the old boolean
    /// rendering, and mid-fade carries no `BOLD`/`DIM` step — so the animation
    /// never moved a settled frame and never pops mid-ramp.
    #[test]
    fn the_border_ramp_ends_are_the_settled_styles() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_theme(DEFAULT_THEME);

        let focused = panel_border_style_at(1.0);
        assert_eq!(focused.fg, Some(theme().border));
        assert!(focused.add_modifier.contains(Modifier::BOLD));

        let receded = panel_border_style_at(0.0);
        assert_eq!(receded.fg, Some(shade(theme().border, 0.35)));
        assert!(receded.add_modifier.contains(Modifier::DIM));

        let mid = panel_border_style_at(0.5);
        assert_eq!(mid.add_modifier, Modifier::empty());
        assert_ne!(mid.fg, focused.fg);
        assert_ne!(mid.fg, receded.fg);

        assert_eq!(panel_edge_color_at(1.0), theme().border);
        assert_eq!(panel_edge_color_at(0.0), shade(theme().border, 0.35));
    }

    /// The frame ladder is cumulative and clamps: level 0 is the shipped look
    /// exactly, the default rung is rounded, and stepping wraps. Tested through
    /// the pure helpers so the process-global level is never mutated while
    /// snapshot tests are rendering.
    #[test]
    fn frame_ladder_steps_clamps_and_wraps() {
        assert_eq!(DEFAULT_FRAME_LEVEL, 1, "rounded is the fresh-install look");
        assert_eq!(next_frame_level(0), 1);
        assert_eq!(next_frame_level(1), 0, "the ladder wraps");
        assert_eq!(next_frame_level(255), 0, "out-of-range clamps first");

        assert_eq!(frame_set_at(0, border::PLAIN), border::PLAIN);
        assert_eq!(frame_set_at(0, border::THICK), border::THICK);
        assert_eq!(frame_set_at(1, border::PLAIN), border::ROUNDED);
        assert_eq!(frame_set_at(1, border::THICK), border::ROUNDED);
        assert_eq!(
            frame_set_at(1, border::PLAIN).top_left,
            "╭",
            "the frame must stay rounded"
        );
        assert_eq!(
            frame_set_at(1, border::PLAIN).horizontal_top,
            "─",
            "the frame must stay one light weight"
        );

        assert_eq!(frame_name_at(0), "classic");
        assert_eq!(frame_name_at(1), "rounded");
        assert_eq!(frame_name_at(255), "rounded");

        let rule = frame_rule();
        assert_eq!((rule.left, rule.right), ("├", "┤"));
    }

    /// Cycling visits every palette and wraps, writing the canonical index each
    /// time — not a drifting counter.
    #[test]
    fn cycle_theme_visits_every_palette() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_theme(DEFAULT_THEME);
        let path = std::env::temp_dir().join("headroom-theme-cycle-test-ui.conf");
        let _ = std::fs::remove_file(&path);

        assert_eq!(theme_name(), "rust");
        cycle_theme();
        assert_eq!(theme_name(), "teal");
        write_ui_conf_at(&path, current_ui_prefs()).expect("write conf");
        let body = std::fs::read_to_string(&path).expect("read conf");
        assert!(
            body.starts_with("theme = 0\n"),
            "the theme must be written first: {body:?}"
        );
        assert!(
            body.contains("animations = on\n"),
            "the write must carry every key: {body:?}"
        );

        cycle_theme();
        assert_eq!(theme_name(), "rust", "cycling must wrap");

        let _ = std::fs::remove_file(&path);
        set_theme(DEFAULT_THEME);
    }
}
