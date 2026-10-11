# DSP perf batch (Item 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove per-sample transcendentals and IR-build waste with zero audible change.

**Architecture:** Cache knob-rate values on change, move vibe/phaser coefficients to control-rate with interpolation, collapse pitch windows, single-powf compressor, oscillator recurrence for IR modes.

**Tech Stack:** Rust, no new dependencies; `examples/bench.rs` + fidelity harness for verification.

**Spec:** `docs/superpowers/specs/2026-10-11-dsp-perf-batch-design.md`

## Global Constraints

- No heap allocation, blocking, or locking on the audio thread (AGENTS.md DSP constraints).
- Knob values are 0.0–1.0 internally.
- Tone-neutral: fidelity `--check` must pass with zero baseline edits; if it moves, the change is wrong.
- Gates before push: `cargo fmt --check`; `cargo clippy --all-targets --all-features -- -D warnings`; `cargo test --release`; `cargo test`; `cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml`.
- Work directly on `main`; never push unprompted.
- Measure with release builds only (`cargo run --release --example bench`).

## Review Focus

- Stale cache after a knob move (threshold/ratio/rate): expect the effect to track the knob within one block.
- Control-rate stepping/zipper in vibe/phaser sweep: expect no audible stepping; existing throb/sweep tests plus harness still pass.
- Compressor knee behavior at threshold boundary: expect continuous gain (no step at `over = 0`).
- IR predelay boundary: expect zero modal contribution before `predelay`, first sample `sin(0) = 0`.
- Ragged external IRs: expect no panic on any length mix (existing test).

---

### Task 1: NoiseGate threshold cache

**Files:**
- Modify: `src/dsp/effects/noise_gate.rs:67-77`
- Test: `src/dsp/effects/noise_gate.rs` tests module

**Interfaces:**
- Consumes: `param_changed`, `db_to_lin` from `super` (exist).
- Produces: `NoiseGate::process(sample, threshold, release)` signature unchanged; adds private `last_threshold: f32`, `threshold_lin: f32` fields.

- [ ] **Step 1: Write the failing test** — threshold change takes effect mid-stream: drive a loud tone, flip threshold from pass-all to gate-all, assert output gates within one block; flip back, assert it passes again.

```rust
#[test]
fn threshold_change_takes_effect_without_stale_cache() {
    let mut ng = NoiseGate::new(SR);
    // loud tone passes at threshold 0.0 ...
    // ... then gates at threshold 1.0 within 24000 samples
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib dsp::effects::noise_gate 2>&1 | tail -5`
Expected: FAIL (no `last_threshold` field / test not yet meaningful — currently recomputes every sample so it passes behaviorally; the test pins the invalidation contract for the cached version).

- [ ] **Step 3: Implement cache in `src/dsp/effects/noise_gate.rs`** — add `last_threshold: f32` (init `-1.0`), `threshold_lin: f32` fields; at top of `process`, `if param_changed(threshold, self.last_threshold)` recompute `threshold_lin = db_to_lin((threshold - 1.0) * 80.0)` and store.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib dsp::effects::noise_gate 2>&1 | tail -5`
Expected: PASS, all existing gate tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/dsp/effects/noise_gate.rs
git commit -m "perf(gate): cache threshold_lin on knob change"
```

### Task 2: Pitch ratio cache + window fold

**Files:**
- Modify: `src/dsp/effects/pitch.rs:72-106`
- Test: `src/dsp/effects/pitch.rs` tests module

**Interfaces:**
- Consumes: `param_changed` (exists).
- Produces: `Pitch::process(x, pitch, mix, tone)` signature unchanged; adds private `last_pitch: f32`, `ratio: f32` fields.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn pitch_change_takes_effect_without_stale_cache() {
    // unison passes ~dry at mix=1 within tolerance; octave-up shifts up (zero-crossing rate rises)
}

