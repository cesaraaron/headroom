# Mouse click-to-focus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add mouse click-to-focus to the main panels: click a panel to focus it, click a knob/tile/ribbon stage/master cell to focus that cell, and in the timeline click a track row to select it and the ruler/waveform to seek.

**Architecture:** `draw` records the interactive regions it renders into a `HitMap` (`Vec<(Rect, HitTarget)>`) and returns it; the event loop hit-tests mouse presses against the last frame's map and applies the target through the *existing* focus state (`focus`, `chain_cursor`, `header_on_master`, `panels`, `panel_mem`, the timeline selection/seek). Mouse events are ignored while any overlay is open. Mouse capture is enabled for the whole run.

**Tech Stack:** Rust, `crossterm` (mouse capture + `Event::Mouse`), `ratatui` (`Rect`); the existing `ui` input/focus modules.

**Spec:** `docs/superpowers/specs/2026-10-10-mouse-click-focus-design.md`

## Global Constraints

- UI-thread only; nothing added to the audio path.
- Focus-only: a click changes focus, never a value.
- Overlays are keyboard-only: any open overlay swallows mouse events.
- Enter/leave cleanly: mouse capture is disabled on exit alongside raw mode.
- Follow existing patterns in `src/ui/input.rs` for focus helpers; do not invent new focus rules.

## Review Focus

Inputs/conditions the spec implies but no single task tests, most likely to bite:
1. **Zero-width regions** (a panel narrower than its gutters) — a recorder must skip empty rects, or hit-testing returns a target for a cell that isn't there.
2. **Clicks in borders/gaps** — must yield `None` (do nothing), not the panel behind.
3. **Overlap order** — the timeline plot must win over the row it sits in, and a knob over the panel; `hit_test` returns the *last* match.
4. **Overlays open** — a click must not reach the panels behind a modal.
5. **Off-board / hidden cells** — only drawn cells are recorded, so a click can never focus an unreachable knob.

---

### Task 1: Hit-testing primitives

