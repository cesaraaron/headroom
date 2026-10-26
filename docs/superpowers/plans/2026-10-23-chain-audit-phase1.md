# Signal-Chain Audit Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the five CRITICAL findings from `docs/chain-audit-2026-10.md` (1.1, 1.2, 3.1+3.2, 4.1+4.2, 5.1) and stop there.

**Architecture:** Six TDD tasks, each independently testable: calibration target, looper monitor routing, the NFB divider/loop closure, wiring the nine amp models to it, the cab drive/mic-position state, and the region-export input window. A final task records the tone drift and runs the gate. No DSP code is restructured beyond the interfaces named here.

**Tech Stack:** Rust 2024, `cargo test --release --lib`, `cargo clippy --all-targets --all-features -- -D warnings`, the offline fidelity harness.

**Spec:** `docs/chain-audit-2026-10.md` (findings 1.1, 1.2, 3.1, 3.2, 4.1, 4.2, 5.1 and "Suggested Fix Phasing" Phase 1). Design decisions approved by the maintainer on 2026-10-23:

- Looper playback is re-amped through `take_chain` (raw-DI loop, tone follows the rig), not captured post-rig.
- `REFERENCE_VERSION` goes 2 → 3 with the P90 target correction.
- The NFB loop closes inside the 8× power stage; the baseline may be re-blessed in the final task.

## Global Constraints

- No heap allocation, locking, or blocking on the audio thread (`AGENTS.md`).
- Biquad state is preserved across buffer boundaries; no state resets except where this plan says so.
- Knob values are normalized 0.0–1.0 internally.
- Rectifier-sag envelopes are *not* reset.
- Every routed/topology commit is logged in `docs/fidelity-implement.md`'s increment log.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --release --lib` stay green after every task.
- Spec reference values: P90 target `-22.0 dBFS`; `POWER_OS_RATE = 8.0`; `DRIVE_OS_RATE = 4.0`; export window clamp removed from the input mix only.

## Review Focus

- **Non-48 kHz devices.** The divider filters move to 8×sr and the cone filter to 4×sr; at 44.1/96 kHz a wrong rate scaling is a silent cutoff shift or a blow-up. Pinned by `nfb_is_stable_at_44k1_and_96k` (Task 4) and the 4×-clocked cone test (Task 5).
- **Presence at its extremes.** The corrected bleed inverts sign semantics for the pot; 0.0 and 1.0 must stay finite and monotonic. Pinned by `presence_bleeds_the_top_out_of_the_divider` (Task 3).
- **Region exports starting at frame 0 or wholly after the last clip.** Removing the clamp must not read samples outside a clip's span or write outside the window. Pinned by `range_export_writes_only_the_window` (existing) and `region_export_feeds_pre_window_audio_to_the_chain` (Task 6).
- **Looping with no takes / an empty loop.** The moved looper sample must not leak onto the live bus or double-count. Pinned by `looper_playback_rides_the_take_bus_not_the_live_bus` (Task 2).
- **A nominal P90 at the trim clamp.** The corrected target must produce a sane trim, not the +24 dB clamp. Pinned by `p90_calibration_does_not_hit_the_trim_clamp` (Task 1).

---

### Task 0: Commit the spec and the plan

**Files:**
- Add: `docs/chain-audit-2026-10.md` (untracked; copied from the main checkout)
- Add: `docs/superpowers/plans/2026-10-23-chain-audit-phase1.md`

- [ ] **Step 1: Commit both documents**

```bash
git add docs/chain-audit-2026-10.md docs/superpowers/plans/2026-10-23-chain-audit-phase1.md
git commit -m "docs: add the 2026-10 signal-chain audit and its phase-1 plan"
```

Expected: one commit, working tree clean.

---

### Task 1: P90 calibration target and reference version

**Files:**
- Modify: `src/audio/calibration.rs:124-150`
- Modify: `docs/fidelity-plan.md:411-420`, `docs/fidelity-plan.md:658`
- Test: `src/audio/calibration.rs` tests module

**Interfaces:**
- Produces: `target_peak_dbfs(PickupClass::P90) -> -22.0`; `REFERENCE_VERSION = 3`. No signature changes.

- [ ] **Step 1: Write the failing tests**

In the `src/audio/calibration.rs` tests module add:

```rust
/// The pre-B8 placeholder (-4.5 dBFS) was 15.2 dB hotter than a humbucker, so
/// every P90 calibration clamped its trim at +24 dB. A P90 sits between the
/// measured single-coil and humbucker targets.
#[test]
fn p90_target_sits_between_single_coil_and_humbucker() {
    let sc = target_peak_dbfs(PickupClass::SingleCoil);
    let p90 = target_peak_dbfs(PickupClass::P90);
    let hb = target_peak_dbfs(PickupClass::Humbucker);
    assert!(sc < p90 && p90 < hb, "single {sc}, P90 {p90}, humbucker {hb}");
    assert!(p90 > sc + 1.0, "P90 must not collapse onto the single-coil target");
    assert!(p90 < hb - 1.0, "P90 must not collapse onto the humbucker target");
}

/// A quiet performance on a P90 must not pin the trim at the +24 dB clamp.
/// At `measured = -30 dBFS` the old `-4.5` target asked for +25.5 dB and clamped.
#[test]
fn p90_calibration_does_not_hit_the_trim_clamp() {
    let play = windows(db_to_lin(-30.0), 250);
    let noise = windows(db_to_lin(-80.0), 200);
    let r = compute_calibration(&play, &noise, PickupClass::P90, false, false).unwrap();
    assert!(r.trim_db < 24.0, "trim clamped: {}", r.trim_db);
}
```

This uses the existing `windows()` / `db_to_lin()` test helpers, exactly as
`pickup_targets_shift_the_trim` does.

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test --release --lib audio::calibration::tests::p90 -- --nocapture`

Expected: `p90_target_sits_between_single_coil_and_humbucker` FAILS on the
`-4.5` target; the clamp test FAILS because `trim_db` is 24.0.

- [ ] **Step 3: Implement**

In `src/audio/calibration.rs`:

```rust
pub const REFERENCE_VERSION: u32 = 3;
```

Update the doc comment above it to `Version 3 = the P90 target corrected to an
interpolated provisional value; the measured targets are unchanged.`

```rust
pub fn target_peak_dbfs(p: PickupClass) -> f32 {
    match p {
        PickupClass::SingleCoil => -24.6,
        // Provisional and *interpolated*, not measured: a P90 sits between a
        // single-coil and a humbucker. The pre-B8 placeholder (-4.5) was 15 dB
        // above a humbucker and clamped every P90 calibration's trim.
        PickupClass::P90 => -22.0,
        PickupClass::Humbucker => -19.7,
    }
}
```

In `docs/fidelity-plan.md`, change the table row to
`| `P90` | −22.0 dBFS (provisional, interpolated) |`, reword the sentence after
the table to say P90 uses an interpolated provisional target until a P90 guitar
is measured, and update the stale sketch at line ~658 to
`// −24.6 / −22.0 / −19.7`.

- [ ] **Step 4: Run the tests and the calibration suite**

Run: `cargo test --release --lib audio::calibration`

Expected: PASS, including the existing `pickup_targets_shift_the_trim`.

- [ ] **Step 5: Commit**

```bash
git add src/audio/calibration.rs docs/fidelity-plan.md
git commit -m "fix(calibration): put the provisional P90 target between SC and HB"
```

---

### Task 2: Route looper playback through the take bus

**Files:**
- Modify: `src/audio/mod.rs:1141-1151` (per-frame monitor mix)
- Test: `src/audio/mod.rs` tests module

**Interfaces:**
- Produces: `fn monitor_frame(live_l: f32, live_r: f32, import_l: f32, import_r: f32, click: f32, take: f32, loop_sample: f32) -> (f32, f32, f32)` returning `(out_left, out_right, take_in)`.

- [ ] **Step 1: Write the failing test**

In the `src/audio/mod.rs` tests module, extend the `use super::...` list with
`monitor_frame`, then add:

```rust
/// The looper records the post-trim dry DI, so its playback is raw take audio:
/// it must be re-amped by `take_chain` instead of summed raw onto the live bus.
#[test]
fn looper_playback_rides_the_take_bus_not_the_live_bus() {
    let (l, r, take) = monitor_frame(0.10, 0.20, 0.01, 0.02, 0.03, 0.40, 0.25);
    assert!((l - 0.14).abs() < 1e-6, "live L must not carry the loop: {l}");
    assert!((r - 0.25).abs() < 1e-6, "live R must not carry the loop: {r}");
    assert!((take - 0.65).abs() < 1e-6, "loop must ride the take bus: {take}");
}

/// An idle/empty loop contributes silence to the take bus.
#[test]
fn idle_looper_adds_nothing_to_either_bus() {
    let (l, r, take) = monitor_frame(0.3, 0.4, 0.0, 0.0, 0.0, 0.0, 0.0);
    assert_eq!((l, r, take), (0.3, 0.4, 0.0));
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test --release --lib audio::tests::looper_playback -- --nocapture`

