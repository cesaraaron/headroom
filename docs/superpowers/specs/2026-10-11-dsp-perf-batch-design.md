# DSP perf batch (Item 2) — design

**Status:** written for review; implements roadmap-next.md "Item 2 — DSP perf batch (tone-neutral, measurable)".
**Scope:** E3 per-sample transcendentals + D7 IR-build `exp`/`sin` + ragged-IR verification + mic-build decision. Palette refactor explicitly out.

## Intent

The audio callback has a 10 ms budget per 480-frame block at 48 kHz
(`examples/bench.rs`). Several stages burn per-sample transcendentals for
values that move at knob/LFO rate (≤8 Hz), and cab construction (~221 ms
measured) burns ~10.5 M `exp`/`sin` pairs in `src/dsp/cab/ir.rs:138-146`.
Remove the waste with zero audible change.

Success = `bench` µs/block and `CabBank::new` ms both drop, and the
fidelity harness reports no LUFS/LTAS movement
(`cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml`).

## Goals / non-goals

**Goals**
- Cache knob-rate values (`NoiseGate` threshold, `Pitch` ratio, vibe/phaser
  rate maps) so they recompute only on knob change (`param_changed` pattern,
  cf. `compressor.rs:86-90`).
- Collapse the pitch grain windows from 2 `cos` to 1 (`win_b = 1 - win_a`,
  since `phase_b = phase + 0.5` ⇒ `cos(θ+π) = -cosθ`).
- Move vibe/phaser all-pass coefficients off the per-sample path (control-rate
  update + interpolation; LFO phase advance itself stays per-sample — one add).
- Reduce the compressor gain computer from `log10`+`powf` to a single `powf`
  via `(thresh_lin/env)^k`, with `k`/`thresh_lin` cached on sustain change.
- Replace the IR modal loop with the complex-oscillator recurrence already
  proven in `ir.rs:466-527` (worst drift < 2e-4).
- Verify the ragged-IR bound (believed already fixed by
  `MicBlend::new` truncating all six to the shortest, `cab/mod.rs:712-728`);
  close with a test run, no code if green.
- Decide the mic-build question without moving boot tone: keep
  `DEFAULT_MIC_BLEND/ROOM = 0.0`, defer ribbon/room synthesis until first use
  **only if** recurrence alone leaves `CabBank::new` slow; otherwise keep
  eager builds.

**Non-goals**
- No tone changes: no filter redesign, no default-value changes, no baseline
  re-bless. If the harness moves, the change is wrong.
- No palette/theming work (separate batch).
- No tone-batch items (F1/B2/B4/B5/C2–C5).
- No CI threshold for bench (machine-dependent, per `4.2`).

**Anti-goals**
- Anything that allocates, blocks, or locks on the audio thread (AGENTS.md
  DSP constraints). Caches are plain `f32` fields written on the audio thread
  from knob values already there — no sharing, no locks.
- Bit-exactness theatre: control-rate/interpolated coefficients are
  intentionally *close*, not identical. The harness (LUFS/LTAS), not sample
  diffs, is the acceptance.

## Constraints

- Knob values are 0.0–1.0 internally; conversion at UI layer.
- Biquad state preserved across buffers; do not reset sag/filter state.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --release` + debug, fidelity `--check` — all green before push.
- Work directly on `main`; never push unprompted (human A/Bs then says the word).
- Measure before/after with `cargo run --release --example bench` (release only;
  debug numbers are noise).

## Design (decisions locked)

1. `noise_gate.rs:77` — cache `threshold_lin` + last threshold; recompute only
   when `param_changed`. Cost if wrong: stale threshold gates wrong — covered by
   a threshold-sweep behavior test.
2. `pitch.rs:84` — cache `ratio` + last pitch; windows folded to one `cos`.
   The remaining `cos` stays per-sample in this batch (recurrence/table is a
   follow-up if bench says the stage still matters).
3. `uni_vibe.rs` / `phaser.rs` — cache `rate_hz` on knob change; recompute the
   all-pass `a` coefficients at control rate (every 32 samples) with linear
   interpolation between updates. LFO phase still advances per sample.
4. `compressor.rs:99-104` — single-`powf` form `target = (thresh_lin/env)^k`
   for `over > 0`, `1.0` otherwise; `thresh_lin`/`k` cached in `set_sustain`.
   Algebra: `over = 20log10(env) - thresh`, `target = 10^(-over·k/20)` =
   `env^-k · 10^(thresh·k/20)` = `(thresh_lin/env)^k`.
5. `ir.rs:138-146` — complex-oscillator recurrence per mode:
   `decay = exp(-1/tau)`, `rc = decay·cos(w)`, `rs = decay·sin(w)`,
   state `(c,s)` from `(1,0)`, samples `s` after predelay, skipped before.
   Predelay boundary: zero contribution before `predelay`, first sample
   `sin(0) = 0`.
6. Mic builds — recurrence first, measure, then decide lazy. Lazy (if needed):
   build close pair eagerly, ribbon/room on first `set(blend,room)` that needs
   them; `recombine` unchanged. Boot tone identical because defaults need only
   close.

## Left to the builder

- Exact control-rate interval (32 is the proposal; 16–64 acceptable if harness
  stays still and bench improves).
- Whether the compressor single-`powf` is enough or a fast approximation pays
  (only with harness-still proof).
- Exact lazy-build plumbing if Task 6 needs it (Option<Vec> vs. build-once flags).

## Findings during execution (2026-10-11, Task 3)

- **Turnaround leverage (rule):** sweeping-notch stages (vibe, phaser) must
  keep exact per-sample coefficients. Any coefficient error, however small,
  levers through `1/a'(t)` into notch-crossing time shifts — unbounded at
  sweep turnarounds — which move narrow LTAS bins unpredictably (measured
  +0.28/+0.59/+0.50 dB at 111 Hz for 32/8/endpoint-32). Only exact knob-rate
  caches ship on these stages. The phaser has zero wet-path coverage in the
  bundled presets, so approximations there are unprovable — same rule.
- **Harness gap (reported, not fixed here):** the fidelity `--check` compares
  with `>` tolerances, so a fully-NaN render passes silently (observed when a
  `0/0` refresh interval slipped into a diagnostic build). Recommend an
  `is_finite` guard as separate Track-4 work.

## Acceptance

- `bench`: process_block µs/block down or flat (never up); `CabBank::new` ms
  down materially after Task 5 (+ Task 6 if taken).
- Fidelity `--check` passes with zero baseline edits.
- Full gates green. No new allocations on the audio thread (`Vec` only in
  constructors / offline synth).
