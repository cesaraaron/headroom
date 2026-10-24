# End-to-End Signal Chain & Audio Quality Audit — 2026-10

A comprehensive technical review of the `rusty-riff` audio pipeline from input capture to final output and offline export:

```text
Device Input & Calibration (src/audio/*, src/dsp/resample.rs, src/looper.rs)
  → Pre-Amp Mono Pedals    (src/dsp/effects/*)
  → Amplifier Models       (src/dsp/amp/*, src/dsp/tonestack.rs)
  → Cabinet & Convolution  (src/dsp/cab/*, src/dsp/conv.rs)
  → Post-Amp Stereo Rack   (src/dsp/effects/*)
  → Master Bus & Limiter   (src/dsp/mod.rs, src/dsp/limiter.rs)
  → Monitor Sum & Export   (src/audio/mod.rs, src/export.rs)
```

Every item below has been verified against `HEAD` with exact `file:line` references, mathematical/logical root cause, audio impact, and a concrete fix.

---

## Severity Legend

| Severity | Meaning |
| --- | --- |
| **CRITICAL** | Breaks a core signal path, corrupts gain staging by >10 dB, or produces wrong audio output by default. |
| **HIGH** | Audible defect in normal use (aliasing, comb filtering, zipper clicks, broken knob/mode, or unstable filter). |
| **MEDIUM** | Noticeable under specific conditions (sample rates ≠ 48 kHz, bypass/reorder transitions, long tracks, or edge settings). |
| **LOW** | Precision drift over long sessions, redundant audio-thread CPU work, or minor doc/code mismatch. |

---

## Executive Summary Matrix

| ID | Stage | Severity | Location | Summary |
| --- | --- | --- | --- | --- |
| **1.1** | Input / Cal | **CRITICAL** | `src/audio/calibration.rs:144-150` | `PickupClass::P90` target is `-4.5 dBFS` (+15.2 dB hotter than Humbucker `-19.7 dBFS`), boosting P90 inputs by ~+17 dB too much. |
| **1.2** | Looper | **CRITICAL** | `src/audio/mod.rs:1143-1151` | Looper captures dry DI input (`self.in_buf`) and sums it directly into post-cab output, playing raw un-amplified DI to speakers. |
| **3.1** | Amp NFB | **CRITICAL** | `src/dsp/amp/mod.rs:631-665` | `GlobalNfb` subtracts a 4 kHz HP (`shaped - HP(shaped)` = low/mids, not highs) and wraps a 12-sample `Oversampler8` delay, turning NFB into a 2 kHz comb filter. |
| **4.1** | Cab Drive | **CRITICAL** | `src/dsp/cab/mod.rs:217, 260-265` | `SpeakerDrive::disp_lp` is designed at base `sr` (100 Hz) but clocked inside `Oversampler4` (`4 * sr`), shifting the cone-excursion cutoff to **400 Hz**. |
| **5.1** | Export | **CRITICAL** | `src/export.rs:354-366` | Loop-region export clamps the input mix loop to `win_start`, feeding pure silence to the chain before `win_start` and losing all pre-window tails/sag. |
| **1.3** | Audio I/O | **HIGH** | `src/audio/mod.rs:681-701, 1223, 1274` | Fallback when output rejects `in_sr` runs input and output streams at different rates with no resampler (pitch shift + continuous xruns). |
| **2.1** | Wah / Comp | **HIGH** | `wah.rs:99`, `compressor.rs:72-77` | Auto-wah and compressor detector thresholds assume ~0 dBFS input; on calibrated DI inputs (`-19.7` / `-24.6 dBFS`), auto-wah barely moves (+5–9%) and compressor boosts by up to +8.6 dB before compressing. |
| **2.2** | Fuzz | **HIGH** | `src/dsp/effects/fuzz.rs:79-84, 105` | `set_guitar` stores clamped `gv >= 0.03` in `last_guitar`; when `guitar == 0.0`, `param_changed` fires every sample (48 kHz `powf` + trig rebuild). |
| **2.3** | Fuzz | **HIGH** | `src/dsp/effects/fuzz.rs:211-229` | `ff_clip` ($f'(0^+)=1.0, f'(0^-)=0.9$) and `tb_clip` ($f'(0^+)=1.0, f'(0^-)=1.1$) have a slope kink at $x=0$, causing scale-invariant zero-crossing rectification on quiet signals. |
| **2.4** | DS-1 / Drives | **HIGH** | `src/dsp/effects/distortion.rs:119, 128, 138` | `ds1_clip(u * gain) / gain.sqrt()` drops saturated output by up to `-16.1 dB` as `DRIVE` increases; `ds_drive` (MIDI CC target) and `level` are unsmoothed. |
| **2.5** | Wah | **HIGH** | `src/dsp/effects/wah.rs:35-37, 93-102` | `wah_position` (primary MIDI CC 11 target) and `wah_mode` are unsmoothed (`SmoothedGain` was placed on `mix` instead), causing SVF zipper noise and mode-switch pops. |
| **2.6** | Whammy | **HIGH** | `src/dsp/effects/pitch.rs:41-52, 101-127` | `GRAIN = 2048.0` is hardcoded in samples (halving in ms at 96 kHz); unison (`pitch = 0.5`) freezes `phase` at a random static comb-filter delay; `ratio` is unsmoothed. |
| **3.2** | Randall | **HIGH** | `src/dsp/amp/randall.rs:188-189, 266` | `Randall` passes `treble` (`knobs[3]`) instead of `presence` (`knobs[4]`) to `self.nfb.feedback(...)`. |
| **3.3** | Amp Picker | **HIGH** | `src/dsp/mod.rs:114-126` | `AmpModel::prev()` maps `Marshall => Supro` instead of `Tweed`, making `Tweed Deluxe` unreachable when cycling backwards. |
| **3.4** | Amp Switch | **HIGH** | `src/dsp/amp/mod.rs:958-1018` | Built-in `AmpModel` switching has no declick crossfade and does not clear stale `Oversampler8` / `Fender` `SpringReverb` buffers. |
| **3.5** | Amp OT | **HIGH** | `src/dsp/amp/mod.rs:466-478` | `OutputTransformer` runs a narrow Gaussian zero-crossing notch `y * exp(-25 * y²)` at base `sr` without oversampling, plus a non-complementary 2nd-order crossover. |
| **4.2** | Cab Mic | **HIGH** | `src/dsp/cab/mod.rs:843-876, 1038-1049` | `MicPosition::clear()` sets `last_pos = 0.0` without retuning filter coefficients, breaking `mic_pos == 0.0` (Edge) on boot and cab switches. |
| **4.3** | Cubic Interp | **HIGH** | `src/dsp/effects/mod.rs:100-117` | `read_cubic` reads unwritten `buf[write]` (the oldest sample in the ring buffer, up to 500 ms old) whenever `1.0 < delay < 2.0` in read-before-write effects. |
| **4.4** | Flanger | **HIGH** | `src/dsp/effects/flanger.rs:105, 115-120` | `span = min_ms + sweep_ms * depth` adds `min_ms` twice (`min_ms + span * lfo`), so `depth = 0.0` still sweeps `0.5–1.0 ms` and Mistress Filter Matrix jumps on unfreeze. |
| **4.5** | External IR | **HIGH** | `src/dsp/mod.rs:1917-1933`, `external.rs:207` | Toggling `cab_external_active` has no declick ramp and replays 93–170 ms of stale FDL convolution spectra in both directions. |
| **5.2** | Limiter | **HIGH** | `src/dsp/limiter.rs:44-47, 127, 206-210` | `Limiter` gain computer has a 0.265 dB step discontinuity at `env == ceiling`, and lookahead subtracts `OS_CORNER_DELAY` while ignoring `det` filter delay (~17 samples short). |
| **5.3** | Biquad | **HIGH** | `src/dsp/biquad.rs:67-140` | `Biquad` does not clamp `freq < 0.5 * sr`; if `freq > Nyquist` (e.g., low-SR device), $\alpha < 0 \implies |p| > 1$, causing instant exponential `NaN` blowup. |
| **1.4** | Audio I/O | **MEDIUM** | `src/audio/mod.rs:1313, 1486` | Inter-stream ring buffer has no startup prefill or high-water mark; drift can accumulate up to 400 ms latency or underrun on startup. |
| **1.5** | Resampler | **MEDIUM** | `src/dsp/resample.rs:22-27` | `center = m as f32 / ratio` loses fractional `f32` precision after $2^{23}$ samples (~2.9 min) and rounds odd samples to even after $2^{24}$ (~5.8 min). |
| **1.6** | Looper | **MEDIUM** | `src/looper.rs:196, 229-236` | Re-enabling overdub for a 2nd pass does not reset `snapshotted`, so Undo wipes out two layers; loop wrap and punch-in/out lack crossfades. |
| **1.7** | Audio / Tuner | **MEDIUM** | `src/dsp/tuner.rs:28, 135`, `audio/mod.rs:134` | `TunerDetector` runs ~6M FLOPs synchronously in one callback and uses fixed `WINDOW = 2048` (too short for 55 Hz at 96 kHz); XRUN handler does blocking file I/O. |
| **2.7** | TS-808 / GEQ | **MEDIUM** | `tube_screamer.rs:68, 90, 108`, `graphic_eq.rs:62` | TS-808 `fb_lp` (61.2 kHz → 5.7 kHz) runs at base `sr` outside `Oversampler4`; `new()` prematurely primes `SmoothedGain` at `0.0`. |
| **2.8** | DS-1 / ML-2 | **MEDIUM** | `distortion.rs:127`, `metal_core.rs:148` | Static linear `pre_clip_hp` and `pre_clip_lp` biquads run at 4×/8× inside the oversampler, degrading `f32` pole precision at 150 Hz and costing 4×/8× CPU. |
| **2.9** | Uni-Vibe | **MEDIUM** | `src/dsp/effects/uni_vibe.rs:95, 109-111` | `mode` is applied after `mix.step()` (single-sample step on Chorus ↔ Vibrato switch); `powf` is evaluated 4× per sample inside the stage loop. |
| **2.10** | Noise Gate | **MEDIUM** | `src/dsp/effects/noise_gate.rs:42, 53, 94-109` | Gate initializes open (`gain: 1.0`), `RELEASE` only sets hold time (not release slope), and fixed `CLOSE_MS = 5.0` ms chops decaying low notes within a single cycle. |
| **3.6** | Vox / Hiwatt | **MEDIUM** | `vox.rs:235`, `hiwatt.rs:281` | `split_gain(..., p = 1.0)` makes stage-2 `k2` a constant `-11 dB` attenuator that can never clip; `split_gain` runs 8× per sample inside the oversampler loop. |
| **3.7** | Supro / Tweed | **MEDIUM** | `supro.rs:123`, `tweed.rs:114` | US 60 Hz amps initialize `SupplyRipple` at 100 Hz (UK mains) instead of 120 Hz. |
| **3.8** | Presence | **MEDIUM** | `src/dsp/amp/mod.rs:403-436` | `DynamicPresence` sums `x + g * hp.process(x)` using a 2nd-order HP whose phase shift causes cancellation dip below the corner. |
| **3.9** | Tone Stack | **MEDIUM** | `src/dsp/tonestack.rs:217, 221-254` | `ToneStack` stores un-normalized `y1..y3` before multiplying by `makeup` (causing transients on knob moves) and runs 432 trig calls per update. |
| **4.6** | Delay | **MEDIUM** | `src/dsp/effects/delay.rs:71, 104, 127` | `max_samples` lacks headroom for wow/flutter at `time = 1.0` (clamping modulation flat at `len - 3`); `time` is unsmoothed; `clear_buffers()` skips feedback biquads. |
| **4.7** | Cab / Reverb | **MEDIUM** | `ir.rs:155`, `external.rs:193`, `reverb.rs:58`, `spring.rs:87` | At `sr != 48 kHz`: cab modal gains scale $\propto \text{sr}$ (+6 dB at 96 kHz), `ExternalIrCab` L2 norm shifts gain by +3 dB at 96 kHz, and `Reverb`/`SpringReverb` loop damping is unscaled with `sr`. |
| **4.8** | Trem / Reorder | **MEDIUM** | `tremolo.rs:54, 103`, `dsp/mod.rs:2253` | `Tremolo` jumps 4 ms in latency when `pitch_depth` crosses `0.0` and uses linear interpolation; `Chorus`, `Tremolo`, `Pitch`, and `SpringReverb` are not cleared on reorder. |
| **5.4** | Bypass Ramp | **MEDIUM** | `src/dsp/mod.rs:1776, 2161-2220` | `bypass_ramp` starts at `0.0` on boot, comb-filters oversampled stages during the 6 ms fade, and freezes stale `SmoothedGain` values while bypassed. |
| **5.5** | AU Switch | **MEDIUM** | `src/dsp/mod.rs:1814, 2319-2434` | Built-in ↔ AU switch only flips between blocks (muting remainder of block), leaves `comp_delay` stale, and `replace_ext_amp` does not sync `applied_ext_amp`. |
| **5.6** | Output Meters | **MEDIUM** | `src/audio/mod.rs:1011, 1153, 1207, 1397` | `limit_decay` is applied per callback instead of per sample (102–410 s decay!), `limit_indicator` checks `0.95` instead of `1.0`, and `out_env` precedes `take_chain` and folds stereo to mono. |
| **5.7** | Oversample / Hum | **LOW** | `oversample.rs:82`, `dsp/mod.rs:1658, 2467` | `Oversampler<N>` normalizes total prototype sum instead of per-phase sums; `HumNoise` phasor lacks renormalization; `master_bus` doc says `output` is post-limiter while code applies it pre-limiter. |