**Files:**
- Create: `src/ui/hit.rs`
- Modify: `src/ui/mod.rs` (add `mod hit;`)
- Test: `src/ui/hit.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  ```rust
  pub(super) struct TimelineAxis { pub x0: u16, pub start: usize, pub len: usize, pub wave_w: usize }
  impl TimelineAxis { pub fn frame_at(&self, col: u16) -> usize; pub fn col_at(&self, frame: usize) -> u16; }
  pub(super) enum HitTarget {
      Panel(u8), Knob(usize), ChainStage(ChainStage), MasterCell,
      TimelineTrack(u64), TimelineSeek(TimelineAxis),
  }
  pub(super) type HitMap = Vec<(ratatui::layout::Rect, HitTarget)>;
  pub(super) fn hit_test(map: &HitMap, col: u16, row: u16) -> Option<&HitTarget>;
  ```

- [ ] **Step 1: Write the failing tests** in `src/ui/hit.rs`:
  - `axis_round_trips`: for `TimelineAxis { x0: 5, start: 1000, len: 48000, wave_w: 100 }`, `col_at(frame_at(c))` equals `c` for `c` in `[x0, x0+99]` within 1.
  - `axis_clamps`: `frame_at(x0 - 3)` and `frame_at(x0 + 200)` stay within `[start, start+len)`.
  - `hit_test_returns_last_match`: two overlapping rects → the second's target; a point in neither → `None`.
  - `hit_test_ignores_zero_width`: a `Rect { width: 0, .. }` never matches.

- [ ] **Step 2: Run tests to verify they fail** — `cargo test --release --lib -- ui::hit` → FAIL (module/items missing).

- [ ] **Step 3: Implement `src/ui/hit.rs`.** `frame_at` = `start + (col - x0) as usize * len / (wave_w - 1).max(1)`, clamped to `[start, start + len)`. `col_at` = `x0 + (frame - start) * (wave_w - 1) / len.max(1)`. `hit_test` = `map.iter().rev().find(|(r, _)| r.width > 0 && r.height > 0 && r.contains((col, row).into()))`. Add `mod hit;` to `src/ui/mod.rs`.

- [ ] **Step 4: Run tests to verify they pass** — `cargo test --release --lib -- ui::hit` → PASS.

- [ ] **Step 5: Commit** — `git add src/ui/hit.rs src/ui/mod.rs && git commit -m "feat(ui): hit-testing primitives for click-to-focus"`

---

### Task 2: Apply a hit target to the focus state

**Files:**
- Modify: `src/ui/input.rs` (add `apply_hit`; make `PanelMemory::remember` reachable if needed)
- Test: `src/ui/input.rs` tests

**Interfaces:**
- Consumes: `HitTarget` (Task 1); `panel_entry`, `PanelMemory`, `Panels` (existing).
- Produces:
  ```rust
  pub(super) fn apply_hit(
      target: &HitTarget,
      focus: &mut Option<usize>,
      chain_cursor: &mut ChainStage,
      header_on_master: &mut bool,
      panels: &mut Panels,
      panel_mem: &mut PanelMemory,
      board: &[bool],
      order: &[u8; CHAIN_LEN],
  );
  ```
  (Timeline targets are ignored here — the caller handles them.)

- [ ] **Step 1: Write the failing tests** in `src/ui/input.rs`:
  - `apply_hit_panel_shows_and_focuses`: from `Panels { amp: false, rig: true, timeline: true }` and `focus = Some(CHAIN_TILE)`, `apply_hit(&HitTarget::Panel(2), …)` → `panels.amp == true` and `focus == Some(AMP_START)`.
  - `apply_hit_knob_focuses_it`: `apply_hit(&HitTarget::Knob(ADD_TILE), …)` → `focus == Some(ADD_TILE)`.
  - `apply_hit_chain_stage_moves_the_cursor`: `apply_hit(&HitTarget::ChainStage(ChainStage::Cab), …)` → `chain_cursor == Cab`, `focus == Some(CHAIN_TILE)`, `header_on_master == false`.
  - `apply_hit_master_selects_the_cell`: `apply_hit(&HitTarget::MasterCell, …)` → `focus == Some(CHAIN_TILE)`, `header_on_master == true`.
  - `apply_hit_remembers_for_tab`: after `apply_hit(&HitTarget::Panel(4), …)`, `Tab` recall enters panel 4 at the expected cell (assert via `panel_mem` behaviour or `cycle_panel`).

- [ ] **Step 2: Run to verify fail** — `cargo test --release --lib -- ui::input::tests::apply_hit` → FAIL.

- [ ] **Step 3: Implement `apply_hit`** in `src/ui/input.rs`, reusing `panel_entry`; for `Panel(n)` set `panels` visible then `*focus = panel_entry(n, board, order)` (never hide). Call `panel_mem.remember(*focus)` at the end for every non-timeline target.

- [ ] **Step 4: Run to verify pass** — `cargo test --release --lib -- ui::input::tests::apply_hit` → PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(ui): apply a click target to the focus state"`

---

### Task 3: `draw` returns a `HitMap` and records the chain row

**Files:**
- Modify: `src/ui/draw.rs` (`draw` signature/return; `render_header`, `render_chain_row`)
- Modify: `src/ui/mod.rs` (the `terminal.draw` closure stores the returned map)
- Test: `src/ui/draw.rs` tests

**Interfaces:**
- Consumes: `HitMap`, `HitTarget` (Task 1).
- Produces: `pub(super) fn draw(...) -> HitMap` (return added; args unchanged).

- [ ] **Step 1: Write the failing test** in `src/ui/draw.rs` (`render_with` already renders into a `TestBackend` and returns the terminal): add `hit_map_records_chain_and_master` — build the full `draw`, get the `HitMap`, and assert `hit_test(map, x, y)` for the column of a known chain stage's label yields `Some(ChainStage(_))`, and the MASTER cell column yields `Some(MasterCell)`. Locate columns from the rendered row text (find the label's x in the first row's string).

- [ ] **Step 2: Run to verify fail** — `cargo test --release --lib -- ui::draw::tests::hit_map_records_chain_and_master` → FAIL.

- [ ] **Step 3: Implement.** Change `draw` to build `let mut hits: HitMap = Vec::new();` and return it. `render_header`/`render_chain_row`: carry an optional `HitTarget` per chain entry — change `chain` to `Vec<(Span<'static>, bool, Option<HitTarget>)>`, and in `render_chain_row` accumulate each entry's x-run within the (scrolled) row and push `(rect, target)` for entries with a target. Record `ChainStage(s)` for each stage entry and `MasterCell` for the `MASTER …` entry (skip `OUTPUT` and arrows). In `src/ui/mod.rs`, hold `let mut hit_map: HitMap = Vec::new();` outside the loop and inside the closure do `hit_map = draw(...);` before rendering overlays.

