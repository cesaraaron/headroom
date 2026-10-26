# MIDI learn / bind screen — implementation plan

> **For agentic workers:** use superpowers:executing-plans (inline) or
> superpowers:subagent-driven-development. Steps use `- [ ]` checkboxes.

**Goal:** an `F2` modal that lists the 15 MIDI-bindable targets, learns a binding
from the physical control, unbinds, and persists to `~/.config/headroom/midi.conf`
with the running connection picking each change up immediately.

**Architecture:** `src/midi.rs` grows a shareable `MidiState` (binding table behind
a `Mutex`, plus a lock-free learn channel: a CC sequence counter and the packed
last CC). The `midir` callback reads the table with `try_lock` and publishes every
CC to the learn channel; the UI owns the modal and polls the channel once per
frame. No audio-thread involvement.

**Tech stack:** Rust, `midir` (existing), `ratatui`/`crossterm`, `std::sync::Mutex`
+ atomics.

**Spec:** `docs/superpowers/specs/2026-10-10-midi-learn-screen-design.md`

## Global constraints

- **Never touch the audio thread.** MIDI runs on its own `midir` thread; its
  callback must not block (`try_lock` only) and must not allocate.
- The UI owns all modal state; the audio path is unchanged.
- Persist via temp-file + rename so a crash can't truncate `midi.conf`.
- One CC ↔ one target: binding either side clears the other's previous owner.

## Review focus

1. **Lock discipline** — the callback uses `try_lock` and must never `lock()`;
   a dropped CC while the UI holds the lock is acceptable and silent.
2. **Learn races** — `cc_since` must not re-fire for the same event; a CC that
   arrives between arming and the first poll still binds.
3. **Collision** — binding CC `c` to target `t` must clear any other target on `c`
   and any other CC on `t`, and the written conf must contain no duplicates.
4. **Config preservation** — `enabled`/`clock` not edited by the screen must
   survive a rewrite; an empty table must not silently fall back to CC 11 → wah.
5. **Reconnect** — replacing the `MidiHandle` must not leak or double-connect.

---

### Task 1: Target names, config write-back, binding table

**Files:** `src/midi.rs`; tests in `src/midi.rs`.

**Interfaces:**
- Produces:
  ```rust
  impl MidiTarget {
      pub fn name(self) -> &'static str;   // config token, inverse of `parse`
      pub fn label(self) -> &'static str;  // display name
      pub const ALL: [MidiTarget; 15];
  }
  pub struct MidiState { /* bindings: Mutex<Vec<(u8, MidiTarget)>>, enabled, clock, … */ }
  impl MidiState {
      pub fn from_conf(text: &str) -> Self;
      pub fn bind(&self, cc: u8, target: MidiTarget);
      pub fn unbind(&self, target: MidiTarget);
      pub fn bindings(&self) -> Vec<(u8, MidiTarget)>;
      pub fn enabled(&self) -> bool;
      pub fn clock(&self) -> bool;
      pub fn write_conf(&self) -> std::io::Result<()>;
  }
  ```

- [ ] **Step 1: failing tests.**
  - `target_name_round_trips`: for every `t` in `MidiTarget::ALL`, `parse(t.name()) == Some(t)`.
  - `bind_clears_colliding_cc_and_target`: bind `(20, DelayMix)`, then `(20, ReverbMix)` → only `(20, ReverbMix)`; then `(21, ReverbMix)` → only `(21, ReverbMix)`.
  - `unbind_removes_only_that_target`.
  - `conf_round_trips`: build a state (enabled=false, clock=true, `[(11, WahPosition), (20, DelayMix)]`), `render_conf` it, `from_conf` the text → same enabled/clock/bindings.
  - `rendered_conf_keeps_enabled_and_clock`.
- [ ] **Step 2: run to fail** — `cargo test --release --lib -- midi::tests` → FAIL.
- [ ] **Step 3: implement.** Add `name`/`label`/`ALL`; `MidiState` with a
  `Mutex<Vec<(u8, MidiTarget)>>`; `from_conf` reusing `parse_midi_conf` (but **not**
  its "empty → CC 11" default — that default belongs only to the config path);
  `render_conf(enabled, clock, &bindings)`; `write_conf` reads `midi_config_path()`,
  writes `midi.conf.tmp`, renames.
- [ ] **Step 4: run to pass** — same command → PASS.
- [ ] **Step 5: commit** — `feat(midi): target names, a live binding table, and conf write-back`

---

### Task 2: Live state, learn channel, start/connect

**Files:** `src/midi.rs`; tests in `src/midi.rs`.

**Interfaces:**
- Consumes: `MidiState` (Task 1).
- Produces:
  ```rust
  impl MidiState {
      pub fn cc_seq(&self) -> u64;
      pub fn cc_since(&self, seq: u64) -> Option<(u8, u8, u64)>;
  }
  pub fn start(params: Arc<Params>) -> (Arc<MidiState>, Option<MidiHandle>);
  pub fn connect(state: &Arc<MidiState>, params: Arc<Params>) -> Option<MidiHandle>;
  ```

- [ ] **Step 1: failing tests.**
  - `cc_since_reports_only_new_events`: a fresh state → `cc_since(0) == None`; after `note_cc(20, 100)` → `Some((20, 100, 1))`; `cc_since(1) == None`.
  - `cc_since_returns_the_latest`: two events → the second's `(cc, value)`.
  - `start_returns_state_even_without_a_port`: `start(params)` returns a state whose
    `bindings()` match the default conf even when `connect` is `None`.