---

## Stage 1 — Input, Calibration, Resampling & Stream Engine

### 1.1 `PickupClass::P90` calibration target is 15.2 dB hotter than Humbucker · **CRITICAL**
- **Location:** `src/audio/calibration.rs:144-150`
- **Code:**
  ```rust
  pub fn target_peak_dbfs(p: PickupClass) -> f32 {
      match p {
          PickupClass::SingleCoil => -24.6,
          PickupClass::P90 => -4.5,
          PickupClass::Humbucker => -19.7,
      }
  }
  ```
- **Root Cause:** When `SingleCoil` (`-24.6 dBFS`) and `Humbucker` (`-19.7 dBFS`) were calibrated to the B8 reference rig, `P90` was left at its legacy pre-B8 placeholder of `-4.5 dBFS` — which is **+15.2 dB hotter than a humbucker** and **+20.1 dB hotter than a single-coil**. Physically and electrically, a P90 sits between a single-coil and a humbucker (~`-22.0 dBFS`).
- **Audio Impact:** Running the calibration wizard with `P90` selected computes a `trim_db` that boosts the guitar by ~`+17 dB` too much (typically hitting the `+24 dB` clamp), slamming every pedal and amp model into heavy clipping.
- **Recommended Fix:** Set `PickupClass::P90` to an interpolated reference target between `SingleCoil` and `Humbucker` (e.g., `-22.0 dBFS`) or measure a reference P90 per the `CONTRIBUTING.md` calibration runbook, and add a unit test asserting `SingleCoil < P90 < Humbucker`.

---

### 1.2 `Looper` records dry DI input and sums it directly into post-cab output · **CRITICAL**
- **Location:** `src/audio/mod.rs:1143-1151`, `src/looper.rs:208-244`
- **Code:**
  ```rust
  let click = self.metro_voice.next_sample(metro_active, metro_bpm);
  let frame = self.player.next_frame(&transport);
  let loop_sample = self.looper.process(sample);
  self.take_in[i] = frame.take;
  let out_left = self.out_l[i] + frame.import_l + click + loop_sample;
  let out_right = self.out_r[i] + frame.import_r + click + loop_sample;
  ```
- **Root Cause:** `sample` is read from `self.in_buf` (the post-trim, pre-rig **dry DI guitar** at ~`-20 dBFS`), and `loop_sample` is summed directly into `self.out_l[i]` and `self.out_r[i]` **after** `self.chain.process_block`.
- **Audio Impact:** Recorded phrase loops play back as **unprocessed, un-amplified, mono dry DI guitar** (~20 dB below the live rig, with zero pedals, amp, cab, or effects).
- **Recommended Fix:** Either:
  1. **Re-amp through `take_chain` (like timeline takes):** Add `loop_sample` into `self.take_in[i] = frame.take + loop_sample;` instead of `out_left`/`out_right`, so the looper is processed through `take_chain`; or
  2. **Post-rig looper pedal (captures the processed tone):** Feed the post-chain signal (`self.out_l[i]`, `self.out_r[i]`) into `self.looper` so each overdub layer preserves the tone it was recorded with.

---

### 1.3 Uncompensated sample-rate mismatch when output device rejects input rate · **HIGH**
- **Location:** `src/audio/mod.rs:681-701, 1223, 1274`
- **Root Cause:** In `negotiate_configs()`, if `output.supported_output_configs()` has no range containing `in_sr`, lines `681-689` fall back to `output.default_output_config()` at `fallback_sr`, log a warning, and still return `in_sr as f32` as the engine sample rate. The input callback pushes `in_sr * out_channels` samples/sec into the SPSC ring buffer while the output stream pops `fallback_sr * out_channels` samples/sec with no resampling.
- **Audio Impact:** Pitch and time shift by `fallback_sr / in_sr` (e.g., ±1.47 semitones between 44.1 kHz and 48 kHz), accompanied by continuous buffer overruns (48k → 44.1k drops 3,900 frames/sec) or underruns (44.1k → 48k inserts 3,900 zero frames/sec).
- **Recommended Fix:** If the output device does not support `in_sr`, check if the input device supports `fallback_sr` so both streams open at `fallback_sr`; if the two devices genuinely have disjoint supported rates, insert a streaming resampler on the output consumer or return a clear startup error.

