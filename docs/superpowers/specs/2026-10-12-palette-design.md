# Palette / theming refactor (3.3 remainder) — design

**Status:** written for review; implements roadmap Item 3.3 remainder
("palette refactor", `roadmap-next.md` Track 3.3). The narrow-width half
shipped; this is the runtime-selectable palette.
**Scope:** `src/ui/**` color plumbing only. No layout, no DSP, no presets.

## Starting state

- All colors are `pub(super) const Color` in `src/ui/styles.rs`: 9 chrome
  (`AMBER`, `ACCENT`, `DIM`, `CHROME`, `GRID`, `HOT`, `SAFE`, `WARN`, `OFF`)
  + 19 pedal liveries (`PEDAL_*`, real-world stompbox colors with comments).
- Consumed via `use super::styles::{...}` imports in ~15 UI files, used
  directly (`ACCENT`) and through two `shade(c, factor)` helpers
  (`draw.rs:2058`, `practice.rs:3168`).
- `config.rs::PEDALS` is a `const` array embedding 19 `PEDAL_*` colors —
  cannot stay `const` under theming.
- Precedent for runtime UI switches: `OPAQUE_BACKGROUND` (`AtomicBool` +
  `set_opaque` + `panel_style()` reader, set once from `ui::run`).
- UI snapshots encode exact pixels; the default theme must reproduce them.

## Intent

Two or three selectable palettes (legibility + taste: e.g. a light theme
for bright rooms, a high-contrast theme for accessibility), switchable
without restart, surviving restart. The default theme is pixel-identical
to today's colors so every existing snapshot passes unchanged.

Success = the player picks a theme, the whole UI (panels, meters, modals,
timeline) follows, and the choice persists across restarts.

## Goals / non-goals

**Goals**
- `Theme` struct owning every current color; 2–3 built-in themes; runtime
  switch with persistence.
- Mechanical call-site migration (`ACCENT` → themed read); `shade()`
  unchanged (takes `Color`).
- `PEDALS` becomes theme-aware (or keeps fixed liveries — human call).
- One snapshot per theme proving full-UI coverage; existing snapshots
  untouched (default = current pixels).

**Non-goals**
- No layout changes, no new panels, no keybinding changes beyond the theme
  switch itself.
- No user-authored custom themes (TOML theme files are a later project if
  ever; built-ins only).
- No DSP/preset/config-format changes beyond one persisted theme id.

**Anti-goals**
- A theme that breaks meter legibility (HOT/WARN/SAFE semantics must read
  in every theme — contrast-checked by snapshot inspection, not just code).
- Snapshot churn on the default theme: any default-snapshot diff is a bug.

## Constraints

- Gates: `cargo fmt --check`, `clippy --all-targets --all-features
  -D warnings`, `cargo test --release` + debug. No fidelity involvement
  (UI-only; harness untouched).
- Work directly on `main`; never push unprompted.
- Render-thread only (like `OPAQUE_BACKGROUND`); no audio-thread contact.

## Design

### Theme registry (`styles.rs`)

```rust
pub(super) struct Theme {
    pub amber: Color, pub accent: Color, pub dim: Color,
    pub chrome: Color, pub grid: Color, pub hot: Color,
    pub safe: Color, pub warn: Color, pub off: Color,
}
// NOTE: no pedal field — liveries stay fixed `PEDAL_*` consts (Decision 2).
```

- `THEMES: &[(&str, Theme)]` static (name + values); `AtomicU8` index;
  `theme() -> &'static Theme` reader; `set_theme(i)` writer, following the
  `set_opaque` precedent. Default index 0 = current colors, value-for-value
  (copy the exact `Rgb` triples, comments intact).
- Keep the old `const` names working during migration? No — flag-day
  rename in one commit (`ACCENT` → `theme().accent`), compiler-checked
  exhaustiveness (any missed site is a build error, not a runtime surprise).

### Call sites

Mechanical: each `use super::styles::{ACCENT, ...}` becomes `{theme, ...}`
(or qualified `styles::theme()`), each `ACCENT` becomes
`theme().accent`. `shade(ACCENT, x)` becomes `shade(theme().accent, x)`.
`config.rs::PEDALS` `const` → built by a `fn pedals() -> ...` reading
`theme().pedals[i]` (or the fixed `PEDAL_*` consts if liveries stay fixed).

### Switching + persistence (human call — options)

(a) Cycling key (my default if keys allow — most letters are taken, so
likely a modal or `T`-with-modifier; exact key is plan detail after audit).
(b) Config-file only (`ui.conf` theme id, hand-edited).
(c) Both: key cycles, writes back to `ui.conf` (mirrors `midi.conf`
write-back precedent). My recommendation.

### Themes shipped (human call)

Default (current, pixel-identical) + my proposal: a light theme + a
high-contrast theme. Exact values are plan detail; each gets a snapshot.

## Acceptance

- Unit: theme registry test (default values equal the legacy triples;
  switch changes reads; out-of-range index clamps).
- Snapshots: all existing snapshots pass unmodified (default identical);
  per-theme verification is TestBackend cell `fg`/`bg` assertions (NOT
  snapshots — `screen_text` drops styles, so per-theme snapshots would be
  byte-identical files proving nothing): render a representative screen per
  theme and assert key cells (accent border, HOT badge, SAFE span, a pedal
  tile) carry that theme's exact `Rgb` values, plus eyeball meter/badge
  legibility per theme.
- Manual: switch, restart, theme persists; every modal visited once
  (checklist in the plan).
- No `pub(super) const …: Color` remains in `styles.rs` except the fixed
  pedal liveries.

## Decisions (locked by the human, 2026-10-12)

1. **Verification: TestBackend cell `fg`/`bg` assertions per theme.**
   Snapshots stay layout-only.
2. **Pedal liveries: fixed real-world colors + rename tiles to real
   product names.** Proposed mapping (human corrects at plan review):
   NOISE GATE → keep (utility); WHAMMY → keep (already a product name);
   WAH → "CRY BABY"; COMP → "DYNA COMP"; FUZZ → "BIG MUFF" (default TYPE
   voice); TS-808 / DS-1 / ML-2 METAL CORE → keep; PRE-AMP EQ → keep;
   UNI-VIBE → keep; CLEAN BOOST → "POWER BOOST"; GRAPHIC EQ → "GE-7";
   PARAMETRIC EQ → keep; FLANGER → per default kind (plan resolves);
   CHORUS → "CE-2"; PHASER → per default kind (plan resolves);
   TREMOLO / VIBRATO → keep; DELAY → per default kind (plan resolves);
   SPRING REVERB → keep. Tile-width consequences are plan detail;
   snapshots re-blessed for the rename.
3. **Ship: current default theme ONLY.** Full infrastructure (Theme
   struct, switch, `ui.conf`), single theme — no new palettes now; theme
   #2 becomes trivial later. The key + conf path is proven with one entry
   (press = "Theme: Default" toast + idempotent write-back).
4. **Switching: cycle key + `ui.conf` write-back.** Exact key resolved in
   plan (no bare letter free — modifier/F-key/modal entry).
