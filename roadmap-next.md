# headroom — next-roads plan

This is the **off-path roadmap**: work deliberately outside the fidelity plan
([`docs/fidelity-plan.md`](docs/fidelity-plan.md) /
[`docs/fidelity-implement.md`](docs/fidelity-implement.md)).

The fidelity path stays the primary track. This document captures product and
engineering directions so they don't get lost, and is the acceptance record for
when each is picked up. Nothing here is committed to a release order by existing;
the ordering at the bottom is a recommendation.

Each item states **why**, **scope**, **audio-thread constraints**, and
**acceptance**, mirroring how the fidelity docs are written.

---

## Handoff — current state (2026-10-12)

**A fresh agent should be able to continue from this section alone.** The
off-path roadmap is DONE except blocked Phase 3. What shipped, what remains,
and the working conventions.

### Just shipped (on `main`, pushed)

| Area | Commits | Where |
| --- | --- | --- |
| DSP perf batch (Item 2) | `803d96e`…`932cec8` | `src/dsp/effects/*` caches, `src/dsp/cab/ir.rs` recurrence + sparse taps; `CabBank::new` 221→24 ms; spec/plan `docs/superpowers/{specs,plans}/2026-10-11-dsp-perf-batch*` |
| F1 limiter remainder | `ab05e93`…`597b6de` | 4× true-peak detector + oversampled clamp (`src/dsp/limiter.rs`), GR readout on OUT row, true-peak hold; baseline re-blessed; `…/2026-10-11-f1-limiter*` |
| Reorder safety (B2/B4/B5) | `3b2b005`…`292e449` | mono-after-cab toast (`src/ui/input.rs`), feedback-tail clear on reorder (`DspChain::snapshot_order`); B4 verified-and-closed; `…/2026-10-11-reorder*` |
| Amp gaps remainder (C2–C5) | `1edf712`…`c83b89a` | Randall load-only ripple, `SpeakerLoad` rename; Vox/Hiwatt/Fender ripple measured inaudible and reverted (tests `#[ignore]`d); no re-bless needed; `…/2026-10-11-amps*` |
| Palette + pedal rename (3.3) | `2177d70`…`f23db93` | `Theme` registry (single default), F3 + `ui.conf`, product tile names; `…/2026-10-12-palette*` (spec) / `2026-10-11-amps.md`-dated plans live under `docs/superpowers/plans/` |
| Equal IN/OUT meter chrome | `03b166b` | `src/ui/draw.rs` unified meter chrome |
| Rename → `headroom` (v0.3.0) | `293dffd` | crate/binary, config dir with a chained one-time migration, `HEADROOM_*` env vars (legacy `RUSTY_AMP_*` still read), CLAP host id, release assets, snapshots re-blessed; GitHub repo renamed `cesaraaron/headroom` |

### Chain audit 2026-10 (Phases 1–2 complete; Phase 3 offline-complete)

`docs/chain-audit-2026-10.md` is the spec and carries the live status block;
`docs/fidelity-implement.md` is the as-built record (`chain-audit-p1` … `p3g`).
All **5 CRITICAL** and **14 HIGH** findings are fixed. Phase 3 is fixed for
everything offline-verifiable: 1.5, 2.7–2.10, 3.6–3.9, 4.6–4.8, 5.6, 5.7.

Still open, and why: **1.3** (output-rate fallback), **1.4** (ring
prefill/watermark), **1.6** (loop-wrap/punch crossfades; undo is fixed), **1.7**
(tuner analysis/XRUN off the audio thread; the window is fixed), **5.4** (frozen
smoothers while bypassed, 6 ms dry/wet comb; ramp init is fixed), **5.5** (AU
switch mid-block mute/`comp_delay`). Each needs a live interface or a
control-thread redesign. The fidelity baseline is re-blessed with each tone
change; the blind A/B listening pack lives in `target/listening/` (gitignored).