---

### 1.4 Inter-stream ring buffer has no startup prefill or clock-drift watermark · **MEDIUM**
- **Location:** `src/audio/mod.rs:1223, 1273-1275, 1313-1314, 1486-1487`
- **Root Cause:** `buf_samples = (sr as usize) / 5 * out_channels * 2` allocates a 400 ms ring buffer with 0 samples prefilled before starting both hardware streams, and never bounds the queue depth.
- **Audio Impact:**
  - If the output callback fires before the first input callback (or runs on a slightly faster crystal), `consumer.pop().unwrap_or(0.0)` injects hard `0.0` steps.
  - If the input crystal is slightly faster (or the OS delays an output callback once), queued audio accumulates in the ring buffer up to **400 ms**, permanently inflating live monitoring latency until the buffer overflows.
- **Recommended Fix:** Prefill 1–2 callback periods of silence on startup and enforce a high-water mark in the consumer/producer (e.g., 3–4 periods) so clock drift or a transient scheduler hiccup cannot build 400 ms of standing latency.

---

### 1.5 `resample.rs` loses `f32` fractional precision after ~2.9–5.8 minutes of audio · **MEDIUM**
- **Location:** `src/dsp/resample.rs:22-27`
- **Code:**
  ```rust
  for (m, o) in out.iter_mut().enumerate() {
      let center = m as f32 / ratio; // position in input samples
      let i0 = (center - half).floor() as isize;
      let i1 = (center + half).ceil() as isize;
      ...
      let t = center - i as f32;
  ```
- **Root Cause:** `m` and `i` are converted to 32-bit `f32` (24-bit mantissa = $16,777,216$ samples ≈ 5 min 50 s at 48 kHz).
  - For $m > 8,388,608$ (~2.9 min), `f32` ULP is `1.0`, so `t = center - i as f32` rounds to **0 fractional bits** (collapsing the 32-tap sinc interpolator to nearest-neighbor integer samples).
  - For $m > 16,777,216$ (~5.8 min), `f32` ULP is `2.0`, so odd integers `m` round to even integers, turning resampled backing tracks into a 2-sample staircase.
- **Recommended Fix:** Compute `center` and `t` in `f64`:
  ```rust
  let ratio_d = to_sr as f64 / from_sr as f64;
  let center = m as f64 / ratio_d;
  ...
  let t = (center - i as f64) as f32;
  ```

---

### 1.6 `Looper` multi-layer overdub undo bug & un-crossfaded boundaries · **MEDIUM**
- **Location:** `src/looper.rs:196, 229-236`
- **Root Cause:** `self.snapshotted` is only reset on transport state transitions (`finish_recording`, `start_recording`, `clear`, `undo`), not when `self.overdub` transitions from `false → true` in `poll_controls` (`self.overdub = ctl.overdub.load(Relaxed);`). If a user records overdub pass 1, toggles overdub off to listen, and toggles overdub back on to record pass 2, `prev` is not snapshotted before pass 2 overwrites `buf` — so pressing Undo wipes out **both** pass 2 and pass 1. Additionally, loop wrap (`cursor: len - 1 → 0`), stop, and overdub punch-in/out have no short crossfade.
- **Recommended Fix:** Reset `self.snapshotted = false` in `poll_controls` whenever `!was_overdub && now_overdub`, and apply a short (2–4 ms) linear crossfade at loop boundaries and punch-in/out.

---

### 1.7 `TunerDetector` audio-thread CPU spike, 96 kHz window length bug, and XRUN file I/O · **MEDIUM**
- **Locations:** `src/dsp/tuner.rs:28-29, 135-161, 218-231` and `src/audio/mod.rs:134-153, 951-973, 1249, 1279`
- **Root Cause:**
  1. `TunerDetector` uses a fixed `WINDOW = 2048` samples regardless of `sr`. At `sr = 96 kHz`, `max_lag` for 55 Hz is `1746` samples, leaving only `2048 - 1746 = 302` samples (**3.1 ms**, <20% of one period of 55 Hz) of overlap in `detect_pitch`, degrading low-string tuning accuracy at 88.2/96 kHz.
  2. Every `HOP = 1024` samples, `analyze()` runs synchronously inside the real-time audio callback (~$1.34 \times 10^6$ autocorrelation iterations + 64 Goertzel filters ≈ 6M FLOPs in a single callback).
  3. `build_input_stream` / `build_output_stream` error callbacks call `log_line()`, which performs blocking `fs::create_dir_all`, `OpenOptions::open`, and `writeln!` on the real-time stream thread during an XRUN.
- **Recommended Fix:** Decimate input by 2× (or scale `WINDOW` with `sr` and move `analyze()` to the control thread via a small ring buffer), and route XRUN counts through an atomic counter logged by the UI thread.

---

## Stage 2 — Pre-Amp Mono Pedals (`src/dsp/effects/`)

### 2.1 Auto-Wah and Compressor detectors are mis-scaled for calibrated DI guitar levels · **HIGH**
- **Locations:** `src/dsp/effects/wah.rs:45, 87-99` and `src/dsp/effects/compressor.rs:72-77, 101-115`
- **Root Cause:**
  Per `src/audio/calibration.rs:144-150`, calibrated DI guitar peaks sit at `-24.6 dBFS` (`0.0589`, SingleCoil) and `-19.7 dBFS` (`0.1035`, Humbucker), so rectified average envelopes sit around `0.025–0.066`:
  1. **Auto-Wah (`wah.rs:99`):** `let sweep = (self.env * sens.clamp(0.0, 1.0) * SWEEP_OCT).exp2();` assumes a ~`0 dBFS` input (like the unit test at `wah.rs:229`, which drives a `0.9` sine). At `DEFAULT_WAH_SENS = 0.55`, a calibrated Humbucker sweeps `fc` by only **+9%** (`0.125` octaves) on hard pick attacks and `< 3.5%` in sustain; a SingleCoil sweeps by only **+5%**.
  2. **Compressor (`compressor.rs:72-77`):** `self.thresh_db = -6.0 - sustain * 34.0`. At `DEFAULT_CMP_SUSTAIN = 0.40`, `thresh_db = -19.6 dBFS` (`thresh_lin = 0.1047`) while `auto_makeup = 1.595` (`+4.05 dB`). Because `self.env` is a rectified average (`≤ 0.066` on Humbucker, `≤ 0.037` on SingleCoil), a calibrated guitar **never reaches threshold** across the lower 50–65% of the `SUSTAIN` knob (`gain == 1.0`, zero compression), while `auto_makeup` boosts the uncompressed signal by up to **+8.6 dB**.
- **Recommended Fix:**
  - In `Wah`, scale the envelope detector sensitivity to the calibrated DI reference level (approx. `8×–10×`, or normalize against `-20 dBFS`) so `DEFAULT_WAH_SENS = 0.55` sweeps 1.5–2 octaves on a calibrated pick attack.
  - In `Compressor`, lower the threshold range (`thresh_db`) to match calibrated DI levels (e.g., `-18 dB` to `-48 dB`) and compute `auto_makeup` relative to actual threshold depth Below `-18 dBFS` so `gain * auto_makeup` compresses peaks rather than acting as a fixed linear boost.

---

### 2.2 `Fuzz::set_guitar` stores clamped `gv >= 0.03` in `last_guitar`, causing per-sample filter rebuilds when `guitar == 0.0` · **HIGH**
- **Location:** `src/dsp/effects/fuzz.rs:79-84, 105-107`
- **Code:**
  ```rust
  fn set_guitar(&mut self, guitar: f32) {
      let gv = guitar.clamp(0.03, 1.0);
      let corner = (70.0 * (1.0 / gv).powf(1.3)).clamp(70.0, 1200.0);
      self.guitar_hp.set_highpass(self.sr, corner, 0.707);
      self.last_guitar = gv;
  }
  ```