- [ ] **Step 2: run to fail** — FAIL.
- [ ] **Step 3: implement.** Add `cc_seq: AtomicU64`, `last_cc: AtomicU32`; a
  `#[cfg(test)] fn note_cc(&self, cc, value)` (or a private `record_cc` the test
  calls) that stores `last_cc` and bumps `cc_seq`. Change `start` to
  `(Arc<MidiState>, Option<MidiHandle>)`: always build the state from the conf,
  connect when `enabled` and a port exists (drop the `bindings.is_empty()` gate).
  Extract the connect body into `connect(&state, params)`. In the callback, replace
  the frozen `bindings` with `state.bindings.try_lock()` and record every CC into
  the learn channel before writing targets.
- [ ] **Step 4: run to pass** — `cargo test --release --lib -- midi` → PASS.
- [ ] **Step 5: commit** — `feat(midi): live bindings, a learn channel, and manual reconnect`

---

### Task 3: The modal state machine

**Files:** create `src/ui/midi.rs`; `mod midi;` in `src/ui/mod.rs`; tests in `src/ui/midi.rs`.

**Interfaces:**
- Consumes: `MidiState`, `MidiTarget` (Tasks 1–2).
- Produces:
  ```rust
  pub(super) enum Learn { Idle, Armed { target: MidiTarget, seq: u64 } }
  pub(super) struct MidiUi { pub(super) open: bool, cursor: usize, learn: Learn }
  impl MidiUi {
      pub(super) fn new() -> Self;
      pub(super) fn cursor_target(&self) -> MidiTarget;
      pub(super) fn move_cursor(&mut self, dir: i32);
      pub(super) fn handle_key(&mut self, code: KeyCode, state: &Arc<MidiState>) -> bool;
      pub(super) fn pump(&mut self, state: &Arc<MidiState>);
      pub(super) fn learning(&self) -> Option<MidiTarget>;
  }
  ```

- [ ] **Step 1: failing tests.**
  - `cursor_wraps_both_ways`.
  - `enter_arms_learn_on_the_cursor_target`.
  - `a_cc_binds_the_armed_target_and_writes_conf`: arm, then `note_cc`, `pump` → the
    state has the binding and `learn == Idle`.
  - `d_unbinds_the_cursor_target`.
  - `esc_cancels_learn_before_closing`: armed → `Esc` → still open, `Idle`; `Esc`
    again → closed.
- [ ] **Step 2: run to fail** — FAIL.
- [ ] **Step 3: implement.** `Learn`, `MidiUi`, and the key handling (`Up`/`Down`,
  `Enter`, `D`/`Delete`, `Esc`). `pump` polls `cc_since(seq)`, binds + `write_conf`
  on a hit. `handle_key` returns whether it consumed the key.
- [ ] **Step 4: run to pass** — `cargo test --release --lib -- ui::midi` → PASS.
- [ ] **Step 5: commit** — `feat(ui): MIDI bind modal state machine`

---

### Task 4: Render the modal

**Files:** `src/ui/midi.rs`; tests there.

**Interfaces:**
- Consumes: `MidiUi` (Task 3), `MidiState` (Task 1).
- Produces: `pub(super) fn render_midi(f: &mut Frame, ui: &MidiUi, state: &MidiState, port: Option<&str>);`

- [ ] **Step 1: failing test** — `snapshot_midi_modal`: render into a `TestBackend`
  with two bindings and one armed target; assert the snapshot (insta) and, before
  that, `text.contains("Wah position")` etc.
- [ ] **Step 2: run to fail** — FAIL.
- [ ] **Step 3: implement.** `Clear` + double-border block (title `M I D I`,
  right-aligned port or `no input`); one row per `MidiTarget::ALL` showing `label`
  and `CC nn` / `—` / the learn prompt on the armed row; cursor row highlighted;
  a footer hint line.
- [ ] **Step 4: run to pass** — the test + `cargo test --release --lib -- ui::midi` → PASS.
- [ ] **Step 5: commit** — `feat(ui): render the MIDI bind modal`

---

### Task 5: Wire it up, guard it, document it

**Files:** `src/main.rs`, `src/ui/mod.rs`, `src/ui/draw.rs` (help), `README.md`, `docs/guide.md`.

**Interfaces:**
- Consumes: everything above.
- Produces: `ui::run(..., midi: (Arc<MidiState>, Option<MidiHandle>))`.

- [ ] **Step 1: failing test** — extend the overlay-guard coverage: assert the MIDI
  modal's presence blocks mouse clicks (add it to the documented list and to the
  call-site array in the same commit).
- [ ] **Step 2: run to fail** — FAIL.
- [ ] **Step 3: implement.**
  - `main.rs`: pass `midi::start(...)` into `run`.
  - `run`: keep `Arc<MidiState>` + `Option<MidiHandle>`; add `midi_ui: MidiUi`.
  - `F2` toggles the modal; while open the modal consumes `↑↓ Enter D R Esc` and
    letter/number keys are suppressed; `R` = `midi::connect` (replace the handle).
  - Call `midi_ui.pump(&midi_state)` once per loop iteration, before drawing.
  - Draw `render_midi` in the draw closure when open; add `midi_ui.open` to the
    overlay-guard array.
  - `K` reference row `F2  MIDI bindings`; README key row; guide "Expression pedal".
- [ ] **Step 4: run to pass** — `cargo test --release --lib -- ui` → PASS, plus a
  manual pass.
- [ ] **Step 5: commit** — `feat(ui): F2 MIDI bindings screen`

---

## Self-review

- **Spec coverage:** table + learn + persist (T1), live pickup + reconnect (T2),
  state machine (T3), render (T4), wiring/guard/docs (T5). Covered.
- **Type consistency:** `MidiState`/`cc_seq`/`cc_since`/`MidiUi`/`Learn`/`render_midi`
  used identically across tasks.
- **Review focus placed:** lock discipline + learn race (T2), collision + config
  preservation (T1), reconnect (T2/T5).