Expected: FAIL to compile — `monitor_frame` not found.

- [ ] **Step 3: Implement the helper and use it**

Add near `output_stage` in `src/audio/mod.rs`:

```rust
/// Split one frame's monitor sources between the two monitor buses.
///
/// The looper records the post-trim dry DI, so its playback is *take* audio:
/// it belongs on the take bus, where `take_chain` re-amps it with the current
/// rig. The click and the import bus are finished signals and stay live.
#[inline]
fn monitor_frame(
    live_l: f32,
    live_r: f32,
    import_l: f32,
    import_r: f32,
    click: f32,
    take: f32,
    loop_sample: f32,
) -> (f32, f32, f32) {
    (
        live_l + import_l + click,
        live_r + import_r + click,
        take + loop_sample,
    )
}
```

Replace the per-frame mix:

```rust
let loop_sample = self.looper.process(sample);
let (out_left, out_right, take_in) = monitor_frame(
    self.out_l[i],
    self.out_r[i],
    frame.import_l,
    frame.import_r,
    click,
    frame.take,
    loop_sample,
);
self.take_in[i] = take_in;
self.out_l[i] = out_left;
self.out_r[i] = out_right;
```

Update the comment above it: the looper is now monitor-only on the take bus,
never captured in a take and never summed raw onto the live bus.

- [ ] **Step 4: Run the tests**

Run: `cargo test --release --lib audio::tests`

Expected: PASS, including `monitor_buses_are_added_after_the_master...`.

- [ ] **Step 5: Commit**

```rust
git add src/audio/mod.rs
git commit -m "fix(looper): re-amp loop playback through the take bus"
```

---

### Task 3: Correct and close the global-NFB divider

**Files:**
- Modify: `src/dsp/amp/mod.rs:631-665` (`GlobalNfb`), `src/dsp/amp/mod.rs:916-937` (`PowerOs`)
- Test: `src/dsp/amp/mod.rs` tests module

**Interfaces:**
- Produces: `pub(crate) const POWER_OS_RATE: f32 = 8.0`; `GlobalNfb::new(sr: f32, beta: f32, lf_corner: f32)` now designs its filters at `sr * POWER_OS_RATE`; `PowerOs::shape_closed_loop(&mut self, x: f32, nfb: &mut GlobalNfb, presence: f32, pf_out: &mut f32, f: impl FnMut(f32) -> f32) -> f32`.
- `PowerOs::shape` is unchanged and remains the open-loop reference.

- [ ] **Step 1: Write the failing tests**

Add to the `src/dsp/amp/mod.rs` tests module:

```rust
/// Presence must bleed the *top* out of the divider. The old code subtracted
/// the highpass's complement (`shaped - HP(shaped)`), which bled the midrange,
/// so above 4 kHz the pot did nothing.
#[test]
fn presence_bleeds_the_top_out_of_the_divider() {
    let sr = 48_000.0;
    let rms = |presence: f32| {
        let mut nfb = GlobalNfb::new(sr, 0.5, 220.0);
        let mut e = 0.0f64;
        let n = 24_000usize;
        for i in 0..n {
            let x = (2.0 * PI * 6_000.0 * i as f32 / sr).sin();
            let y = nfb.feedback(x, presence);
            if i >= n / 2 {
                e += f64::from(y * y);
            }
        }
        (e / (n / 2) as f64).sqrt() as f32
    };
    let dark = rms(0.0);
    let neutral = rms(0.5);
    let bright = rms(1.0);
    assert!(dark > neutral * 1.1, "presence 0 must feed the top back in: {dark} vs {neutral}");
    assert!(bright < neutral * 0.9, "presence 1 must bleed the top out: {bright} vs {neutral}");
}

/// The loop's only delay must be one high-rate sample. The old tap was 12
/// base-rate samples, which put a notch in the closed-loop response at 4 kHz
/// (and a boost at 2 kHz) — a comb filter, not a feedback loop.
#[test]
fn the_nfb_loop_does_not_comb_the_power_stage() {
    let sr = 48_000.0;
    // A linear power curve isolates the loop from the clipper.
    let ratio = |freq: f32| {
        let mut os = PowerOs::new(sr);
        let mut closed_nfb = GlobalNfb::new(sr, 0.5, 220.0);
        let mut pf_closed = 0.0f32;
        let (mut e_open, mut e_closed) = (0.0f64, 0.0f64);
        let n = 48_000usize;
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f32 / sr).sin();
            let y_open = os.shape(x, |u| u);
            let y_closed =
                os.shape_closed_loop(x, &mut closed_nfb, 0.5, &mut pf_closed, |u| u);
            if i >= n / 2 {
                e_open += f64::from(y_open * y_open);
                e_closed += f64::from(y_closed * y_closed);
            }
        }
        (e_closed / e_open).sqrt()
    };
    let g2 = ratio(2_000.0);
    let g4 = ratio(4_000.0);
    assert!(
        (g4 / g2 - 1.0).abs() < 0.15,
        "the loop combs the power stage: 2 kHz {g2:.3}, 4 kHz {g4:.3}"
    );
}
```