- [ ] **Step 4: Run to verify pass** — same command → PASS; also `cargo test --release --lib -- ui::draw` (snapshots unchanged).

- [ ] **Step 5: Commit** — `git commit -am "feat(ui): draw returns a hit map; record the chain row and master"`

---

### Task 4: Record amp/cab knob and pedal-tile regions

**Files:**
- Modify: `src/ui/draw.rs` (`draw` threads `&mut HitMap` into `render_amp_panel`/`render_amp_box`/`render_cab_box`/`render_rig`/`render_pedal_tile`/`render_add_tile`)
- Test: `src/ui/draw.rs` tests

**Interfaces:**
- Consumes: `HitMap`/`HitTarget` (Task 1).
- Produces: no new public API — `draw` records extra regions.

- [ ] **Step 1: Write the failing tests**:
  - `hit_map_records_an_amp_knob`: with panel 2 visible, the centre of the first amp knob's cell yields `Some(Knob(idx))` where `idx` is that knob's focus index (`AMP_START + i`).
  - `hit_map_records_a_pedal_tile`: with panel 4 visible and a pedal on the board, the pedal tile's rect yields `Some(Knob(PEDALS[i].start))`.
  - `hit_map_records_the_add_tile`: the `+ ADD` tile yields `Some(Knob(ADD_TILE))`.

- [ ] **Step 2: Run to verify fail** — `cargo test --release --lib -- ui::draw::tests::hit_map_records_an_amp_knob` → FAIL.

- [ ] **Step 3: Implement.** Thread `hits: &mut HitMap` through the panel renderers. In the amp/cab knob loops, record `(cell_rect, HitTarget::Knob(AMP_START + i))` (and the cab/mic equivalent `MIC_START + i`) for each *drawn* knob (skip cells with zero width). In `render_rig`, record `(tile_rect, HitTarget::Knob(PEDALS[i].start))` for each on-board pedal tile and `(tile_rect, HitTarget::Knob(ADD_TILE))` for the add tile. Record panel-level fallbacks last (see Task 5 note) — for now, no panel fallback.

- [ ] **Step 4: Run to verify pass** — the three tests → PASS; `cargo test --release --lib -- ui::draw`.

- [ ] **Step 5: Commit** — `git commit -am "feat(ui): record amp/cab knob and pedal-tile hit regions"`

---

### Task 5: Record timeline regions; `PracticeUi` click helpers