Key findings to not re-derive: sweeping-notch stages keep exact
per-sample coefficients (control-rate interpolation moves LTAS bins —
see `uni_vibe.rs::last_rate` note); `sagged_rail` cancels supply exactly
in the linear zone (clean power stages can't imprint ripple); 120 Hz
sidebands need f0=200/WIN=24000 bins (never integer-cycle at 225/30720);
fidelity `--check` is NaN-blind (needs an `is_finite` guard, Track-4).

### Remaining

- **Blocked:** fidelity Phase 3 (measured-IR / cab-capture match) needs
  re-amp + mic captures — do not start.
- **If wanted:** theme #2+ (infrastructure in place — data entry +
  eyeball); Vox power-drive re-voice (×9 trim leaves the power stage
  clean — no grind/ghosts; new project, re-blesses); `is_finite` harness
  guard; the deferred minors listed in each batch's ledger (now in git
  history — `git log` for `Ruling:`/`minor (deferred)` lines).
- **Owed by the human:** palette manual checklist (F3 toast, restart
  restore, modal tour, narrow terminal); amp listening pass (Randall hot,
  Vox cranked, mother_solo/stairway A/B from F1).

### Next up

Nothing scheduled. The off-path roadmap is complete; play the rig and
file new work as it earns it.

**Blocked:** fidelity Phase 3 (measured-IR / cab-capture match) needs re-amp + mic
captures — do not start.

### Known deferred minors (recorded; no user-facing defect)

- `TimelineAxis::col_at` is `#[cfg(test)]`-only (`src/ui/hit.rs`).
- The mouse overlay-guard test (`src/ui/mod.rs` tests) exercises a synthetic bool
  slice, not the real ~19-flag array at the call site — there is no test seam
  without extracting a struct.
- A MIDI control that emits two CC numbers per motion binds the second (`cc_since`
  returns the *latest*, not the first).
- `D` on an already-unbound MIDI target still rewrites `midi.conf`.
- `MidiState::write_conf` errors are swallowed (`let _ =` at both call sites).

### Working conventions

- **Branch**: work directly on `main`. The human A/Bs the result, then says the word
  to push; never push unprompted.
- **Gates before every push** — all must pass:
  ```bash
  cargo fmt --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --release
  cargo test                                   # debug
  cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml
  ```
- **Re-bless a snapshot** after an intentional UI change:
  `INSTA_UPDATE=always cargo test --release --lib`, then
  `find . -name '*.snap.new' -not -path './target/*' -delete`.
- **Workflow** for anything non-trivial: spec first
  (`docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`), then a plan
  (`docs/superpowers/plans/…`), then TDD execution task by task. Parallel-agent
  rules: `docs/parallel-work.md`.
- **Audio path**: `AGENTS.md` → "DSP constraints" — no allocation, blocking or
  locking on the audio thread; knob values are 0.0–1.0 internally.
- **Measure before you refactor**: the compact-mode attempt on 2026-10-11 was
  justified by a probe that forgot to pass the timeline and was wrong; a two-minute
  render probe settled it. Prefer a probe over an assumption.

## Track 1 — Playability and performance control

### 1.1 MIDI input + expression-pedal control

**Why.** The wah is currently an *auto-wah* (`src/dsp/effects/wah.rs`); every
preset comment that claims a "rocked wah" describes something the software can't
do. An expression treadle also turns the rig into a playable instrument rather
than a knob-only emulator. This is the single highest playability payoff.

**Scope.**

- A MIDI backend on the control thread (no allocation or lock scope on the audio
  thread). Learn/bind screen: map a CC to a target (wah position, amp gain,
  delay mix, master width, …).
- Continuous targets are already `Arc<AtomicF32>`; a bound CC writes them. The
  audio thread keeps reading the same atomics — no new hot-path code.
- Wah gains a **manual mode**: position comes from the bound control instead of
  the envelope follower. Keep auto-wah as a mode, not a replacement.
- Add a **learn** workflow (move the pedal, it binds) and persistence in the
  config dir.