- **Root Cause:** `set_guitar` stores the clamped `gv` (`>= 0.03`) in `self.last_guitar`, while `process` checks `if param_changed(guitar, self.last_guitar)` using the unclamped `guitar`. When `guitar < 0.029` (e.g., `0.0`), `(0.0 - 0.03).abs() > 0.001` is `true` on **every sample**, calling `powf`, `sin`, `cos`, and `set_highpass` at 48 kHz audio rate indefinitely.
- **Recommended Fix:** Set `self.last_guitar = guitar;` (the raw parameter) in `set_guitar`. Also keep `self.guitar_hp` ticked continuously (or set its cutoff to 20 Hz when `gv == 1.0`) instead of conditionally bypassing it at line 130 (`if voice == Voice::FuzzFace && gv < 0.999`), which otherwise injects a filter-state discontinuity into 95× of downstream fuzz gain when `gv` drops below `0.999`.

---

### 2.3 Derivative discontinuity (kink) at $x = 0$ in `ff_clip` and `tb_clip` causes crossover distortion on quiet signals · **HIGH**
- **Location:** `src/dsp/effects/fuzz.rs:211-229`
- **Code:**
  ```rust
  fn ff_clip(x: f32) -> f32 {
      if x >= 0.0 {
          (x * 0.7).tanh() / 0.7
      } else {
          (x * 0.9 / 0.8).tanh() * 0.8
      }
  }
  ```
- **Root Cause:**
  - For `ff_clip`: $f'(0^+) = \frac{0.7}{0.7}\operatorname{sech}^2(0) = \mathbf{1.0}$, while $f'(0^-) = 0.8 \cdot \frac{0.9}{0.8}\operatorname{sech}^2(0) = \mathbf{0.9} \neq 1.0$.
  - For `tb_clip`: $f'(0^+) = \frac{1.15}{1.15}\operatorname{sech}^2(0) = \mathbf{1.0}$, while $f'(0^-) = 0.82 \cdot \frac{1.1}{0.82}\operatorname{sech}^2(0) = \mathbf{1.1} \neq 1.0$.
  Because $f'(0^+) \neq f'(0^-)$, both shapers have a **first-derivative kink right at $x = 0$**. A slope kink at the origin is scale-invariant: even as signal amplitude $A \to 0$ (rolling `GUITAR` volume back for Fuzz Face cleanup, or decaying note tails), every zero crossing undergoes half-wave rectification $\Delta k \cdot |x|$ cascaded across 2–3 stages.
- **Recommended Fix:** Match small-signal slope at the origin ($f'(0^+) = f'(0^-) = 1.0$) while keeping asymmetric saturation rails $R_+, R_-$ via $\pm R_\pm \tanh(x / R_\pm)$:
  - `ff_clip`: negative branch `(x / 0.8).tanh() * 0.8` (slope 1.0 at 0, rail `-0.8`).
  - `tb_clip`: negative branch `(x / 0.82).tanh() * 0.82` (slope 1.0 at 0, rail `-0.82`).

---

### 2.4 DS-1 `ds1_clip(u * gain) / gain.sqrt()` inverts saturated level (-16.1 dB drop at max drive) & unsmoothed drive/level controls · **HIGH**
- **Locations:** `src/dsp/effects/distortion.rs:119, 128, 138` (plus `metal_core.rs:140, 165`, `fuzz.rs:137, 179`, `tube_screamer.rs:138`, `compressor.rs:77, 126`)
- **Root Cause:**
  1. In `distortion.rs:128`, `ds1_clip(u * gain) / gain.sqrt()` divides the clipped signal (whose rails are fixed at `+1.0 / -1.22`) by $\sqrt{1 + 40 \cdot \text{drive}}$. Once the signal is in saturation, turning `DRIVE` up from `0.0` to `1.0` **reduces** the clipper's output ceiling from `1.0` down to **`0.156` (`-16.1 dB`)** — the same `/ sqrt(g)` defect previously fixed in the amps (Phase C1).
  2. Although `src/dsp/effects/mod.rs:135` explicitly cites `ds_drive` as a MIDI CC target smoothed by `SmoothedGain`, `distortion.rs` never uses `SmoothedGain` for `drive` or `level`. Similarly, `Compressor::auto_makeup` (`0..+18 dB`, bound to `MidiTarget::CompSustain`), `MetalCore` `dist`/`level`, `Fuzz` `fuzz`/`guitar`/`level`, and `TubeScreamer` `level` are unsmoothed per-sample multiplies.
- **Recommended Fix:** Replace `/ gain.sqrt()` in `distortion.rs` with fixed rail normalization (compensating level rather than dividing the clipper rail by $\sqrt{\text{gain}}$), and wrap the drive/level/makeup scalars in `SmoothedGain`.

---

### 2.5 `wah_position` (the primary MIDI CC 11 target) and `wah_mode` are unsmoothed · **HIGH**
- **Location:** `src/dsp/effects/wah.rs:35-37, 93-102`
- **Root Cause:** `wah.rs:35-37` has the comment *"`wah_position` is the primary MIDI CC target in this app, so a pedal sweep lands here continuously"* on `mix: SmoothedGain`, while `position`, `mode`, and the resulting SVF cutoff `fc` have **no smoothing** at all (and `powf` is called every sample). When a 7-bit MIDI expression pedal moves, `src/midi.rs:163` snaps `wah_mode` to `1.0` and steps `wah_position` in 128 discrete increments while the SVF holds high-Q (`Q` up to `6.5`) resonant energy.
- **Recommended Fix:** Smooth the target cutoff frequency `fc` (or `position` and `mode`) with a `SmoothedGain` / one-pole filter (~5–8 ms) before computing `g = (PI * fc / self.sr).tan()`, and cache the knob-rate `powf` calls behind `param_changed`.

---

### 2.6 `Pitch` (Whammy) `GRAIN` is hardcoded in samples for 48 kHz & unison (`pitch = 0.5`) freezes at a random comb-filter delay · **HIGH**
- **Location:** `src/dsp/effects/pitch.rs:41-52, 101-127`
- **Root Cause:**
  1. `GRAIN = 2048.0` and `BUF_LEN = 4096` are fixed sample counts instead of scaling with `sr`. At 96 kHz, the grain shrinks to `21.3 ms` (and `10.7 ms` at 192 kHz), producing 23–47 Hz ring-modulation warble on low notes.
  2. At unison (`pitch = 0.5`, `ratio = 1.0`), `self.phase += (1.0 - ratio) / GRAIN` adds `0.0`, freezing `self.phase` at whatever `[0, 1)` value it had when `pitch` returned to `0.5`. With `mix > 0`, that sums two static delayed taps (up to `42.7 ms` late) with the dry signal, creating a random static comb filter / slapback.
  3. `self.ratio` is unsmoothed on pitch sweeps.
- **Recommended Fix:** Scale `grain` and `buf` capacity with `sr` (`~43 ms`), smooth `ratio` with `SmoothedGain`, and when `ratio ≈ 1.0` drive `phase` smoothly back to `0.0` (where `tap_a = 0` and `win_b = 0`) so unison has zero comb delay.

---

### 2.7 `TubeScreamer` feedback low-pass (`fb_lp`, 61.2 kHz → 5.7 kHz) runs at base `sr` outside `Oversampler4`, and `new()` prematurely primes `SmoothedGain` · **MEDIUM**
- **Locations:** `src/dsp/effects/tube_screamer.rs:68-71, 81, 90, 108, 129-133` and `src/dsp/effects/graphic_eq.rs:62`
- **Root Cause:**
  1. `feedback_corner_hz(drive)` spans `61.2 kHz` (`drive = 0.0`) to `5.66 kHz` (`drive = 1.0`). Setting `self.fb_lp.set_cutoff(feedback_corner_hz(drive), self.sr)` at base `sr` places $f_c$ above Nyquist for `drive < 0.16` and runs `fb_lp` *after* `self.os.process` instead of inside the 4× oversampled loop at `4.0 * self.sr` (`192 kHz`).
  2. `TubeScreamer::new` calls `ts.set_drive(0.0)` (and `GraphicEq::new` calls `eq.rebuild(..., 0.5)`), which calls `SmoothedGain::set` during construction and consumes `primed = true`. The first `process()` call with a non-zero knob then ramps over ~35 ms instead of snapping to the initial value.
- **Recommended Fix:** Clock `fb_lp` inside `self.os.process` at `4.0 * self.sr`, and do not set `primed = true` until the first sample in `process()`.

---

### 2.8 Static linear pre-clip biquads in `Distortion` (4×) and `MetalCore` (8×) run inside the oversampler loop · **MEDIUM**
- **Locations:** `src/dsp/effects/distortion.rs:79, 84, 126-129` and `src/dsp/effects/metal_core.rs:83, 88, 147-155`
- **Root Cause:** `pre_clip_hp` (130/150 Hz) and `pre_clip_lp` (4500/3800 Hz) are linear filters placed before the first nonlinearity, yet they are evaluated 4× or 8× per sample inside `os.process`. At `sr8 = 8 * 96 kHz = 768 kHz`, a 150 Hz biquad has $\omega_0 = 0.00123\text{ rad}$ ($1 - \cos\omega_0 = 7.5 \times 10^{-7} \approx 6\,\epsilon_{\text{f32}}$), causing severe `f32` pole quantization noise while burning 4×–8× excess biquad CPU.
- **Recommended Fix:** Design and run `pre_clip_hp` and `pre_clip_lp` at base rate `sr` immediately *before* `self.os.process(...)`.

