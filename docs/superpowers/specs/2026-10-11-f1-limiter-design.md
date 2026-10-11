# F1 limiter remainder — design

**Status:** written for review; implements roadmap Item 3, first workstream
("F1 remainder", `roadmap-next.md` + `docs/fidelity-implement.md:2379`).
**Scope:** the master-bus + final limiter in `src/dsp/limiter.rs` and its
readout path (`src/audio/mod.rs`, `src/dsp/mod.rs::Levels`,
`src/ui/draw.rs`). No other stage changes.

## Intent

The lookahead limiter shipped (F1) does its job on *sample* peaks, but three
gaps remain, all known and deliberately deferred:

1. **No true-peak detection.** The VU "true peak" (`audio/mod.rs:1156`) is a
   sample peak with a slow decay — structurally blind to inter-sample overs,
   so a mastered-hot rig can clip the converter between samples while the
   meter reads safe.
2. **No gain-reduction readout.** The `LIM` badge is binary (`limit_indicator`,
   set when pre-limiter signal exceeds 0.95, fades over ~0.4 s). It says
   limiting *happened*, never *how much*. The actual GR (`Limiter::gain`,
   stereo-linked, one value for both channels) never leaves the audio thread.
3. **The limiter's own corner runs at base rate.** The safety clamp
   (`yl.clamp(...)`) is a memoryless waveshaper: inter-sample overs that the
   base-rate detector misses get base-rate-clipped — the exact defect E1/E2
   removed from the pedals with `Oversampler4/8`.

Success = the rig reports honest peaks and GR in dB, inter-sample overs are
turned down instead of clipped, and the fidelity baseline moves only where
E1/E2 moved it (aliasing-sensitive top bands) before being re-blessed.

## Goals / non-goals

**Goals**
- 4× true-peak estimation on the final output (the converter guard), reusing
  the existing `Oversampler4` prototype (`src/dsp/oversample.rs`, M=12 Kaiser).
- Detector that *sees* inter-sample peaks: the envelope follower runs off the
  4× peak, so overs are prevented, not just displayed.
- Oversampled safety clamp: the gained signal is clamped at 4× and decimated,
  so the ceiling holds between samples too.
- A stereo-linked GR readout in dB on the OUT meter row, next to the existing
  `LIM` badge (no new layout, snapshot-tested like the badge).
- Latency accounting stays exact: `Limiter::latency()` grows by the oversampler
  group delay (~12 base-rate samples total for up+down at 4×), and the export
  onset test (`export.rs:525`) is updated to match.

**Non-goals**
- No detector ballistics redesign (1.5 ms lookahead, 0.15 ms attack,
  25 ms release, 40 ms peak-hold, −1 dBFS ceiling, +1.5 dB makeup all stay).
- No per-channel GR (stereo-linked is the design; a panned peak must not move
  the image — already tested).
- No true-peak *limiting* standard beyond 4× (no ITU-R BS.1770 meter; the
  harness's LUFS metrics already cover loudness).
- No changes to any pedal, amp, cab, or the monitor-bus topology.

**Anti-goals**
- Anything that allocates, blocks, or locks on the audio thread. The readout
  crosses as one relaxed `AtomicF32` store per callback — the same discipline
  as `Levels::input/output/output_peak/limiting`.
- A readout that lies: the dB number must equal the actual applied gain
  (min gain over the block), not a smoothed estimate of it.

## Constraints

- Knob values 0.0–1.0; not applicable here (no new knobs — the limiter has no
  user controls and gains none).
- `cargo fmt --check`, `clippy --all-targets --all-features -D warnings`,
  `cargo test --release` + debug, fidelity `--check` — the last is expected to
  move (top bands only) and be **re-blessed** per CONTRIBUTING, like E1/E2.
- No new dependencies (reuses `Oversampler4`).
- The `clamp_hits` invariant stays: on ordinary material the oversampled clamp
  should still never bite steady-state (it catches only inter-sample overs the
  envelope already handled).

## Design

### 1. Detector sees inter-sample peaks

Per sample, in `Limiter::process`: upsample the makeup'd detector input
(`li`, `ri`) to 4× with the shared interpolator, take the max abs over the 4
phases (stereo max across channels — still one linked value), and feed the
*existing* peak-hold envelope follower with that 4× peak instead of the
base-rate sample peak. Nothing else in the detector changes (attack/release/
margin/ceiling constants untouched). Detector-correcting (not
measurement-only): overs are prevented, not just displayed.

Cost: interpolation per sample per channel (~M·N = 48 MACs per detector
stream). Instance layout (my call): 4 `Oversampler4` fields per `Limiter` —
detector-L/R upsample-only (histories cannot interleave across the
detector/corner or L/R roles) plus corner-L/R via the canonical
`process(x, clamp)` loop; prototypes are small and preallocated in `new()`.
No allocation, no locking.

### 2. Oversampled corner

The safety clamp moves into the 4× domain: the gained, delayed signal is
upsampled, clamped to `±ceiling` per high-rate sample, and decimated back
(the canonical `Oversampler4::process(x, |u| u.clamp(...))` loop the drive
pedals share). The detector's lookahead delay line grows by the oversampler
round-trip group delay so the gain still lands on the peak it was computed
for — `latency()` reports lookahead + oversampler delay, and
`export.rs:525`'s onset accounting follows it. The exact integer (expect 12
base-rate samples round-trip; my call) is measured like
`oversample.rs::roundtrip_is_unity_in_passband`, hard-coded, and asserted by
`latency()` plus the export onset test. Decimation can ring microscopically
over ceiling at base rate: exactness is asserted at high rate, base rate keeps
an E1-style epsilon (`CEILING + 1e-4`-class), documented. `clamp_hits` counts
one hit per base-rate sample where any high-rate phase clamped (my call).