**Audio-thread constraints.** MIDI parsing and binding live off the callback;
only atomic stores cross the boundary (same discipline as A/B and the timeline).

**Acceptance.**

- A bound CC moves the wah smoothly through its range in manual mode; auto mode
  is unchanged when unbound.
- Learn/bind persists across restarts; unbinding restores auto/default.
- Unit tests for the mapping; a manual pass with a real controller.

**Status — done (CC bind via config; no learn UI yet).** The wah gained a
**manual treadle mode** (`wah mode = 1`) with a `POSITION` knob (250 Hz–2.5 kHz),
and `src/midi.rs` (new `midir` dependency) connects to the first MIDI input and
drives `wah_position` from the **Expression CC (11)**, switching the wah to
manual when the pedal moves. Bindings live in `~/.config/headroom/midi.conf`
(`enabled`, the legacy `cc = 11` shorthand, and `<cc> = <target>` for
`wah_position`, `delay_mix`, `reverb_mix`, `boost_gain`, …). No hardware was
available to verify the live path here — the parsing/mapping are unit-tested, and
the app runs fine with no controller. The learn/bind **screen** shipped
2026-10-10 (`F2`; `src/ui/midi.rs`) — see the handoff section.

### 1.2 Tap-tempo and MIDI clock sync

**Why.** Delay times are set by a normalized knob; players set delay by tapping
or from the session clock.

**Scope.** A tap-tempo binding computes BPM and writes `delay_time`; optional MIDI
clock → `delay_time`. Keep the knob authoritative when tapped.

**Acceptance.** Tap a few beats and the delay locks; MIDI clock changes track;
tests for the tap-tempo windowing.

**Status — done.** `;` taps: two or more steady taps set the delay `TIME` to the
tapped interval (clamped to 0–500 ms) and show the BPM in the footer;
`src/tap_tempo.rs` is a tested pure state machine (too-fast taps ignored, slow
taps restart the count). With `clock = true` in `midi.conf`, an incoming 24-ppqn
**MIDI clock** sets the delay `TIME` to the beat and the footer shows
`MIDI CLK <bpm>`; `src/midi.rs::MidiClock` is a tested interval→BPM estimator
(smoothed, rejects burst/gap intervals) that also follows `Start`/`Continue`/
`Stop`. When the clock stops, the `TIME` knob and the `;` tap take back over.

---

## Track 2 — Practice and composition

### 2.1 Looper / overdub

**Why.** The practice player already decodes, resamples, and plays backing
tracks against a shared timeline (`src/practice.rs`, `src/dsp/player.rs`). A
looper is the natural next step and makes the app a writing tool.

**Scope.**

- Record a **loop region** on the timeline: capture the calibrated dry guitar
  (or the processed mix — decide), then play it back layered.
- **Overdub**: layer successive passes; **undo** the last layer.
- Reuse the existing lock-free install/dispose discipline for new buffers
  (`rtrb`), and the monitor-only bus so loops are never captured in a rendered WAV.

**Audio-thread constraints.** Loop buffers are preallocated on the control thread
and installed lock-free; displaced buffers ship back for disposal (same as
practice tracks, plugin inserts, external IRs).

**Acceptance.** Record → loop → overdub → undo round-trips; loop timing stays
sample-aligned; the recording tap excludes loops; tests for the loop cursor math.

**Status — done.** `src/looper.rs` implements a monitor-only looper (record →
loop → overdub → undo) with preallocated buffers and lock-free control; the loop
bus rides the monitor path after the recording tap, like the metronome. Keys: `Y`
record/pause/resume, `F` clear, `,` overdub, `.` undo; state shows in the footer.
Unit tests cover cursor wrap, overdub, undo and no-grow; a whole-bank
integration test (`tests/bundled_presets.rs`) guards preset renders.

### 2.2 Timeline UX & navigation

**Why.** The practice timeline needed a readable, navigable waveform while
writing and recording.