---

### 2.9 `UniVibe` unsmoothed `mode` step on Chorus ↔ Vibrato switch & 4× redundant `powf` per sample · **MEDIUM**
- **Location:** `src/dsp/effects/uni_vibe.rs:93-102, 107-111`
- **Root Cause:**
  1. `let wet_amt = mix + (1.0 - mix) * mode;` applies `mode` *after* `self.mix.step()`, causing an instantaneous 50% step when toggling `MODE` between Chorus (`0.0`) and Vibrato (`1.0`). Passing `mix + (1.0 - mix) * mode` into `self.mix.set(...)` smooths both `mix` and `mode` with the existing smoother.
  2. `F_MIN * (F_MAX / F_MIN).powf(depth * lfo)` does not depend on `spread`, yet sits inside the 4-stage loop (calling `powf` 4× per sample instead of once).

---

### 2.10 `NoiseGate` starts open (`gain: 1.0`), `RELEASE` only sets hold time, and fixed 5 ms close ramp chops low notes · **MEDIUM**
- **Location:** `src/dsp/effects/noise_gate.rs:42, 51-55, 94-109`
- **Root Cause:**
  1. `NoiseGate::new` initializes `gain: 1.0` on `envelope: 0.0`, leaking ~20 ms of noise on startup before closing.
  2. `self.hold_left` is not clamped when `release` is turned down while `open == false`.
  3. `CLOSE_MS = 5.0` ms is hardcoded and shorter than one period of a low E string (12.1 ms), chopping decaying notes abruptly once hold expires.
- **Recommended Fix:** Initialize `gain: 0.0`, clamp `self.hold_left = self.hold_left.min(max_hold)` on every sample, and scale `close_coeff` with the `release` knob (e.g., 15–150 ms release slope after a shorter hold).

---

## Stage 3 — Amplifier Models & Tone Stack (`src/dsp/amp/`, `src/dsp/tonestack.rs`)

### 3.1 `GlobalNfb::feedback` has an inverted `bleed` filter and a 12-sample oversampler delay inside the loop · **CRITICAL**
- **Location:** `src/dsp/amp/mod.rs:631-665`
- **Code:**
  ```rust
  pub fn new(sr: f32, beta: f32, lf_corner: f32) -> Self {
      Self {
          lf: Biquad::lowpass(sr, lf_corner, 0.707),
          bleed: Biquad::highpass(sr, 4000.0, 0.707),
          beta,
      }
  }
  pub fn feedback(&mut self, v_out: f32, presence: f32) -> f32 {
      let shaped = v_out - self.lf.process(v_out);
      let bled = shaped - self.bleed.process(shaped);
      let bleed_amt = (presence - 0.5) * 2.0;
      let looped = shaped - bleed_amt * bled;
      self.beta * looped
  }
  ```
- **Root Cause:**
  1. **Inverted filter band:** `self.bleed` is `Biquad::highpass(sr, 4000.0, 0.707)`. Therefore `bled = shaped - self.bleed.process(shaped)` is `(I - HP_4kHz)(shaped)`, which is the **low/mid band below 4 kHz**, not the high frequencies! Above 4 kHz, `self.bleed.process(shaped) ≈ shaped`, so `bled ≈ 0` and `looped ≈ shaped` **regardless of `presence`**. Moving `presence` from `0.0` to `1.0` actually removes negative feedback from the **midrange** (`lf_corner` to `4 kHz`) while leaving feedback above 4 kHz unchanged.
  2. **12-sample FIR group delay inside the feedback loop:** In every amp's `power_amp`, `self.pf_out` is captured *after* `self.os_power.shape(x - fb, ...)`. `Oversampler8` has 11 base-rate samples of linear-phase FIR group delay (+ 1 sample for `pf_out` = **12 samples** total loop delay, or 250 µs at 48 kHz). A 12-sample delay at 48 kHz has $180^\circ$ phase lag at **2 kHz** (and $360^\circ$ at 4 kHz, $540^\circ$ at 6 kHz). Subtracting `fb` therefore creates **positive feedback at 2 kHz, 6 kHz, 10 kHz** and notches at 4 kHz, 8 kHz — turning `GlobalNfb` into a 12-sample comb filter!
- **Recommended Fix:**
  - Change `bled` to use the highpass output directly (`let bled = self.bleed.process(shaped);`, or use a 1st-order shelf in the divider).
  - Close the NFB loop around a zero-delay (or single-oversampled-sample) power-stage clipper *inside* the `os_power` callback (or take `pf_out` pre-decimation / model the closed-loop transfer directly) so the 11-sample FIR interpolator/decimator group delay is outside the feedback loop.

---

### 3.2 `Randall` wires `GlobalNfb::feedback` to `treble` (`knobs[3]`) instead of `presence` (`knobs[4]`) · **HIGH**
- **Location:** `src/dsp/amp/randall.rs:188-189, 266`
- **Code:**
  ```rust
  let treble = knobs[3];
  let presence = knobs[4];
  ...
  let fb = self.nfb.feedback(self.pf_out, treble);
  ```
- **Root Cause:** `Randall` has a dedicated `PRESENCE` knob at `knobs[4]`, but line 266 passes `treble` instead of `presence` to `self.nfb.feedback(...)`.
- **Recommended Fix:** Pass `presence` (`knobs[4]`) to `self.nfb.feedback(self.pf_out, presence)`.

---

### 3.3 `AmpModel::prev()` skips `AmpModel::Tweed` · **HIGH**
- **Location:** `src/dsp/mod.rs:114-126`
- **Code:**
  ```rust
  pub fn prev(self) -> Self {
      match self {
          Self::Marshall => Self::Supro,
          ...
          Self::Tweed => Self::Supro,
      }
  }
  ```
- **Root Cause:** `Self::Marshall.prev()` returns `Self::Supro` instead of `Self::Tweed` (both `Marshall` and `Tweed` return `Supro`).
- **Audio / UI Impact:** Cycling backwards through amp models skips `Tweed Deluxe` completely (`Marshall.prev().next() != Marshall`).
- **Recommended Fix:** Change `Self::Marshall => Self::Supro` to `Self::Marshall => Self::Tweed` in `AmpModel::prev()`, and add a round-trip test `for m in AmpModel::ALL { assert_eq!(m.next().prev(), m); assert_eq!(m.prev().next(), m); }`.

---

### 3.4 Switching built-in `AmpModel`s has no declick ramp and replays frozen `Oversampler8` / `Fender` `SpringReverb` tails · **HIGH**
- **Location:** `src/dsp/amp/mod.rs:958-1018`, `src/dsp/mod.rs:1890-1901`
- **Root Cause:** Unlike `CabBank` (which clears incoming cab state and crossfades over 6 ms), `AmpBank::process` switches models mid-block on a single sample with no crossfade and without clearing the incoming amp's `Oversampler8` histories or `Fender`'s internal `SpringReverb` tank (`20` allpasses + `2` feedback springs). Switching away from `Fender` while reverb is ringing freezes the spring tank; switching back to `Fender` later replays the old chord's reverb tail and clicks on the step discontinuity.
- **Recommended Fix:** Mirror `CabBank`'s `live` / `fading` 6 ms crossfade in `AmpBank` and clear the incoming amp's delay/oversampler/spring histories (while preserving rectifier sag `envelope` per `AGENTS.md`).

---

### 3.5 `OutputTransformer::process` runs a sharp Gaussian zero-crossing notch at base sample rate without oversampling · **HIGH**
- **Location:** `src/dsp/amp/mod.rs:466-478`
- **Code:**
  ```rust
  let low = self.lf.process(x);
  let high = x - low;
  let y = (low * self.drive).tanh() / self.drive + high;
  let cross = self.xover * y * (-(y * y) * 25.0).exp();
  y - cross
  ```
- **Root Cause:**
  1. `cross = self.xover * y * (-25.0 * y * y).exp()` is a narrow Gaussian notch around $y = 0$ ($\sigma \approx 0.14$) applied at the **base sample rate** outside `os_power`. Every zero crossing generates high-order odd harmonics that alias back into the audio band.
  2. `self.lf` is a 2nd-order Butterworth LP (`Q = 0.707`), which has $-90^\circ$ phase shift at `lf_corner`. Subtracting `high = x - low` and compressing `low` leaves an uncancelled out-of-phase component at `lf_corner`.