#[test]
fn grain_windows_sum_to_unity() {
    // for a sweep of phases: win_a + win_b == 1 within 1e-6 using the folded form
}
```

- [ ] **Step 2: Run tests to verify behavior baseline**

Run: `cargo test --lib dsp::effects::pitch 2>&1 | tail -8`
Expected: FAIL on the new names (no cached fields yet) or PASS on unity (documents current 2-cos form); the fold test pins the identity the refactor relies on.

- [ ] **Step 3: Implement in `src/dsp/effects/pitch.rs`** — cache `ratio` via `param_changed(pitch, self.last_pitch)` (init `last_pitch = -1.0`); replace `win_b` computation with `1.0 - win_a` (keep single `cos` for `win_a`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib dsp::effects::pitch 2>&1 | tail -8`
Expected: PASS, all existing pitch tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/dsp/effects/pitch.rs
git commit -m "perf(pitch): cache ratio, fold grain window to one cos"
```

### Task 3: UniVibe + Phaser rate caches (control-rate REVERTED — see ruling)

**Outcome (2026-10-11):** control-rate + interpolated sweep coefficients were
implemented, measured, and REVERTED. vena: at 32/16/8 the vibe moved one
harness LTAS bin +0.28/+0.59 dB (non-monotonic — alignment luck), endpoint
sampling + frac fixes did not save it (+0.50 dB), because sweep turnarounds
give any coefficient error unbounded leverage over notch-crossing times.
Rule: sweeping-notch stages keep exact per-sample coefficients; only exact
knob-rate caches ship. The phaser never runs wet in any bundled preset, so
the harness cannot validate approximations there at all.

**Files:**
- Modify: `src/dsp/effects/uni_vibe.rs`, `src/dsp/effects/phaser.rs`
- Test: existing `uni_vibe`/`phaser` tests modules (rate-tracking + cached-rate
  tests stay; they pin the exact caches).

**Files:**
- Modify: `src/dsp/effects/uni_vibe.rs:62-84`, `src/dsp/effects/phaser.rs:96-154`
- Test: existing `uni_vibe`/`phaser` tests modules

**Interfaces:**
- Consumes: nothing new.
- Produces: `process` signatures unchanged; per-object cached `last_rate: f32`, `rate_hz: f32`, control counter + interpolated `a` values (private fields; exact names up to implementer).

- [ ] **Step 1: Write the failing tests** — knob tracking at control rate:

```rust
#[test]
fn rate_change_takes_effect_promptly() {
    // slow LFO at rate 0, fast at rate 1: envelope modulation rate follows within 1s
}
```

(Add one per module; reuse existing `window_peaks` helpers.)

- [ ] **Step 2: Run tests to verify baseline**

Run: `cargo test --lib dsp::effects::uni_vibe dsp::effects::phaser 2>&1 | tail -8`
Expected: new tests FAIL (no cached fields) or PASS against per-sample baseline — pins the contract before the refactor.

- [ ] **Step 3: Implement `UniVibe`** — cache `rate_hz` on `param_changed(rate, ...)`; advance `self.phase` per sample (one add); recompute the four stage `a` coefficients every 32 samples with linear interpolation between updates.

- [ ] **Step 4: Implement `Phaser`** — same: cache `rate_hz`; per-sample phase advance; `a_l`/`a_r` at control rate (32) with interpolation. Keep Phase-90 mono collapse and feedback logic untouched.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib dsp::effects::uni_vibe dsp::effects::phaser 2>&1 | tail -8`
Expected: PASS including `wet_output_throbs_over_time`, `deeper_depth_sweeps_further`, phaser notch tests.

- [ ] **Step 6: Fidelity spot-check**

Run: `cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml 2>&1 | tail -5`
Expected: PASS, no baseline edits.

- [ ] **Step 7: Commit**

```bash
git add src/dsp/effects/uni_vibe.rs src/dsp/effects/phaser.rs
git commit -m "perf(mod): control-rate vibe/phaser coefficients"
```

### Task 4: Compressor single-powf gain computer

**Files:**
- Modify: `src/dsp/effects/compressor.rs:50-116`
- Test: `src/dsp/effects/compressor.rs` tests module

**Interfaces:**
- Consumes: `db_to_lin` (cached form), existing `set_sustain`/`set_attack`.
- Produces: `process` signature unchanged; `set_sustain` additionally caches `thresh_lin: f32`, `knee_k: f32` (`k = 1 - 1/ratio`).

- [ ] **Step 1: Write the failing test** — knee continuity + equivalence:

```rust
#[test]
fn gain_is_continuous_through_the_knee() {
    // sweep env across thresh_db: target gain has no step at over = 0
}

#[test]
fn single_powf_matches_db_form() {
    // for a grid of env/thresh/ratio: (thresh_lin/env).powf(k) == db_to_lin(-over*k) within 1e-5
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib dsp::effects::compressor 2>&1 | tail -8`
Expected: FAIL (no `thresh_lin`/`knee_k` fields yet).