Which limiters: both — master-bus (`DspChain::master_limiter`) and final
(`audio::final_limiter`) share the struct, so both gain the corner. The
final one is where true-peak matters (converter guard); the master one is
where it is heard.

### 3. GR readout (dB, audio-thread-safe)

- `Limiter` tracks the minimum `gain` applied since the last read
  (`block_min_gain`, reset on read — a plain `f32` field, audio thread only).
- `DspChain::process_block` returns it (or the audio callback reads it off
  the chain's master limiter after the block — whichever the plan finds
  cleaner; the chain owns the limiter).
- The audio callback stores `20·log10(min_gain)` (0.0 dB when open) into a
  new `Levels::gr_db: Arc<AtomicF32>` with a single relaxed store per
  callback, next to the existing `output_peak`/`limiting` stores
  (`audio/mod.rs:959-964`).
- UI: the OUT meter row shows the dB value beside the `LIM` badge
  (`render_vu_row_detail`, `draw.rs:432`); badge semantics unchanged (final
  stage, binary). Snapshot tests cover the row with and without reduction.
  Fixed-width field (`0.0` when open), bar shrinks accordingly (my call).

Edge cases: silence → gain stays 1.0 → readout 0.0 dB (never negative, never
NaN — `log10(1.0) = 0`, gain is clamped to (0, 1] by construction).
`reset()` also resets `block_min_gain` to 1.0.

## Acceptance

- Unit: true-peak catches a constructed inter-sample over that the sample
  peak misses (two base-rate samples at ±0.9 with a 4× peak above ceiling —
  assert detector reduces gain where the old detector did not); GR readout
  equals applied gain on a driven block (0.0 dB on silence/moderate material);
  oversampled clamp holds the ceiling on the E1-style square-at-4× probe;
  `latency()` equals lookahead + oversampler delay (assert against the
  measured onset in the export test).
- Harness `--check` moves only in aliasing-sensitive top bands (E1/E2
  signature), then the baseline is re-blessed; LUFS-I must not move (no level
  change — makeup untouched).
- Bench: `process_block` delta recorded; 4× detector + corner on two limiters
  is the cost to report.
- Snapshot: OUT row with `LIM` + dB readout renders (new snapshot, inspected).

## Decisions (locked by the human, 2026-10-11)

1. **True-peak display: separate upsampler (a).** An upsample-only 4× stream
   over the final post-ceiling output in the audio loop; per-block max into
   the existing ~1.5 s `out_peak` decay. Decoupled from the limiter
   internals; the tick occasionally holds slightly higher on attacks.
2. **GR source: live-master limiter (a).** Matches the meter row's existing
   live-centric semantics (OUT bars follow live playing; take playback moves
   only the peak tick; the badge fires on live-driven overloads) — the number
   can never contradict what the row shows in practice.
3. **Re-bless per workstream (now).** F1 gets its own re-bless; CI stays
   green with a reviewable diff each time (E1/E2 precedent).