- **Recommended Fix:** Move the class-AB crossover nonlinearity inside `os_power`'s 8× oversampled closure, and use a 1st-order complementary split (`OnePoleLp`) for the LF core saturation path.

---

### 3.6 `Vox` and `Hiwatt` pass `p = 1.0` to `split_gain`, making stage-2 gain `k2` a constant `-11 dB` attenuator · **MEDIUM**
- **Locations:** `src/dsp/amp/vox.rs:24, 235-242` and `src/dsp/amp/hiwatt.rs:18, 281-288` (plus `randall.rs:221`)
- **Root Cause:** In `split_gain(total, pregain, front_load, p)`, when `p = 1.0`, `k1 = front_load * pregain` and `k2 = (PREAMP_GAIN_COEFF * pregain) / k1 = const` (`0.288` on Vox, `0.271` on Hiwatt). Because stage 1 is bounded by `[-1, 1]`, stage 2's input never exceeds `±0.45` and **can never clip at any `GAIN` setting**. Additionally, `vox.rs`, `hiwatt.rs`, and `randall.rs` call `split_gain` 8 times per sample *inside* the `for` loop over `up.iter()`, and `vox.rs:240` / `hiwatt.rs:287` have dead `let _ = k2;` statements.
- **Recommended Fix:** Hoist `split_gain` outside the 8× loop in `vox.rs`, `hiwatt.rs`, and `randall.rs`, remove the dead `let _ = k2;`, and use `p < 1.0` (e.g. `0.7–0.85`) if stage 2 is intended to contribute harmonic breakup at high gain.

---

### 3.7 `Supro` and `Tweed` (US 60 Hz amps) initialize `SupplyRipple` at 100 Hz (UK mains) instead of 120 Hz · **MEDIUM**
- **Location:** `src/dsp/amp/supro.rs:123` and `src/dsp/amp/tweed.rs:114`
- **Root Cause:** `AmpModel::mains_hz` (`src/dsp/mod.rs:96`) classifies `Supro` and `Tweed` as 60 Hz US amps (whose full-wave rectified ripple is `120.0 Hz`), but both constructors pass `100.0` Hz to `SupplyRipple::new`.
- **Recommended Fix:** Change `100.0` to `120.0` in `Supro::new` and `Tweed::new`.

---

### 3.8 `DynamicPresence` sums `x + g * hp.process(x)` with a 2nd-order HP, causing phase cancellation below the corner · **MEDIUM**
- **Location:** `src/dsp/amp/mod.rs:403-436`
- **Root Cause:** `self.hp` is a 2nd-order Butterworth highpass (`Biquad::highpass`), which shifts phase by $+90^\circ$ at `freq` and $+180^\circ$ below `freq`. Summing `x + g * self.hp.process(x)` causes destructive phase cancellation an octave below the presence corner (1.5–2.5 kHz) when `g > 0`.
- **Recommended Fix:** Use a 1st-order highpass (`x - lp.process(x)`) or a `Biquad::high_shelf` so the presence lift is minimum-phase without a midrange cancellation dip.

---

### 3.9 `ToneStack` stores un-normalized `y1..y3` before `makeup` and runs 432 trig calls per knob move · **MEDIUM**
- **Location:** `src/dsp/tonestack.rs:217, 221-254`
- **Root Cause:**
  1. `ToneStack::process` stores the passive (un-normalized) output `y` in `self.y1..y3` and multiplies by `self.makeup` only on return (`y * self.makeup`). When a tone knob changes the network's passive attenuation, `b0..b3` and `makeup` rescale inversely while `y1..y3` remain at the old un-normalized scale, creating a transient jump.
  2. `peak_magnitude` evaluates 72 frequency points calling `cos`/`sin` 6 times per point (`432` trig calls on the audio thread per knob move), and does not clamp its upper bound `18_000.0` below `0.5 * self.sr`.
- **Recommended Fix:** Pre-multiply `self.b0..b3` by `makeup` inside `update_coefficients` (so `y` and `y1..y3` are always unity-normalized and `process` saves a multiply), and use a complex rotor recurrence in `peak_magnitude` clamped to `0.49 * self.sr`.

---

## Stage 4 — Cabinet Simulation, Convolution & Post-Amp Stereo Rack FX

### 4.1 `SpeakerDrive::disp_lp` is designed at base `sr` (`100 Hz`) but clocked at `4 × sr`, shifting the cone-excursion cutoff to `400 Hz` · **CRITICAL**
- **Location:** `src/dsp/cab/mod.rs:217, 260-265`
- **Code:**
  ```rust
  // In SpeakerDrive::new(sr):
  disp_lp: Biquad::lowpass(sr, DISP_FC, DISP_Q), // DISP_FC = 100.0
  ...
  // In SpeakerDrive::process(x):
  let up = self.os.upsample(x);
  for (o, &u) in down.iter_mut().zip(up.iter()) {
      let d = self.disp_lp.process(u).clamp(-1.5, 1.5);
  ```
- **Root Cause:** `disp_lp` is initialized with `sr` instead of `sr * DRIVE_OS_RATE` (`4.0 * sr`), but is clocked 4 times per base sample inside the `Oversampler4` loop. Its actual cutoff frequency is therefore $4 \times 100\text{ Hz} = \mathbf{400\text{ Hz}}$! (Meanwhile, the offline diagnostic `speaker_drive_stats` at `cab/mod.rs:609` clocks `disp_lp` at base rate `sr`, hiding the bug from the analysis tool.)
- **Audio Impact:** Midrange guitar fundamentals up to 400–600 Hz pass through `disp_lp` and drive speaker $Bl$ motor droop (`1 / (1 + BL_DROOP_K * d²)`) and Doppler modulation (`DOPPLER_DEPTH * d`), causing single-note midrange lines to self-modulate and intermodulate treble instead of only low-frequency bass excursion doing so.
- **Recommended Fix:** Initialize `disp_lp: Biquad::lowpass(sr * DRIVE_OS_RATE, DISP_FC, DISP_Q)` in `SpeakerDrive::new` (or clock `disp_lp` once per base sample before `self.os.upsample(x)`).

---

### 4.2 `MicPosition::clear()` sets `last_pos = 0.0` without resetting filter coefficients, breaking `mic_pos == 0.0` (Edge) · **HIGH**
- **Location:** `src/dsp/cab/mod.rs:843-876, 1038-1049`
- **Code:**
  ```rust
  pub fn clear(&mut self) {
      self.l.clear();
      self.r.clear();
      self.last_pos = 0.0;
  }
  ```
- **Root Cause:** `MicPosition::new` tunes `l` and `r` to neutral (`pos = 0.5`, 0 dB shelves, `comb_g = 0.0`) and sets `last_pos = -1.0` to force the first `set(pos)` to run. However, `CabBank::process` calls `self.clear_cab(model)` on boot (`self.live == None`) and on every cab switch, which calls `MicPosition::clear()` — setting `self.last_pos = 0.0` without touching the filter coefficients! If the user sets `mic_pos = 0.0` (Edge), `self.mic.set(0.0)` checks `if (pos - self.last_pos).abs() <= 0.001 { return; }` (`0.0 == 0.0`) and returns without ever applying the Edge voicing.
- **Recommended Fix:** Set `self.last_pos = -1.0;` in `MicPosition::clear()` so the next `set(pos)` call always retunes the filters to the current `pos`.

---

### 4.3 `read_cubic` reads unwritten `buf[write]` (the oldest sample in the ring buffer) whenever `1.0 < delay < 2.0` · **HIGH**
- **Location:** `src/dsp/effects/mod.rs:100-117` (called by `Delay`, `Flanger`, `Chorus`)
- **Code:**
  ```rust
  let d = delay.clamp(1.0, (len - 3) as f32);
  let i = d.floor() as usize;
  let t = d - i as f32;
  let idx = |k: usize| buf[(write + len - k) % len];
  let (p0, p1, p2, p3) = (idx(i - 1), idx(i), idx(i + 1), idx(i + 2));
  ```
- **Root Cause:** `Delay`, `Flanger`, and `Chorus` call `read_cubic` **before** writing the current sample into `buf[write]`. At that moment, `idx(1) = buf[write - 1]` is the newest sample $x[n-1]$, while `idx(0) = buf[write]` has **not been overwritten yet** and holds the **oldest sample in the entire buffer** $x[n - \text{len}]$ (up to 500 ms old in `Delay`!). Whenever `1.0 < d < 2.0` (`i = 1, t > 0`), `p0 = idx(0) = buf[write]` is mixed into the cubic polynomial with weight $-\frac{1}{2}t(1-t)^2$.
- **Recommended Fix:** Clamp `delay` to `[2.0, (len - 3) as f32]` for read-before-write buffers, or write `buf[write]` before calling `read_cubic` (with delay offset adjusted by +1 sample so `idx(0)` is always the current sample $x[n]$).