**Files:**
- Modify: `src/ui/practice.rs` (`render` thread a `&mut HitMap`; add `select_track`/`seek_frame`)
- Modify: `src/ui/draw.rs` (pass the map into `ui.render`, and record a `Panel(3)` fallback for the timeline's non-plot area)
- Test: `src/ui/practice.rs` tests

**Interfaces:**
- Consumes: `HitMap`/`HitTarget`/`TimelineAxis` (Task 1).
- Produces:
  ```rust
  impl PracticeUi {
      pub(super) fn select_track(&mut self, id: u64);   // same effect as ↑/↓ landing on that row
      pub(super) fn seek_frame(&mut self, frame: usize, practice: &Practice); // same as ←/→ seek
  }
  ```
  and `PracticeUi::render(..., hits: &mut HitMap)`.

- [ ] **Step 1: Write the failing tests** in `src/ui/practice.rs`:
  - `select_track_selects_the_row`: after `select_track(id)`, `session.selected() == Some(id)`.
  - `seek_frame_moves_the_playhead`: `seek_frame(f, &practice)` sets `practice.position()` to `f` (via the same path ←/→ uses).
  - `timeline_records_a_seek_axis`: rendering the pane records a `TimelineSeek` whose `frame_at(x0)` equals the view start (round-trip against `view`).

- [ ] **Step 2: Run to verify fail** — `cargo test --release --lib -- ui::practice::tests::select_track_selects_the_row` → FAIL.

- [ ] **Step 3: Implement.** In `PracticeUi::render`, for each track row push `(row_rect, HitTarget::TimelineTrack(track.id))` **first**, then for the plot area (ruler + waveform body; `x0 = area.x + GUTTER`) push `(plot_rect, HitTarget::TimelineSeek(TimelineAxis { x0, start: view_start, len: view_len, wave_w }))` **after** — so the plot wins inside a row. `select_track` mirrors the ↑/↓ selection; `seek_frame` mirrors the ←/→ seek (clamp `[0, span_frames)`, assert `sample_rate>0`). In `draw`, pass the map into `ui.render` and push a `Panel(3)` fallback over the timeline strip (inserted **before** the row/plot regions so they win).

- [ ] **Step 4: Run to verify pass** — the three tests → PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(ui): record timeline hit regions and add click helpers"`

---

### Task 6: Event plumbing — mouse capture and dispatch

**Files:**
- Modify: `src/ui/mod.rs` (capture enable/disable; `Event::Mouse`; overlay guard; dispatch)
- Test: `src/ui/mod.rs` tests (guard) + manual run

**Interfaces:**
- Consumes: `hit_test`, `apply_hit` (Tasks 1–2), `PracticeUi::select_track`/`seek_frame` (Task 5), the `hit_map` (Tasks 3–5).

- [ ] **Step 1: Write the failing test** in `src/ui/mod.rs`:
  - `mouse_is_ignored_while_an_overlay_is_open`: extract the guard as `fn mouse_blocked(help: bool, presets: bool, sessions: bool, amp: bool, cab: bool, cal: bool, add: bool, export: bool, prompt: bool) -> bool` (or take a small struct) and assert it is `true` when any flag is set and `false` when none are.

- [ ] **Step 2: Run to verify fail** — `cargo test --release --lib -- ui::tests::mouse_is_ignored_while_an_overlay_is_open` → FAIL.

- [ ] **Step 3: Implement.**
  - Import `EnableMouseCapture`, `DisableMouseCapture`, `MouseButton`, `MouseEventKind` from `crossterm::event`; add them to the startup `execute!` and the teardown.
  - Add the `mouse_blocked(...)` guard covering every overlay flag used by the loop (help, presets, `session_browser.open`, amp, cab, `ir_browser.open`, `amp_browser.open`, add-pedal, tuner, metronome, `cal_ui.open()`, export, `practice_ui.gain_open()`/`move_open()`, `unsaved_prompt.is_some()`).
  - In the loop, match `Event::Mouse(m)` with `m.kind == MouseEventKind::Down(MouseButton::Left)`: if `mouse_blocked(...)` → ignore; else if let `Some(target) = hit_test(&hit_map, m.column, m.row)` → `match target { HitTarget::TimelineTrack(id) => practice_ui.select_track(*id), HitTarget::TimelineSeek(ax) => practice_ui.seek_frame(ax.frame_at(m.column), &practice), other => apply_hit(other, &mut focus, &mut chain_cursor, &mut header_on_master, &mut panels, &mut panel_mem, &board, &params.chain_slots()) }`. Keep the existing `Event::Key` arm unchanged; make the `event::read` match handle both variants.

- [ ] **Step 4: Run to verify pass** — `cargo test --release --lib -- ui` → PASS. Then a **manual check**: `cargo run --release`, confirm clicking each panel focuses it, clicking a knob/pedal tile/ribbon stage/master cell focuses it, clicking a track row selects it, clicking the waveform seeks, and that clicking while the `K` help (or any modal) is open does nothing (and normal terminal text-selection still works with Shift).

- [ ] **Step 5: Commit** — `git commit -am "feat(ui): mouse click-to-focus via event plumbing"`

---

## Self-Review

- **Spec coverage:** focus-only (Task 2 + 6 dispatch), timeline row+seek (Task 5), overlays ignored (Task 6 guard), always-on capture (Task 6), `draw -> HitMap` (Tasks 3–5), reuse of focus state (Task 2). Covered.
- **Type consistency:** `HitTarget`/`HitMap`/`hit_test`/`TimelineAxis`/`apply_hit`/`select_track`/`seek_frame` names are used identically across tasks.
- **Review Focus tests placed:** zero-width + gaps + overlap-order in Task 1; overlay guard in Task 6; off-board cells are structural (only drawn cells are recorded) — asserted by the pedal-tile test in Task 4.