**Scope / acceptance.** F1–F11, all shipped: empty-timeline navigation, a time
ruler on a fixed gutter, fractional seek steps, a live recording waveform, taller
rows with `Tab` row-zoom, adaptive + braille waveforms, playhead-centred time
zoom, and waveform gain modes. **Status — done.** The plan and as-built records
were folded away; the shipped behaviour is documented in the module docs of
`src/practice.rs` and the in-app `K` reference.

### 2.3 Loop-region export

**Why.** The offline export was whole-session only; capturing just a good loop
meant exporting everything and trimming by hand.

**Scope.** The `E` dialog gains a `Tab` range control: **full session** (default,
unchanged) or exactly the **loop region** `[loop_start, loop_end)` (no appended
tail; the chain is warmed from frame 0 for continuous state). **Status — done**;
as-built D5, resolving the old timeline limitation #10.

---

## Track 3 — UX and product

### 3.1 Preset browser: search, tags, favorites, A/B compare

**Why.** 17 bundled presets already fill the modal; users will have many more.
Finding and comparing tones should not be scrolling.

**Scope.** Fuzzy search over name/description; tag and favorite fields
(backward-compatible TOML, defaulted); an A/B compare that flips between two
selected presets without leaving the modal.

**Status — favorites + search + tags done.** `F` stars the selected preset (a `★`
column), persisted by name in `~/.config/headroom/favorites.txt`.
**Type-to-filter** matches name/description/**tags** (printable keys filter,
`Backspace` deletes, `Esc` clears then closes; commands are the upper-case
letters). The `tags = [...]` preset field is defaulted and shown in the list; the
17 bundled presets are tagged (artist/role). **A/B compare** is in: `X` in the
browser flips between the two most recently applied presets, keeping the modal
open. **Track 3.1 is complete.**

**Acceptance.** Snapshot tests for the modal; loading/search round-trips;
favorites persist.

### 3.2 Onboarding and device profiles

**Why.** First-run asks device → channel → output and then input calibration. A
saved per-interface profile (channels, trim, pickup class) removes repeats.

**Scope.** Persist a named profile keyed to the interface name; a one-screen
setup that reuses it.

**Acceptance.** Relaunch reuses the profile; changing interface prompts once.

**Status — effectively done (pre-existing).** Device selection is remembered by
*name* in `~/.config/headroom/audio.conf` (survives re-enumeration), and
`input-calibration.toml` stores **one calibration row per
`device + channels + channel`** (trim, pickup class, reference version) that is
matched on startup — i.e. a per-interface profile already. A dedicated
one-screen "profiles" setup is not needed; scope stays as-is.

### 3.3 Theming and layout polish

**Why.** The TUI is fixed-colour and can crowd small terminals.

**Scope.** A couple of colour themes, a compact mode, and graceful narrow-width
layout. Keep the ratatui snapshot tests meaningful.

**Acceptance.** Screenshots/snapshots per theme; no clipping at a small size.

**Status — done 2026-10-12 (infrastructure + rename, single theme).**
The narrow-width half landed 2026-10-11; the palette half landed 2026-10-12
(`2177d70`…`f23db93`, spec `docs/superpowers/specs/2026-10-12-palette-design.md`):
a `Theme` registry in `src/ui/styles.rs` (single default palette,
value-for-value legacy — all existing snapshots pass unmodified), ~400 call
sites migrated flag-day, F3 cycle + `~/.config/headroom/ui.conf` write-back,
and pedal tiles renamed to product names (CRY BABY, DYNA COMP, BIG MUFF,
POWER BOOST, GE-7, CE-2; multi-kind pedals stay generic). Verified by
TestBackend cell-color assertions (snapshots are glyphs-only). Theme #2 is
now data entry + eyeball.

### 3.4 UI quality-of-life (rolling)

**Why.** Small interaction wins that don't fit a single plan item.

**Status — several landed:**
- **Open on the timeline; close to it.** The app opens with only the practice
  timeline (plus the always-visible chain ribbon) and focus on the timeline;
  closing a focused panel returns focus to the timeline when it is open (else the
  ribbon). `Panels::timeline_only()`.
- **Quick-add a pedal.** On the pedalboard, `a` opens the add-pedal picker
  (elsewhere `a` still opens the amp browser); `D` removes the focused pedal.
- **Timeline ruler polish.** A gutter divider and minor ticks, drawn as a low
  vertical tick.
- **Expiring notices.** Status toasts have a lifetime rather than sticking.
- **Factory defaults, import library, list search.** A reset-to-factory path,
  an import library browser, and type-to-search in the session/preset lists.

**Open follow-ups.** Both are now closed: the **MIDI learn/bind screen** (1.1)
shipped 2026-10-10, and the narrow-width half of 3.3 shipped 2026-10-11. The
remaining 3.3 work is the palette refactor.

---

## Track 4 — Engineering and quality

### 4.1 CI

**Why.** The gates (`fmt`, `clippy -D warnings`, `test --all-features`, harness
`--check`) should run on a clean clone, not by hand.

**Scope.** A GitHub Actions workflow running the gates on push/PR with a Rust
toolchain cache, warn-free and fast enough to be useful.

**Status — done (and extended).** `.github/workflows/test.yml` already ran
`fmt --check`, `clippy --all-features -D warnings`, `cargo test`, and a
pending-snapshot guard on Linux. **Added a macOS job** that runs
`cargo test --all-features` (covering the AudioUnit host, which only compiles on
macOS) and the **fidelity harness** `--check` against
`docs/fidelity/baseline-synth-48k.toml` (generated on aarch64-apple-darwin), so
preset/DSP voicing drift now fails CI.

**Acceptance.** Green on `main`; a deliberately broken commit fails the right gate.

**Caveat — resolved (2026-09-27).** Push/PR triggers were briefly not creating
runs; Actions were re-enabled and push-triggered CI now runs both jobs green
(Linux gates + the macOS `--all-features`/fidelity `--check`) on every `main`
push.

### 4.2 Benchmarks

**Why.** The audio path and cab construction have real budgets (the docs already
note `DspChain::new` is ~1.7 s in debug). Regressions should be visible.

**Scope.** Benchmarks for the callback cost per stage and for `CabBank::new`
(construction), runnable locally and (optionally) in CI with generous thresholds.

**Status — done.** `examples/bench.rs` (dependency-free) times `DspChain::new`,
`CabBank::new`, and `process_block` on representative presets, printing µs/block
and % of the 10 ms realtime budget. Measured on the maintainer rig: chain build
~222 ms, cab build ~221 ms, `process_block` 3.3–3.6 % of realtime. **Not yet a CI
gate** (thresholds would be machine-dependent).

### 4.3 Docs site or generated user guide

**Why.** The Eleventy site was removed in the de-fork; the user guide is now the
README plus the in-app `K` overlay. A generated guide would help onboarding.

**Scope.** Either a small static docs build from Markdown, or a `cargo doc`/
mdBook guide mirroring the README + `K` reference. No docs-parity rule beyond
keeping the README honest.

**Status — done.** `docs/guide.md` covers build/run, calibration (`N`), the
signal chain, presets (browser, favorites, search, save/export/import, config
paths), practice/recording, and plugins; linked from the README. A hosted docs
site was not re-added.

---

## Recommended order

_Done: everything below plus all earlier items — the off-path roadmap is
complete (see the handoff section at the top)._

Nothing remaining except blocked Phase 3 (measured-IR / cab-capture match —
needs re-amp + mic captures) and optional follow-ups (theme #2+, Vox
power-drive re-voice, `is_finite` harness guard).

The **fidelity path is closed**: the amp effects loop (Phase 2 item 5) was
retired as out-of-scope, the TS-808 was corrected against the circuit (no 340 Hz
input HP; 720 Hz gain shelf), and the built-in↔AU toggle is now clickless. Only
the measurement-bound Phase 3 items (measured-IR / cab-capture match) remain, and
those are blocked on re-amp/mic captures.