Remove the unused `open_nfb`/`pf_open` scaffolding if clippy objects (the `let _`
line is only there to keep the type visible while the test is written; delete
both and the `let _` before finishing).

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test --release --lib dsp::amp::tests::presence_bleeds -- --nocapture`

Expected: FAIL to compile — `shape_closed_loop` not found. After adding a
stub, `presence_bleeds...` fails because all three RMS values are equal at
6 kHz.

- [ ] **Step 3: Implement**

In `src/dsp/amp/mod.rs` near `PowerOs`:

```rust
/// Oversampling factor of the power-stage clipper. Anything expressed in
/// base-rate samples inside the loop must be scaled by this.
pub(crate) const POWER_OS_RATE: f32 = 8.0;
```

`GlobalNfb::new`:

```rust
pub fn new(sr: f32, beta: f32, lf_corner: f32) -> Self {
    // `sr` is the *base* rate. The divider is clocked once per oversampled
    // sample (see `PowerOs::shape_closed_loop`), so its corners must be
    // designed at the oversampled rate or they drift by 8x.
    let os_sr = sr * POWER_OS_RATE;
    Self {
        lf: Biquad::lowpass(os_sr, lf_corner, 0.707),
        bleed: Biquad::highpass(os_sr, 4000.0, 0.707),
        beta,
    }
}
```

`GlobalNfb::feedback` — replace the bled line and its comment:

```rust
// Presence bleed: the pot removes the *top* from the loop path, so more
// presence means less high-frequency feedback (a brighter, less damped top).
// 0.5 is neutral, so the pot does not shift the amp's balance at rest.
let bled = self.bleed.process(shaped);
let bleed_amt = (presence - 0.5) * 2.0;
let looped = shaped - bleed_amt * bled;
```

Add to `impl PowerOs` (and update the struct's doc to mention both methods):

```rust
/// Run the power-stage curve at 8× with the global NFB loop closed *inside*
/// the oversampled loop.
///
/// The feedback is the previous **high-rate** output sample, so the loop's
/// delay is one high-rate sample (1/8 base sample) instead of the
/// interpolator+decimator's ~11 base-rate samples. That delay is what used to
/// turn the divider into a comb filter at 2/4/6 kHz.
#[inline]
pub fn shape_closed_loop<F: FnMut(f32) -> f32>(
    &mut self,
    x: f32,
    nfb: &mut GlobalNfb,
    presence: f32,
    pf_out: &mut f32,
    mut f: F,
) -> f32 {
    let up = self.os.upsample(x);
    let mut down = [0.0f32; 8];
    let mut prev = *pf_out;
    for (o, &u) in down.iter_mut().zip(up.iter()) {
        let y = f(u - nfb.feedback(prev, presence));
        *o = y;
        prev = y;
    }
    *pf_out = prev;
    self.os.downsample(down)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --release --lib dsp::amp::tests::presence_bleeds dsp::amp::tests::the_nfb_loop`

Expected: both PASS. If `the_nfb_loop` still shows a comb, the feedback is
still taken from the decimated output — check that `prev` is assigned
`y` inside the loop.

- [ ] **Step 5: Commit**

```bash
git add src/dsp/amp/mod.rs
git commit -m "fix(amp): close the NFB loop at the oversampled rate and fix the presence bleed"
```

---

### Task 4: Wire all nine models to the closed loop, and Randall's presence

**Files:**
- Modify: `src/dsp/amp/{marshall,mesa,randall,vox,hiwatt,plexi,fender,supro,tweed}.rs` (`power_amp`)
- Test: `src/dsp/amp/mod.rs` tests module

**Interfaces:**
- Consumes: `PowerOs::shape_closed_loop(x, &mut self.nfb, presence, &mut self.pf_out, |u| ...)` from Task 3.

- [ ] **Step 1: Write the failing test**

Add to the `src/dsp/amp/mod.rs` tests module:

```rust
/// The corrected divider must stay finite at every sample rate the engine
/// supports, at both presence extremes and at full gain.
#[test]
fn nfb_is_stable_at_44k1_and_96k() {
    for sr in [44_100.0f32, 96_000.0] {
        for model in AmpModel::ALL {
            let mut amp: Box<dyn Amplifier> = match model {
                AmpModel::Marshall => Box::new(Marshall::new(sr)),
                AmpModel::Mesa => Box::new(Mesa::new(sr)),
                AmpModel::Randall => Box::new(Randall::new(sr)),
                AmpModel::Vox => Box::new(Vox::new(sr)),
                AmpModel::Hiwatt => Box::new(Hiwatt::new(sr)),
                AmpModel::Plexi => Box::new(Plexi::new(sr)),
                AmpModel::Fender => Box::new(Fender::new(sr)),
                AmpModel::Supro => Box::new(Supro::new(sr)),
                AmpModel::Tweed => Box::new(Tweed::new(sr)),
            };
            amp.set_load(crate::dsp::CabModel::Mesa.speaker_load());
            for presence in [0.0f32, 0.5, 1.0] {
                let knobs = standard_knobs(model, 1.0, 0.5, 0.5, 0.6, presence, 1.0);
                for n in 0..(sr as usize) {
                    let x = (2.0 * PI * 110.0 * n as f32 / sr).sin() * 0.5;
                    let y = amp.process(x, &knobs);
                    assert!(y.is_finite(), "{model:?} @ {sr} Hz, presence {presence}");
                }
            }
        }
    }
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --release --lib dsp::amp::tests::nfb_is_stable_at`

Expected: compiles, but the models still use the old base-rate tap; the test
may pass — this test is a *safety net*, not the RED for the wiring. The RED for
the wiring is that `the_nfb_loop_does_not_comb_the_power_stage` passes while
the models still call `shape`; record that in the ledger and proceed.

- [ ] **Step 3: Wire every `power_amp`**

In each of the nine models, replace the two-line feedback/shape block with the
closed-loop call. The closure body is unchanged.

Marshall (`src/dsp/amp/marshall.rs:264-269`) becomes:

```rust
// The divider is evaluated per oversampled sample inside the loop, so its
// only delay is one high-rate sample rather than the FIR's ~11 base samples.
let out = self.os_power.shape_closed_loop(
    x,
    &mut self.nfb,
    presence,
    &mut self.pf_out,
    |u| TubeClip::PUSH_PULL.shape(u * drive_up) * rail * 0.7992,
);
```

Do the same shape-preserving edit in the other eight; keep each model's
existing drive/rail/trim expression inside the closure. The input argument
that was `x - fb` (or `x * drive - fb`) becomes the first argument `x` (or
`x * drive`).

Randall additionally changes `self.nfb.feedback(self.pf_out, treble)` to pass
`presence` — with the new call that is simply the `presence` argument to
`shape_closed_loop`.

- [ ] **Step 4: Run the amp suite**

Run: `cargo test --release --lib dsp::amp::tests`

Expected: PASS, including `nfb_scoops_the_midrange_and_leaves_the_bass`,
`presence_changes_the_top_end`, `master_drives_the_power_stage`, and the
stability tests. If the scoop test fails on its hard-coded beta=0 references
(0.4777 / 0.6495), re-measure them by temporarily setting `NFB_BETA = 0.0` in
`marshall.rs`, rendering the same 80/400 Hz tones, recording the RMS, and
restoring the constant; note the new values in the commit message.

- [ ] **Step 5: Commit**

```bash
git add src/dsp/amp/*.rs
git commit -m "fix(amp): run every model's NFB loop inside the 8x power stage"
```

---

### Task 5: Cab cone-excursion rate and mic-position state

**Files:**
- Modify: `src/dsp/cab/mod.rs:217` (`SpeakerDrive::new`), `src/dsp/cab/mod.rs:844-848` (`MicPosition::clear`), `src/dsp/cab/mod.rs:609` (`speaker_drive_stats`)
- Test: `src/dsp/cab/mod.rs` (new tests module at end of file)

**Interfaces:**
- No public signature changes.

- [ ] **Step 1: Write the failing tests**

Append to `src/dsp/cab/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// `disp_lp` is clocked once per oversampled sample, so it must be designed
    /// at `4 * sr`. At base rate its 100 Hz corner landed at 400 Hz, letting
    /// midrange guitar fundamentals modulate the cone.
    #[test]
    fn cone_displacement_filter_runs_at_the_oversampled_rate() {
        let sr = 48_000.0;
        let mut drive = SpeakerDrive::new(sr);
        let n = 4_800usize;
        let mut e = 0.0f64;
        for i in 0..n {
            let x = (std::f32::consts::PI * 2.0 * 300.0 * i as f32 / sr).sin() * 0.5;
            for &u in drive.os.upsample(x).iter() {
                let d = drive.disp_lp.process(u);
                e += f64::from(d * d);
            }
        }
        let rms = (e / (n as f64 * DRIVE_OS_RATE as f64)).sqrt() as f32;
        // 100 Hz 2nd-order @ 300 Hz ≈ -19 dB; the broken 400 Hz corner was ~-1 dB.
        assert!(rms < 0.1, "300 Hz leaked into cone displacement: {rms}");
    }

    /// A cab switch calls `clear()` and then `set(mic_pos)`. `clear()` must
    /// invalidate the cached position, or `mic_pos == 0.0` (Edge) is a no-op.
    #[test]
    fn clearing_mic_position_reapplies_the_position() {
        let sr = 48_000.0;
        let render = |mic: &mut MicPosition| -> Vec<f32> {
            (0..256)
                .map(|i| {
                    let x = (std::f32::consts::PI * 2.0 * 3_000.0 * i as f32 / sr).sin() * 0.5;
                    mic.process(x, x).0
                })
                .collect()
        };
        let mut fresh = MicPosition::new(sr);
        fresh.set(0.0);
        let mut switched = MicPosition::new(sr);
        switched.clear();
        switched.set(0.0);
        assert_eq!(render(&mut fresh), render(&mut switched));
    }
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test --release --lib dsp::cab::tests -- --nocapture`

Expected: `cone_displacement...` FAILS (RMS ≈ 0.3); `clearing_mic_position...`
FAILS (neutral vs Edge renders differ).

- [ ] **Step 3: Implement**

`SpeakerDrive::new`:

```rust
// Clocked once per oversampled sample (see `process`), so design it at the
// oversampled rate or the 100 Hz corner lands at 400 Hz.
disp_lp: Biquad::lowpass(sr * DRIVE_OS_RATE, DISP_FC, DISP_Q),
```

`MicPosition::clear`:

```rust
fn clear(&mut self) {
    self.l.clear();
    self.r.clear();
    // Force the next `set` to retune. A cab switch is exactly the moment the
    // incoming cab must adopt the current mic position; leaving this at 0.0
    // silently skipped `set(0.0)` (Edge).
    self.last_pos = -1.0;
}
```

`speaker_drive_stats` mirrors the real path: design its filter at
`sr * DRIVE_OS_RATE` and clock it `DRIVE_OS_RATE` times per input sample,
keeping the last sub-step's displacement (the same value the base-rate
accounting reads after the oversampled loop).

- [ ] **Step 4: Run the tests**

Run: `cargo test --release --lib dsp::cab::tests`

Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add src/dsp/cab/mod.rs
git commit -m "fix(cab): clock the cone filter at 4x and invalidate the mic-position cache on clear"
```

---

### Task 6: Feed pre-window audio to the export chain

**Files:**
- Modify: `src/export.rs:357-358`
- Test: `src/export.rs` tests module

**Interfaces:**
- No signature changes.

- [ ] **Step 1: Write the failing test**

Add to the `src/export.rs` tests module:

```rust
/// A region export still renders the chain from frame 0 so the rig is warm
/// (sag, cab delay line, reverb/delay tails). Clamping the *input* mix to
/// `win_start` muted that warm-up and made the window start from silence —
/// losing every tail that crossed the region boundary.
#[test]
fn region_export_feeds_pre_window_audio_to_the_chain() {
    let dir = std::env::temp_dir().join(format!(
        "headroom-export-prefeed-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let take = dir.join("take.wav");
    // A clip that ends 200 samples before the window, with a long wet tail.
    let samples: Vec<f32> = (0..2_800)
        .map(|i| {
            let t = i as f32 / 48_000.0;
            (-t * 8.0).exp() * (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.6
        })
        .collect();
    write_mono_wav(&take, 48_000, &samples);

    let params = Params::new();
    params.rev_enabled.store(true, Relaxed);
    params.rev_mix.store(0.9, Relaxed);
    params.rev_room.store(0.95, Relaxed);
    let job = ExportJob {
        dest: dir.join("out.wav"),
        sample_rate: 48_000,
        project_sample_rate: 48_000,
        clips: vec![ExportClip {
            path: take,
            start_ticks: 0,
            gain: 1.0,
        }],
        rig: snapshot_rig(&params),
        range_ticks: Some((3_000, 4_000)),
        ir_path: None,
        ir_active: false,
        insert: None,
        amp: None,
    };
    let cancel = AtomicBool::new(false);
    let progress = AtomicU32::new(0);
    let out = run(job, &progress, &cancel).expect("export");
    let reader = hound::WavReader::open(&out).expect("open out");
    let raw: Vec<f32> = reader.samples::<f32>().map(|s| s.expect("sample")).collect();
    let rms = (raw.iter().map(|&x| f64::from(x * x)).sum::<f64>() / raw.len() as f64).sqrt();
    assert!(rms > 1e-4, "the pre-window tail was muted: rms {rms}");
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test --release --lib export::tests::region_export_feeds -- --nocapture`

Expected: FAIL with `rms 0` (or the amp's zero floor), because the clip is
muted before `win_start`.

- [ ] **Step 3: Implement**

In the input mix loop:

```rust
// Feed the whole clip timeline to the chain; the *write* below is what
// clamps to the region. Clamping here muted the warm-up and lost every tail
// crossing the region boundary.
let lo = written.max(clip.start);
let hi = (written + n).min(clip_end);
```

Leave the write loop's `let lo = written.max(win_start); let hi = (written + n).min(win_end);` untouched.

- [ ] **Step 4: Run the export suite**

Run: `cargo test --release --lib export::tests`

Expected: PASS, including `range_export_writes_only_the_window` and the
preroll test.

- [ ] **Step 5: Commit**

```bash
git add src/export.rs
git commit -m "fix(export): warm the chain from pre-window audio on region exports"
```

---

### Task 7: Record the drift, re-bless if needed, and run the gate

**Files:**
- Modify: `docs/fidelity-implement.md` (increment log)
- Modify: `docs/fidelity/baseline-synth-48k.toml` only if `--check` drifts

- [ ] **Step 1: Run the full gate**

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --release --lib
```

Expected: all green. Save the tail of the test output in the ledger.

- [ ] **Step 2: Measure fidelity drift**

```bash
cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml
```

Expected: the NFB loop change moves the high bands of every amp; if `--check`
reports drift, re-bless in the same commit:

```bash
cargo run --release --example fidelity_render -- --presets all \
    --write-baseline docs/fidelity/baseline-synth-48k.toml
```

- [ ] **Step 3: Log the increment**

Add to `docs/fidelity-implement.md`'s increment log, matching the existing
table/heading style:

- **Chain audit 2026-10, phase 1** — corrected the P90 provisional target;
  routed looper playback through `take_chain`; fixed the NFB presence bleed and
  moved the loop inside the 8× power stage (12-sample → 1/8-sample loop delay);
  wired Randall's presence; clocked the cab cone filter at 4×; invalidated the
  mic-position cache on `clear`; un-muted pre-window audio in region exports.
  Record the measured baseline drift (or "no re-bless needed") and the LUFS/LTAS
  direction in the entry.

- [ ] **Step 4: Commit**

```bash
git add docs/fidelity-implement.md docs/fidelity/baseline-synth-48k.toml
git commit -m "docs: record the phase-1 chain-audit drift"
```