---

### 4.4 `Flanger` adds `min_ms` twice (`span = min_ms + sweep_ms * depth`), so `depth = 0.0` still sweeps · **HIGH**
- **Location:** `src/dsp/effects/flanger.rs:105, 115-120`
- **Code:**
  ```rust
  let span = min_ms + sweep_ms * depth;
  ...
  let del_l = (min_ms + span * lfo(self.phase)) * self.sr / 1000.0;
  ```
- **Root Cause:** `min_ms` is added to `span` and added a second time to `del_l` (compare `chorus.rs:67-69`: `let span = SWEEP_MS * depth; let del_l = (MIN_MS + span * lfo(self.phase))...`).
  - At `depth = 0.0`, `span = 0.5 ms`, so the flanger still sweeps from `0.5 ms` to `1.0 ms` instead of staying stationary.
  - At `depth = 1.0`, the max delay reaches `2 * min_ms + sweep_ms = 5.5 ms` instead of `5.0 ms`.
  - In Electric Mistress mode (`mistress == true`), crossing `depth = 0.05` unfreezes `lfo` while `span` already contains `min_ms = 0.4 ms`, causing a sudden delay jump.
- **Recommended Fix:** Change line 105 to `let span = sweep_ms * depth;`.

---

### 4.5 Toggling `cab_external_active` has no declick ramp and replays stale convolver audio in both directions · **HIGH**
- **Location:** `src/dsp/mod.rs:1917-1933`, `src/dsp/cab/external.rs:207-251`, `src/dsp/cab/mod.rs:1038-1049`
- **Root Cause:**
  1. `ExternalIrCab` has no `clear()` method: toggling `cab_external_active` `false → true` immediately emits 128 stale samples from `out_buf` and up to `LIVE_MAX_IR_LEN = 8192` samples (~170 ms) of old FDL spectra.
  2. While `ext_cab` is active (or while `route.skip_cab` is `true` for a full-rig AU), `CabBank` is not clocked, but `self.cab.live` stays `Some(model)`. When switching back to the built-in cab, `CabBank::process` sees `self.live == Some(model)`, skips `clear_cab(model)`, and skips the crossfade ramp — replaying 93 ms of stale FDL spectra with a hard single-sample cut.
- **Recommended Fix:** Add `ExternalIrCab::clear()`, track whether `ext_cab` (or `skip_cab`) was active on the previous sample, clear the incoming cab path on transition, and apply the 6 ms declick crossfade.

---

### 4.6 `Delay` buffer lacks headroom for wow/flutter at `time = 1.0`, does not smooth `time`, and leaves filter state in `clear_buffers()` · **MEDIUM**
- **Location:** `src/dsp/effects/delay.rs:71, 104-110, 127, 146, 167`
- **Root Cause:**
  1. `max_samples = (sr * 0.5) as usize + 1` (`24001` at 48 kHz), and `read_cubic` clamps to `(len - 3) as f32 = 23998.0`. At `time = 1.0` (`base = 24000`), Tape and Echorec wow/flutter (`wobble` up to `1.0037`, requesting `24089` samples) is clamped flat at `23998`, killing the LFO modulation at max delay time.
  2. `base = time * self.sr * 0.5` is unsmoothed, so a knob step or tap-tempo update jumps the read tap discontinuously into the feedback loop.
  3. `Delay::clear_buffers()` zeroes `buf_l`/`buf_r` but does not `.clear()` the 6 feedback biquads (`damp_l/r`, `ec_lp_l/r`, `ec_hp_l/r`) or `sat_os_l/r`.
- **Recommended Fix:** Allocate `max_samples = (sr * 0.52) as usize + 8`, smooth `base` with a one-pole smoother, and clear the feedback biquads and oversamplers in `clear_buffers()`.

---

### 4.7 Sample-rate (`sr != 48000`) bugs in Cab IR synthesis, `ExternalIrCab`, `Reverb`, and `SpringReverb` · **MEDIUM**
- **Locations:**
  1. **`src/dsp/cab/ir.rs:25, 145-161`:** Modal and scatter decaying sinusoids `*out += g * s` have DTFT peak magnitude $\propto g \cdot \text{sr}$ while the biquad impulse body scales as $1/\text{sr}$ — making cab modal resonances **+6 dB louder at 96 kHz** than at 48 kHz. Also, `Texture::predelay` (`96..150` samples) is specified in raw samples instead of ms, halving room-mic predelay and stereo Haas width at 96 kHz.
  2. **`src/dsp/cab/external.rs:193-201`:** `normalize_pair` normalizes L2 energy ($\sqrt{\sum h[n]^2} = 1$) after resampling to `target_sr`, which scales passband gain by $\sqrt{\text{target\_sr} / 48000}$ (+3 dB louder at 96 kHz for the same WAV file).
  3. **`src/dsp/effects/reverb.rs:58-59` & `src/dsp/effects/spring.rs:87, 146`:** One-pole loop damping coefficients `damp1` and `damp` are raw per-sample constants independent of `sr`, doubling the damping cutoff frequency in Hz at 96 kHz vs 48 kHz.
- **Recommended Fix:** Scale modal amplitude `g` in `ir::synth` by `48_000.0 / sr`, scale `Texture::predelay` by `sr / 48_000.0`, scale `normalize_pair` by `(48_000.0 / sr).sqrt()`, and derive `damp` one-pole coefficients from `sr` via $d(\text{sr}) = d_{48\text{k}}^{48000 / \text{sr}}$.

---

### 4.8 `Tremolo` jumps 4 ms in latency when `pitch_depth` crosses `0.0` & missing `clear_buffers()` on reorder · **MEDIUM**
- **Locations:** `src/dsp/effects/tremolo.rs:54-62, 103-110` and `src/dsp/mod.rs:2253-2261`
- **Root Cause:**
  1. `Tremolo::process` bypasses its delay line (`0` latency) when `pitch_depth == 0.0` and switches in a single sample to reading at `CENTER_MS = 4.0 ms` (192 samples at 48 kHz) as soon as `pitch_depth > 0.0`, using 2-point linear interpolation (`Tremolo::read`) instead of `read_cubic`.
  2. `DspChain::snapshot_order` (`src/dsp/mod.rs:2256-2259`) clears `reverb`, `delay`, `flanger`, and `phaser` on chain reorder, but does not clear `pitch` (43 ms buffer), `chorus` (22 ms buffer), `tremolo` (8 ms buffer), or `Fender`'s `SpringReverb`.

---

## Stage 5 — Master Bus, Limiter, Bypass Ramps & Offline Export

### 5.1 Loop-region export (`range_ticks: Some(...)`) zeroes out all clip audio prior to `win_start` · **CRITICAL**
- **Location:** `src/export.rs:354-366`
- **Code:**
  ```rust
  for clip in &decoded {
      let clip_end = clip.start + clip.mono.len();
      let lo = (written).max(clip.start).max(win_start);
      let hi = (written + n).min(clip_end).min(win_end);
      if lo < hi {
          for out in lo..hi {
              mono[out - written] += clip.mono[out - clip.start] * clip.gain;
          }
      }
  }
  chain.process_block(&mono[..n], &mut left[..n], &mut right[..n]);
  ```
- **Root Cause:** The loop-region exporter runs `written` from `0` up to `win_end` specifically so the signal chain is warmed by the session audio leading into `win_start` (`src/export.rs:11-12, 279-280`). However, clamping `lo` with `.max(win_start)` inside the **input clip-summing loop** makes `lo >= win_start >= hi` for every block where `written + n <= win_start`!
- **Audio Impact:** `mono[..n]` is filled with `0.0` for the entire timeline `[0, win_start)`. Any take playing before `win_start` is muted prior to the loop start, so its reverb tail, delay repeats, cab convolution, and amp sag are completely missing at the start of the exported WAV (while the thread still burns CPU processing silence from `0` to `win_start`).
- **Recommended Fix:** Remove `.max(win_start)` and `.min(win_end)` from lines 357–358 (`let lo = written.max(clip.start); let hi = (written + n).min(clip_end);`), keeping the `win_start..win_end` clamp only on the WAV sample-writing loop at lines 372–374.

---

### 5.2 `Limiter` has a 0.265 dB step discontinuity at threshold (`env == ceiling`) and 16.9-sample shorter effective lookahead · **HIGH**
- **Location:** `src/dsp/limiter.rs:44-47, 127, 206-210`
- **Code:**
  ```rust
  let target = if self.env > self.ceiling {
      self.ceiling * MARGIN / self.env
  } else {
      1.0
  };
  ```