- [ ] **Step 3: Implement** — in `set_sustain`, cache `thresh_lin = db_to_lin(thresh_db)`, `knee_k = 1 - 1/ratio`; in `process`, replace `lin_to_db`+`db_to_lin` with `if over > 0 { (thresh_lin / env.max(1e-6)).powf(knee_k) } else { 1.0 }` where `over` still gates the branch (keep one `lin_to_db`? No — compute `over` from linear compare: `env > thresh_lin`). Use pure-linear branch to drop the `log10` entirely.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib dsp::effects::compressor 2>&1 | tail -8`
Expected: PASS including `reduces_dynamic_range_and_is_finite`, `zero_sustain_is_level_transparent`.

- [ ] **Step 5: Commit**

```bash
git add src/dsp/effects/compressor.rs
git commit -m "perf(comp): single-powf gain computer"
```

### Task 5: IR modal recurrence

**Files:**
- Modify: `src/dsp/cab/ir.rs:134-146`
- Test: `src/dsp/cab/ir.rs` tests + `recurrence_tests` + per-cab `assert_modes_realized` suites

**Interfaces:**
- Consumes: `Texture`, `ir_len` (unchanged).
- Produces: `synth()` signature and output unchanged (bit-close, not bit-exact: worst drift < 2e-4 per existing bound).

- [ ] **Step 1: Write the failing test** — synth integration bound:

```rust
#[test]
fn synth_with_recurrence_matches_direct_form() {
    // render the 110 Hz / 90 ms texture from the existing test via synth(),
    // compare against a direct-form reference loop: max tap diff < 2e-4
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib dsp::cab::ir 2>&1 | tail -8`
Expected: FAIL (synth still uses direct form; reference comparison scaffolding missing) — or PASS trivially if written against direct; the point is the test exists before the swap.

- [ ] **Step 3: Implement recurrence in `synth()`** — per mode: `tau`, `w`, `decay`, `(rc, rs)`, state `(c,s)=(1,0)`; iterate `n` in `0..len`, `nn = n.saturating_sub(predelay)`, skip accumulation when `n < predelay`, add `g * s` then step `(c,s)`. Keep fade + DC-removal untouched.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib dsp::cab 2>&1 | tail -8`
Expected: PASS — `synth_realizes_a_mode_at_its_frequency_and_t60`, comb test, DC test, all per-cab `assert_modes_realized`, plus the new bound test.

- [ ] **Step 5: Measure**

Run: `cargo run --release --example bench 2>&1 | tail -8`
Expected: `CabBank::new` ms materially down vs. pre-change baseline (record both in ledger).

- [ ] **Step 6: Commit**

```bash
git add src/dsp/cab/ir.rs
git commit -m "perf(cab): modal recurrence in IR synth"
```

### Task 6: Mic-build decision + ragged-bound verification

**Files:**
- Verify: `src/dsp/cab/mod.rs:696-776`, test at `mod.rs:1108`
- Modify (only if bench says so): per-cab `new()` (`mesa.rs:163`, siblings) + `BlendedCab` lazy ribbon/room
- Test: `src/dsp/cab/mod.rs` tests module

**Interfaces:**
- Consumes: Task 5's faster `synth`.
- Produces: boot tone identical (`DEFAULT_MIC_BLEND/ROOM = 0.0` unchanged); `CabBank::new` faster if lazy taken.

- [ ] **Step 1: Verify ragged bound (no code)**

Run: `cargo test --release --lib dsp::cab::tests::ragged_mic_irs_do_not_panic 2>&1 | tail -5`
Expected: PASS — `MicBlend::new` already truncates to shortest; close the sub-item with a ledger note.

- [ ] **Step 2: Measure post-Task-5 construction**

Run: `cargo run --release --example bench 2>&1 | tail -8`
Expected: record `CabBank::new` ms in ledger. If under ~120 ms, skip lazy-build (ledger the ruling) and go to Step 4.

- [ ] **Step 3 (conditional): Lazy ribbon/room** — build close pair eagerly per cab; synthesize ribbon/room on first `set()` needing them (`blend > 0.001 || room > 0.001`). TDD: test that `recombine(0,0)` output equals close-only mix and that moving blend to 1 produces the same taps as eager (compare against eager-built reference in-test). Keep `recombine` math untouched.

- [ ] **Step 4: Full gates + fidelity**

Run: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --release 2>&1 | tail -5`
Run: `cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml 2>&1 | tail -5`
Expected: all green, no baseline edits.

- [ ] **Step 5: Commit (only if code changed)**

```bash
git add -A
git commit -m "perf(cab): verify ragged bound; lazy mic builds"
```
