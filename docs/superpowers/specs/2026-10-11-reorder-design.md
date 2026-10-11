# Reorder safety (B2/B4/B5) — design

**Status:** written for review; implements roadmap Item 3, second workstream
("B2 / B4 / B5", `roadmap-next.md` + `docs/dsp-findings-2026-09.md` Phase B).
**Scope:** `[`/`]` moves (`src/ui/input.rs::move_selected_stage`), order
publishing (`src/dsp/mod.rs::set_chain_order`), feedback-state clearing in
`DspChain`, one permutation level test. No tone-model changes (those are C).

## Starting state (already shipped — not this work)

- Stereo rack pedal before the Amp: **refused** (`stereo_stages_follow_the_amp`,
  `move_selected_stage:490`, tested). Deliberately not sanitized: an authored
  mono-collapsed preset stays valid.
- Cab before Amp: **refused** in the move, **sanitized** at runtime in
  `set_chain_order` (every build, not just debug).
- Destructive-move toasts do not exist yet anywhere; the vehicle is the
  expiring footer `save_msg: Option<(String, Instant)>` (`ui/mod.rs:478,733,776`).

## Intent

Reordering is re-patching physical gear: levels move and tails cut, and the
rig should behave like the pedals it emulates. What it must not do is silently
destroy information the user cannot get back by moving the stage back — a mono
fold downstream of the cab is exactly that (the stereo image is gone for
everything after it, with no indication at the time of the move). And a moved
stateful stage must not drag its old regime's buffers into the new one.

Success = every `[`/`]` press either does something legible (moves, refuses
with the existing silent refusal, or moves with a toast explaining the cost),
a reordered chain always renders a sane level, and feedback tails never smear
across a reorder.

## Goals / non-goals

**Goals**
- B2: toast when a move *introduces* a mono pedal after the Cab
  (`Gate/Whammy/Wah/Comp/Fuzz/Ts/Ds/Metal/PreEq/Vibe/Boost` per
  `ChainStage::is_mono_pedal`). Toast condition is the transition, not the
  state: before = no mono-after-cab, after = mono-after-cab — so nudging
  unrelated stages in an already-folded chain does not nag. Copy (my call):
  `"<Stage> after cab: stereo folds to mono"`. Refusals stay silent as today.
- B4: VERIFY-AND-CLOSE — the permutation level test
  (`reordered_chains_stay_in_a_sane_level_band`, `dsp/mod.rs:2764`) and its
  measurement table already ship; the batch runs it and records it, no new
  code. No auto-trim per human decision (re-patch moves levels like real
  gear), documented in the existing table.
- B5: on an order change, `DspChain` clears feedback-loop audio buffers only —
  reverb tanks, delay lines, flanger buffers, phaser all-pass/feedback state.
  Kept: amp sag (explicitly protected), LFO phases (sweep continues smoothly),
  fast followers and biquads (settle on their own). Documented next to the
  clearing code. Applies to live and take chains through the shared path.

**Non-goals**
- B1 (bypass ramp), B6 (leap over hidden stages): not in the roadmap item,
  untouched.
- B2 stereo-before-Amp (refused) and cab-before-Amp (sanitized): shipped.
- B2 full-rig-AU case (post-mic signal to line FX): the help-modal row stands;
  no code guard (unchanged).
- New knobs, new params, preset-format changes.

**Anti-goals**
- No allocation, blocking, or locking on the audio thread. Order observation
  reuses the existing seqlock read; clearing is `memset`-class writes to
  already-owned buffers.
- No toast spam: one expiring footer line per introducing move, same TTL
  discipline as `save_msg`; refusals stay silent.

## Constraints

- `cargo fmt --check`, `clippy --all-targets --all-features -D warnings`,
  `cargo test --release` + debug, fidelity `--check` — expected NOT to move
  (no tone change; if it moves, the change is wrong — no re-bless here).
- `set_chain_order` stays the single publish funnel (UI moves + preset
  apply); the clearing lives in `DspChain` where the instances live, keyed
  off the observed order.
- Move semantics otherwise unchanged: ends refuse, off-board stages hold
  slots, cursor follows its stage.

## Design

### B2: the toast is a pure function of the transition

```rust
/// Names the cost of a reorder that newly places a mono pedal after the cab,
/// or `None` when the move introduces no such fold. Returns owned text: the
/// copy names the first mono-after-cab stage in `after` (which need not be
/// the moved stage — e.g. moving the Cab left over a mono pedal).
fn reorder_warning(before: &[u8; CHAIN_LEN], after: &[u8; CHAIN_LEN]) -> Option<String>
```

Checked in `move_selected_stage` after the swap passes both refusals and
before `set_chain_order` (which widens to return the warning alongside its
bool); the `[`/`]` handler shows it via `save_msg`. Unit-tested over
transitions (introduces → Some; already-folded → None;
refused moves never reach it). Copy: `"<Stage> after cab: stereo folds to
mono"` with the first mono-after-cab stage's display name.

### B4: permutation level test + documented no-trim

A test renders a fixed DI through a listed set of legal orders and asserts
every output is finite with RMS inside `[−60, +6]` dBFS (exact band is plan
detail — wide enough for boosted-EQ-into-amp, tight enough to catch silence
or lockup). The existing `swapped_chain_order_changes_output` stays. Module
docs state the no-compensation rule and why (re-patch fidelity).

### B5: clear on observed order change

`DspChain` keeps the last applied order next to the seqlock read; when it
observes a new one, it clears the four feedback stages' audio buffers before
processing, then records the order (skips `None` reads — write in flight —
without clearing). Take chain inherits it through the shared code. Exact
fields are plan detail; the rule is: recirculating audio is cleared,
everything else (sag, LFO phase, followers, biquads) is kept. A test drives
`set_chain_order` directly with two legal post-amp orders (UI moves cannot
cross the amp — stereo-before-amp is refused) and asserts a ringing reverb
tail does not survive the move; a second test asserts amp sag *does* survive
a reorder.

## Acceptance

- Unit: warning transitions (Some/None/refused-never-reaches); permutation
  levels in-band and finite; reverb tail cleared on reorder; sag preserved
  across reorder.
- UI snapshot: a toast visible after an introducing move (new snapshot,
  inspected) — or a focused footer test if the harness covers it cheaper.
- Harness `--check` green with zero baseline edits (no tone moved).
- Bench: order-change path is control-thread + memset; no `process_block`
  delta expected (record either way).

## Decisions (locked by the human, 2026-10-11)

1. **B4: no auto-trim — test + document only.** Levels move like real gear on
   re-patch; compensating behind the player's back would make the rig not
   behave like its pedals.
2. **B5: clear feedback buffers on reorder, keep LFO phases.** Tails cut like
   a physical re-patch (unplug/replug = silence, not smear).