- **Root Cause:**
  1. **Discontinuous gain computer:** At `self.env = self.ceiling^-`, `target = 1.0` (output peak `ceiling = 0.891`). At `self.env = self.ceiling^+`, `target` jumps discontinuously from `1.0` to `MARGIN = 0.97` (`-0.265 dB`, pulling the output down to `0.864`). When `env` hovers near `ceiling`, `target` chatters by `0.265 dB`.
  2. **Lookahead offset:** Line 127 sets `let line = look.saturating_sub(OS_CORNER_DELAY).max(1);` (`72 - 11 = 61` samples at 48 kHz). However, `self.gain` multiplies `self.delay_l[self.pos]` **before** `self.corner_l` (line 230), whereas `det_l.upsample(li)` delays the detector peak by `5.875` base-rate samples. Thus the effective lookahead is `72 - 11 - 5.875 = 55.1` samples (and `<= 0` at low sample rates).
- **Recommended Fix:** Open the limiter knee smoothly at `let knee = self.ceiling * MARGIN; let target = if self.env > knee { knee / self.env } else { 1.0 };`, and size `delay_l` to `look` (`72` samples) so the gain multiplier receives the full 1.5 ms lookahead.

---

### 5.3 `Biquad` does not clamp `freq < 0.5 * sr`, causing pole instability (`|p| > 1`, `NaN` blowup) if `freq > Nyquist` · **HIGH**
- **Location:** `src/dsp/biquad.rs:67-140`
- **Root Cause:** None of `hp_coeffs`, `lp_coeffs`, `low_shelf_coeffs`, `high_shelf_coeffs`, `peak_eq_coeffs`, or `bandpass` clamps `freq` below `0.5 * sr`. If `freq > 0.5 * sr` (e.g., on a 16 kHz audio device where amps/cabs instantiate 8–10 kHz shelves/lowpasses), $\omega_0 = 2\pi f / \text{sr} \in (\pi, 2\pi) \implies \sin(\omega_0) < 0 \implies \alpha < 0 \implies a_2 = \frac{1-\alpha}{1+\alpha} > 1.0$, placing the poles outside the unit circle and causing immediate exponential blowup to `±inf` / `NaN`.
- **Recommended Fix:** Clamp `let freq = freq.clamp(1.0, sr * 0.495);` and `let q = q.max(1e-4);` at the entry of all `Biquad` coefficient constructors.

---

### 5.4 Bypass crossfade (`bypass_ramp`) starts at `0.0` on boot, comb-filters oversampled stages, and freezes stale `SmoothedGain` values · **MEDIUM**
- **Location:** `src/dsp/mod.rs:1776, 2161-2220`
- **Root Cause:**
  1. `DspChain::new` initializes `bypass_ramp: [0.0; CHAIN_LEN]` instead of matching `params.stage_enabled(stage)`, so any stage enabled at startup fades in from dry over the first 6 ms.
  2. When a stage is bypassed (`g == 0.0`), `process()` is not called. If a user lowers a pedal's `drive`/`gain` knob while it is bypassed (or loads a preset), its internal `SmoothedGain` remains frozen at the old pre-bypass value (e.g., `117×` on TS-808) and blasts for ~8 ms when the pedal is re-enabled.
  3. Linearly mixing 0-sample dry `d` with 12-sample-delayed oversampled wet `w` during the 6 ms ramp creates a moving comb filter notch at ~2 kHz.

---

### 5.5 Built-in ↔ AU `declick_gain` mutes remainder of block, leaves `comp_delay` stale, and `replace_ext_amp` skips state sync · **MEDIUM**
- **Location:** `src/dsp/mod.rs:1814-1821, 2319-2434`
- **Root Cause:**
  1. `replace_ext_amp` (called on the audio thread) replaces `self.ext_amp` without syncing `self.applied_ext_amp` or `self.declick_gain`.
  2. `self.applied_ext_amp` only flips at a block boundary (`if self.declick_gain <= 0.0 && self.applied_ext_amp != want`), so on a 1024-frame ALSA block, `declick_gain` hits `0.0` at frame 192 and outputs 832 frames of dead silence before switching on the next callback.
  3. `comp_delay` is not clocked while `use_ext_amp == true`, replaying frozen pre-AU samples when switching back to the built-in amp.

---

### 5.6 `limit_decay` is applied per callback instead of per sample (102–410 s decay!) and `out_env` precedes `take_chain` · **MEDIUM**
- **Location:** `src/audio/mod.rs:1011, 1153-1159, 1207-1209, 1397`
- **Root Cause:**
  1. `limit_decay = (-1.0 / (0.4 * sr)).exp()` (`0.999948` at 48 kHz) is a **per-sample** decay coefficient, but `self.limit_indicator *= self.limit_decay;` at line 1011 runs **once per callback**, stretching the 0.4 s decay to **102.4 seconds** (at 256 frames) or **409.6 seconds** (at 1024 frames).
  2. Line 1207 triggers `limit_indicator = 1.0` at `> 0.95` (the old `soft_limit` knee) instead of `> 1.0` (`final_limiter`'s ceiling).
  3. `self.out_env` is computed at line 1153 *before* `take_chain` and `final_limiter` on `0.5 * (out_left + out_right)` (so timeline take playback does not move the main `OUT` meter bar, and out-of-phase stereo content cancels in the meter).
- **Recommended Fix:** Decay `limit_indicator` per sample (or raise `limit_decay` to the `frames` power `self.limit_decay.powi(frames as i32)`), trigger `limit_indicator` from `final_limiter.take_gr_db() < 0.0`, and update `self.out_env` post-`output_stage` using `out_left.abs().max(out_right.abs())`.

---

### 5.7 `Oversampler<N>` per-phase DC ripple, `HumNoise` phasor drift, and `master_bus` docstring mismatch · **LOW**
- **Locations:** `src/dsp/oversample.rs:82-90`, `src/dsp/mod.rs:1658-1668, 2442-2472`
- **Root Cause:**
  1. `Oversampler::new` normalizes the grand sum of all $MN$ prototype taps to $N$ rather than normalizing each of the $N$ polyphase branches $\sum_{j=0}^{M-1} h[p + jN] = 1$ individually, leaving a small periodic gain ripple at $f_s$ across the $N$ upsampled phases.
  2. `HumNoise::next()` rotates `(c, s)` by `(cos_w, sin_w)` in `f32` without renormalizing $c^2 + s^2 = 1$, allowing the phasor amplitude to drift over multi-hour sessions (`let g = 1.5 - 0.5 * (c * c + s * s);` keeps it exact).
  3. `master_bus` docstring (`src/dsp/mod.rs:2442-2444`) and `src/export.rs:370` state that `output` is applied *after* `master_limiter`, whereas `src/dsp/mod.rs:2467-2472` applies `output` *before* `master_limiter`.

---

## Suggested Fix Phasing

1. **Phase 1 — Critical Signal-Path & Gain-Staging Fixes (Immediate):**
   - **1.1** Fix `PickupClass::P90` target in `src/audio/calibration.rs`.
   - **1.2** Route `Looper` through `take_chain` (or post-chain stereo) in `src/audio/mod.rs`.
   - **3.1 & 3.2** Fix `GlobalNfb::feedback` highpass bleed & loop delay in `src/dsp/amp/mod.rs`, and wire `presence` in `src/dsp/amp/randall.rs`.
   - **4.1 & 4.2** Fix `SpeakerDrive::disp_lp` sample rate (`sr * DRIVE_OS_RATE`) and `MicPosition::clear()` (`last_pos = -1.0`) in `src/dsp/cab/mod.rs`.
   - **5.1** Remove `.max(win_start)` / `.min(win_end)` from the input mix loop in `src/export.rs`.
2. **Phase 2 — High-Impact Pedal, Cab & Limiter Defects:**
   - **2.1–2.6** Calibrate `Wah` / `Compressor` thresholds for DI levels, fix `Fuzz` `last_guitar` & origin kinks, fix `DS-1` `/ gain.sqrt()`, add `SmoothedGain` to MIDI/drive/level controls, and scale `Pitch` grain with `sr`.
   - **3.3–3.5** Fix `AmpModel::prev()`, add `AmpBank` switch crossfade/clear, and oversample `OutputTransformer` crossover.
   - **4.3–4.5** Fix `read_cubic` index bound, `Flanger` double `min_ms`, and `ExternalIrCab` toggle declick/clear.
   - **5.2–5.3** Fix `Limiter` threshold step discontinuity and clamp `Biquad` frequencies below Nyquist.
3. **Phase 3 — Sample-Rate Parity, Metering & Polish:**
   - **1.3–1.7, 2.7–2.10, 3.6–3.9, 4.6–4.8, 5.4–5.7**.
