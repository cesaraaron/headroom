use ratatui::style::{Color, Style};
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

/// `~/.config/headroom/ui.conf` (mirrors the `midi.conf` shape).
fn ui_conf_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("headroom").join("ui.conf"))
}

/// Load a saved theme id (`theme = <n>`); missing/corrupt files mean the
/// default — a fresh install has no config dir yet, and a forward-dated id
/// clamps in `set_theme`.
pub(super) fn load_theme_conf() {
    let Some(path) = ui_conf_path() else {
        return;
    };
    load_theme_conf_at(&path);
}

pub(super) fn load_theme_conf_at(path: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("theme")
            && let Some(v) = v.trim().strip_prefix('=')
            && let Ok(i) = v.trim().parse::<u8>()
        {
            set_theme(i);
            return;
        }
    }
}

/// Persist the theme id atomically (temp file + rename, like `midi.conf`).
pub(super) fn write_theme_conf() {
    let Some(path) = ui_conf_path() else {
        return;
    };
    let _ = write_theme_conf_at(&path, theme_index());
}

pub(super) fn write_theme_conf_at(path: &std::path::Path, index: u8) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("conf.tmp");
    std::fs::write(&tmp, format!("theme = {index}\n"))?;
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

    /// A missing `ui.conf` is the fresh-install case: the default palette, no
    /// error, no state change.
    #[test]
    fn theme_conf_missing_means_default() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_theme(DEFAULT_THEME);
        let dir = std::env::temp_dir().join("headroom-no-such-dir-xyz");
        load_theme_conf_at(&dir.join("ui.conf"));
        assert_eq!(theme_name(), "rust");
    }

    /// A conf round-trips, and the default is what a fresh install writes.
    /// Uses a temp path so the real config dir is never touched.
    #[test]
    fn theme_conf_round_trips() {
        let _guard = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join("headroom-theme-test-ui.conf");
        let _ = std::fs::remove_file(&path);

        // A conf saved before `rust` was the default still means `teal`.
        write_theme_conf_at(&path, 0).expect("write conf");
        set_theme(DEFAULT_THEME);
        load_theme_conf_at(&path);
        assert_eq!(theme_name(), "teal");

        write_theme_conf_at(&path, DEFAULT_THEME).expect("write conf");
        set_theme(0);
        load_theme_conf_at(&path);
        assert_eq!(theme_name(), "rust");

        set_theme(DEFAULT_THEME);
        let _ = std::fs::remove_file(&path);
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
        write_theme_conf_at(&path, theme_index()).expect("write conf");
        let body = std::fs::read_to_string(&path).expect("read conf");
        assert_eq!(body, "theme = 0\n");

        cycle_theme();
        assert_eq!(theme_name(), "rust", "cycling must wrap");

        let _ = std::fs::remove_file(&path);
        set_theme(DEFAULT_THEME);
    }
}
