//! Stereo-linked lookahead limiter for the master bus.
//!
//! The old master "limiter" was a memoryless rational clip: unity below a
//! `0.95` knee and a hard asymptote at 1.0. Sitting under a rig whose amps are
//! trimmed to a low RMS but whose transients are tall, it did the one thing a
//! limiter should never do — it *distorted* peaks instead of turning them down.
//! That is the harsh "saturation" you hear when a couple of pedals stack gain
//! into the ceiling.
//!
//! This is a real feed-forward limiter: it looks ahead, computes the gain that
//! keeps the output under a fixed ceiling, and applies it smoothly. Peaks are
//! turned down cleanly (gain reduction) rather than clipped, and a fixed makeup
//! gain lifts the whole program so the rig lands at a consumer-loud level.
//!
//! The detector is **stereo-linked** (one gain for both channels from the larger
//! of the two), so a hard-panned peak cannot shift the stereo image — which the
//! old per-channel clip did.

use crate::dsp::effects::lin_to_db;
use crate::dsp::oversample::Oversampler4;

/// Lookahead window, milliseconds. Long enough to open the gain before a
/// transient reaches the output, short enough to be inaudible as latency.
pub const LOOKAHEAD_MS: f32 = 1.5;

/// Gain-*recovery* time constant, milliseconds. Slow, so the gain does not pump
/// between notes.
const RELEASE_MS: f32 = 25.0;

/// Peak-hold detector release, milliseconds. Just long enough to hold a transient's
/// peak across a cycle (so the gain follows the envelope, not the waveform) but
/// short enough to release onto the sustain rather than ducking it — a long hold
/// set the gain by the loudest recent strum and pulled the whole phrase down.
const ENV_RELEASE_MS: f32 = 40.0;

/// Gain-*application* time constant, milliseconds. Must be well under the
/// lookahead so the gain is fully down by the time the peak arrives, otherwise
/// the limiter overshoots and has to clip.
const ATTACK_MS: f32 = 0.15;

/// Output ceiling as a linear sample peak — roughly −1 dBFS of headroom.
pub const CEILING: f32 = 0.891;

/// Gain-curve headroom below [`CEILING`]. The gain targets `CEILING * MARGIN`, so
/// ordinary material never reaches the clamp; the clamp stays only as an exact
/// last-resort guarantee.
const MARGIN: f32 = 0.97;

/// Fixed makeup gain applied inside the limiter so the rig lands at a
/// consumer-loud level instead of the low RMS the amp trims target. Applied
/// *before* the ceiling, so it lifts quiet material and is absorbed by the
/// limiter on loud material rather than clipping at the converter.
pub const MAKEUP: f32 = 1.189; // +1.5 dB

/// Base-rate lag of the 4× true-peak detector: its interpolator's group delay.
/// The lookahead line is lengthened by this so the gain still anticipates the
/// full [`LOOKAHEAD_MS`] after the detector's own delay is accounted for.
const DETECTOR_LAG: usize = 6;

/// The limiter's gain target for a peak-hold envelope.
///
/// The knee opens at `ceiling * MARGIN`, not at `ceiling`: feeding the raw
/// ceiling to the divider put a 0.265 dB step at `env == ceiling` (1.0 below,
/// `MARGIN` above), so a signal sitting on the threshold chattered.
#[inline]
fn gain_target(env: f32, ceiling: f32) -> f32 {
    let knee = ceiling * MARGIN;
    if env > knee { knee / env } else { 1.0 }
}

/// Round-trip peak delay of the corner oversampler, in base-rate samples.
///
/// The linear-phase derivation gives (M·N−1)/N = 11.75, but the decimated
/// impulse peak lands 11 samples after the corner input (measured — the
/// up/decimate phase alignment puts the peak a quarter-sample early of the
/// group-delay center, and sample-indexed accounting needs the peak, not the
/// center). The lookahead line is shortened by exactly this, so the impulse
/// emerges at the same sample as before and `latency()` is unchanged.
/// Pinned by `corner_preserves_latency_and_ceiling`.
const OS_CORNER_DELAY: usize = 11;

/// A stereo-linked lookahead limiter. Real-time safe: all buffers are
/// preallocated by [`Limiter::new`], and `process` neither allocates nor blocks.
pub struct Limiter {
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    pos: usize,
    /// Output ceiling. The master bus uses [`CEILING`] (−1 dBFS); the final output
    /// stage uses 1.0 (it sums already-limited buses).
    ceiling: f32,
    /// Makeup applied before the ceiling. The master bus uses [`MAKEUP`]; the final
    /// stage applies none.
    makeup: f32,
    /// Current gain, 1.0 when open. Applied to the delayed signal.
    gain: f32,
    /// Peak-hold envelope of the (makeup'd) input. The gain computer runs off this,
    /// not the raw sample: following the waveform made the gain dip at every cycle's
    /// peak and lag it, so the output overshot and the safety clamp base-rate-clipped
    /// the peaks on a hot signal — audible as a hiss on transients.
    env: f32,
    attack: f32,
    release: f32,
    env_release: f32,
    /// 4× interpolators for the detector feed: the envelope follower runs off
    /// the inter-sample peak, not the sample peak. Histories are per-channel
    /// (they cannot interleave); preallocated here, touched per sample.
    det_l: Oversampler4,
    det_r: Oversampler4,
    /// How many base-rate samples the 4× corner had to touch. Should stay 0
    /// on ordinary material — if it moves, the detector has regressed and the
    /// corner is doing base-rate-clipping work at high rate instead.
    clamp_hits: u64,
    /// 4× clamp for the corner (see `process`): the gained, delayed signal is
    /// clamped per high-rate sample and decimated back, so inter-sample overs
    /// are turned down instead of base-rate-clipped. Preallocated here.
    corner_l: Oversampler4,
    corner_r: Oversampler4,
    /// Minimum applied `gain` since the last [`take_gr_db`](Self::take_gr_db).
    /// Plain field on the audio thread; the take resets it.
    min_gain: f32,
}

impl Limiter {
    pub fn new(sr: f32) -> Self {
        Self::with_ceiling(sr, CEILING, MAKEUP)
    }

    /// A limiter for the final output stage: a full-scale ceiling, no makeup (it
    /// receives the sum of already-limited buses).
    pub fn output_stage(sr: f32) -> Self {
        Self::with_ceiling(sr, 1.0, 1.0)
    }

    fn with_ceiling(sr: f32, ceiling: f32, makeup: f32) -> Self {
        let look = ((sr * LOOKAHEAD_MS / 1000.0).round() as usize).max(1);
        let sr = sr.max(1.0);
        let release = 1.0 - (-1.0 / (RELEASE_MS / 1000.0 * sr)).exp();
        // The corner below delays the signal by OS_CORNER_DELAY base-rate
        // samples; shorten the line by that, and lengthen it by the detector's
        // own lag, so the gain anticipates the full `look` window on the signal
        // it actually multiplies.
        let line = (look + DETECTOR_LAG).saturating_sub(OS_CORNER_DELAY).max(1);
        Self {
            delay_l: vec![0.0; line],
            delay_r: vec![0.0; line],
            pos: 0,
            ceiling,
            makeup,
            gain: 1.0,
            env: 0.0,
            attack: 1.0 - (-1.0 / (ATTACK_MS / 1000.0 * sr)).exp(),
            release,
            env_release: (-1.0 / (ENV_RELEASE_MS / 1000.0 * sr)).exp(),
            det_l: Oversampler4::new(sr),
            det_r: Oversampler4::new(sr),
            clamp_hits: 0,
            corner_l: Oversampler4::new(sr),
            corner_r: Oversampler4::new(sr),
            min_gain: 1.0,
        }
    }

    /// Latency the limiter adds, in samples: the lookahead, plus the detector's
    /// lag and the corner's round trip (the line is sized to cancel the latter
    /// and compensate the former).
    pub fn latency(&self) -> usize {
        self.delay_l.len() + OS_CORNER_DELAY
    }

    /// Gain reduction applied since the last take, in dB (≤ 0.0; exactly 0.0
    /// when the limiter never engaged — `lin_to_db` floors tiny gains instead
    /// of going `-inf`, so this is always finite). Resets the latch: call once
    /// per block/callback. Audio-thread-safe by construction (plain field,
    /// same thread that runs `process`).
    pub fn take_gr_db(&mut self) -> f32 {
        let gr = lin_to_db(self.min_gain);
        self.min_gain = 1.0;
        gr
    }

    /// Clear the lookahead and open the gain. Used when the limiter is
    /// re-seated (not on a normal parameter change).
    pub fn reset(&mut self) {
        self.delay_l.fill(0.0);
        self.delay_r.fill(0.0);
        self.pos = 0;
        self.gain = 1.0;
        self.env = 0.0;
        self.det_l.reset();
        self.det_r.reset();
        self.corner_l.reset();
        self.corner_r.reset();
        self.min_gain = 1.0;
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let li = l * self.makeup;
        let ri = r * self.makeup;
        // True-peak detector: the max over the 4× phases (and both channels —
        // still one linked value), so an over between base-rate samples pulls
        // the gain down instead of sailing through to the safety clamp.
        let up_l = self.det_l.upsample(li);
        let up_r = self.det_r.upsample(ri);
        let mut peak = 0.0f32;
        for &u in up_l.iter().chain(up_r.iter()) {
            peak = peak.max(u.abs());
        }
        // Peak-hold envelope: instant attack, ~40 ms release. The gain computer runs
        // off this, *not* the raw sample — following the waveform made the gain dip
        // at every cycle's peak and lag it, so the output overshot and the safety
        // clamp base-rate-clipped those peaks (an audible hiss on transients).
        self.env = if peak > self.env {
            peak
        } else {
            self.env * self.env_release
        };
        // Gain that keeps the envelope at the ceiling; 1.0 when already under it.
        // `MARGIN` leaves a hair of headroom, so the gain's residual lag never even
        // reaches the clamp on ordinary material.
        let target = gain_target(self.env, self.ceiling);
        let coeff = if target < self.gain {
            self.attack
        } else {
            self.release
        };
        self.gain += (target - self.gain) * coeff;
        if self.gain < self.min_gain {
            self.min_gain = self.gain;
        }

        // Oversampled corner: clamp the gained signal per high-rate sample and
        // decimate back, so an over between base-rate samples is turned down
        // instead of base-rate-clipped. Counts one hit per base-rate sample
        // where any phase clamped (ordered comparisons, so NaN still counts
        // as nothing — same as the old absolute-value check).
        let ceil = self.ceiling;
        let mut hit = false;
        let yl = self
            .corner_l
            .process(self.delay_l[self.pos] * self.gain, |u| {
                let c = u.clamp(-ceil, ceil);
                hit = hit || c < u || c > u;
                c
            });
        let yr = self
            .corner_r
            .process(self.delay_r[self.pos] * self.gain, |u| {
                let c = u.clamp(-ceil, ceil);
                hit = hit || c < u || c > u;
                c
            });
        if hit {
            self.clamp_hits += 1;
        }
        // Backstop: decimation can ring microscopically over the ceiling; keep
        // the exact guarantee (and the harness's hard sample-peak bound) with
        // a base-rate clamp that stays silent on ordinary material.
        let dl = yl.clamp(-self.ceiling, self.ceiling);
        let dr = yr.clamp(-self.ceiling, self.ceiling);
        self.delay_l[self.pos] = li;
        self.delay_r[self.pos] = ri;
        self.pos += 1;
        if self.pos == self.delay_l.len() {
            self.pos = 0;
        }
        (dl, dr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The safety clamp must never fire on ordinary hot material. When it did, it
    /// base-rate-clipped every cycle's peak (measured 8.6% of samples) — an audible
    /// hiss on transients. The peak-hold envelope follower is what keeps it at zero.
    #[test]
    fn the_clamp_never_fires_on_a_hot_signal() {
        let sr = 48_000.0;
        let mut lim = Limiter::new(sr);
        for i in 0..96_000 {
            let t = i as f32 / sr;
            // ~3x over the ceiling, with a fast 2 Hz tremolo of transients.
            let env = 0.4 + 0.6 * (2.0 * std::f32::consts::PI * 2.0 * t).sin().abs();
            let x = (2.0 * std::f32::consts::PI * 200.0 * t).sin() * 3.0 * env;
            lim.process(x, x);
        }
        assert_eq!(
            lim.clamp_hits, 0,
            "the limiter hard-clipped {} samples",
            lim.clamp_hits
        );
    }

    #[test]
    fn quiet_signal_passes_at_makeup() {
        let mut lim = Limiter::new(48_000.0);
        // Prime past the lookahead with a quiet signal; the output mirrors the
        // input delayed by `latency`, scaled only by the makeup gain. The +64
        // covers the corner oversamplers' histories settling after the line
        // fills (up 12 + down 24 base-rate samples of memory, plus margin) —
        // before that the decimator is still ringing from the startup step.
        let n = lim.latency() + 64;
        for _ in 0..n {
            lim.process(0.1, -0.1);
        }
        for _ in 0..100 {
            let (l, r) = lim.process(0.1, -0.1);
            assert!(
                (l - 0.1 * MAKEUP).abs() < 1e-5,
                "quiet signal must pass at makeup, got {l}"
            );
            assert!((r + 0.1 * MAKEUP).abs() < 1e-5);
        }
    }

    #[test]
    fn holds_the_ceiling_on_a_hot_signal() {
        let mut lim = Limiter::new(48_000.0);
        let mut peak = 0.0f32;
        // A full-scale square drives the detector hard; after the lookahead it
        // must be pinned at/under the ceiling.
        for i in 0..4000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let (l, r) = lim.process(x, x);
            if i > lim.latency() + 100 {
                peak = peak.max(l.abs()).max(r.abs());
            }
        }
        assert!(
            peak <= CEILING + 1e-4,
            "limiter let a sample through at {peak} (ceiling {CEILING})"
        );
    }

    #[test]
    fn stereo_linked_a_panned_peak_keeps_the_image() {
        let mut lim = Limiter::new(48_000.0);
        // Left is hot and right is silent: both channels must receive the same
        // gain, so the silent side stays silent rather than one side clipping.
        let mut l_peak = 0.0f32;
        let mut r_peak = 0.0f32;
        for i in 0..4000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let (l, r) = lim.process(x, 0.0);
            if i > lim.latency() + 100 {
                l_peak = l_peak.max(l.abs());
                r_peak = r_peak.max(r.abs());
            }
        }
        assert!(l_peak <= CEILING + 1e-4);
        assert!(r_peak < 1e-6, "the silent channel must stay silent");
    }

    #[test]
    fn latency_is_the_lookahead_window() {
        let lim = Limiter::new(48_000.0);
        // 1.5 ms lookahead at 48 kHz, plus the detector's 6-sample lag.
        assert_eq!(lim.latency(), 78);
    }

    /// The gain computer must be continuous across the threshold: the old form
    /// stepped 0.265 dB at `env == ceiling`, so a signal sitting there chattered.
    #[test]
    fn the_gain_computer_is_continuous_at_the_ceiling() {
        let c = CEILING;
        let below = gain_target(c * (1.0 - 1e-6), c);
        let above = gain_target(c * (1.0 + 1e-6), c);
        assert!(
            (below - above).abs() < 1e-3,
            "step at the ceiling: {below} vs {above}"
        );
        assert!(gain_target(c * MARGIN, c) <= 1.0 + 1e-6);
        assert!((gain_target(c * MARGIN * 2.0, c) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn recovers_after_the_peak() {
        let mut lim = Limiter::new(48_000.0);
        // Slam it, then feed silence long enough for the release to recover.
        for _ in 0..500 {
            lim.process(1.0, 1.0);
        }
        for _ in 0..48_000 {
            lim.process(0.0, 0.0);
        }
        // A quiet signal now passes at unity again.
        for _ in 0..200 {
            lim.process(0.1, 0.1);
        }
        for _ in 0..10 {
            let (l, _) = lim.process(0.1, 0.1);
            assert!((l - 0.1 * MAKEUP).abs() < 2e-3, "gain did not recover: {l}");
        }
    }

    /// The detector must catch an inter-sample over: a 12 kHz tone at 0.9
    /// started at 45° samples at ±0.707·0.9 (× MAKEUP = 0.757, under the
    /// 0.891 ceiling), while its true peak is 0.9 (× MAKEUP = 1.070, over).
    /// A sample-peak detector holds gain at 1.0; the 4× detector must pull
    /// it down within the lookahead window.
    #[test]
    fn detector_catches_an_intersample_over() {
        let sr = 48_000.0;
        let mut lim = Limiter::new(sr);
        for i in 0..4000 {
            let x = (2.0 * std::f32::consts::PI * 12_000.0 * i as f32 / sr
                + std::f32::consts::FRAC_PI_4)
                .sin()
                * 0.9;
            lim.process(x, x);
        }
        assert!(
            lim.gain < 0.95,
            "detector missed the inter-sample over: gain {}",
            lim.gain
        );
    }

    /// The corner must preserve sample-exact timing and the exact ceiling: a
    /// lone sub-ceiling impulse emerges `latency()` samples later (gain never
    /// moves — the envelope stays under the ceiling), and nothing exceeds the
    /// ceiling. This pins the shrink arithmetic (line shortened by the
    /// corner's group delay): forgetting the shrink puts the peak at 84.
    ///
    /// Characterization: passes before and after (both clamp exactly); guards
    /// the T2 wiring, not the 4× benefit itself — inter-sample overs are
    /// base-rate-invisible by design (see plan Task 2 ruling).
    #[test]
    fn corner_preserves_latency_and_ceiling() {
        let sr = 48_000.0;
        let mut lim = Limiter::new(sr);
        let n = 200usize;
        let mut out = vec![0.0f32; n];
        for (i, o) in out.iter_mut().enumerate() {
            let x = if i == 0 { 0.5 } else { 0.0 };
            // 0.5 × MAKEUP = 0.5945 < 0.891: gain stays 1.0 throughout.
            let (l, _) = lim.process(x, x);
            *o = l;
        }
        let peak_i = out
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(
            peak_i,
            lim.latency(),
            "impulse emerged at {peak_i}, latency() says {}",
            lim.latency()
        );
        assert!(
            out.iter().all(|&v| v <= CEILING),
            "output exceeds the ceiling exactly"
        );
    }

    /// The corner's round-trip group delay is compensated by shortening the
    /// lookahead line, and the detector's 6-sample lag is added back, so the
    /// gain anticipates the full 1.5 ms on the signal it multiplies. The
    /// reported total is therefore the lookahead plus 6 (78 @ 48 kHz). Four
    /// points pin the scaling (lookahead rounds per rate, the other terms are
    /// rate-independent).
    #[test]
    fn limiter_latency_is_lookahead_plus_detector_lag() {
        assert_eq!(Limiter::new(48_000.0).latency(), 78);
        assert_eq!(Limiter::new(44_100.0).latency(), 72);
        assert_eq!(Limiter::new(32_000.0).latency(), 54);
        assert_eq!(Limiter::new(96_000.0).latency(), 150);
    }

    /// Silence must read exactly 0.0 dB — finite, never negative, never NaN.
    /// The gain never leaves 1.0, so the block minimum is exactly 1.0.
    #[test]
    fn gr_readout_is_zero_on_silence() {
        let mut lim = Limiter::new(48_000.0);
        for _ in 0..48_000 {
            lim.process(0.0, 0.0);
        }
        let gr = lim.take_gr_db();
        assert!(gr.is_finite(), "GR readout is not finite: {gr}");
        assert_eq!(gr, 0.0, "silent GR must read 0.0 dB, got {gr}");
    }

    /// The readout must equal the actual applied gain: a settled hot tone
    /// converges to `CEILING·MARGIN/env`, and a hard-panned peak reduces both
    /// channels identically (stereo-linked — one value, no image shift).
    #[test]
    fn gr_readout_matches_applied_gain() {
        let sr = 48_000.0;
        let drive = |p: &mut Limiter, l: f32, r: f32| {
            for n in 0..(sr as usize * 2) {
                let x = (2.0 * std::f32::consts::PI * 220.0 * n as f32 / sr).sin() * 0.8;
                p.process(x * l, x * r);
            }
        };
        let mut both = Limiter::new(sr);
        drive(&mut both, 1.0, 1.0);
        let gr = both.take_gr_db();
        // env ≈ 0.8·MAKEUP; gain converges to CEILING·MARGIN/env.
        let expect = 20.0 * (CEILING * MARGIN / (0.8 * MAKEUP)).log10();
        assert!(
            (gr - expect).abs() < 0.05,
            "GR {gr:.3} dB != applied gain {expect:.3} dB"
        );
        // Panned either way: identical reduction, silent side stays silent.
        for (l, r) in [(1.0, 0.0), (0.0, 1.0)] {
            let mut lim = Limiter::new(sr);
            drive(&mut lim, l, r);
            let gr_pan = lim.take_gr_db();
            assert!(
                (gr_pan - expect).abs() < 0.05,
                "panned GR {gr_pan:.3} dB != {expect:.3} dB"
            );
        }
    }

    /// Reducing, then `reset()`, must reopen the readout: the latched minimum
    /// belongs to the old regime, not the new silence.
    #[test]
    fn reset_reopens_gain_and_readout() {
        let sr = 48_000.0;
        let mut lim = Limiter::new(sr);
        for _ in 0..1000 {
            lim.process(1.0, 1.0);
        }
        assert!(lim.take_gr_db() < -0.5, "hot signal did not reduce");
        lim.reset();
        for _ in 0..100 {
            lim.process(0.0, 0.0);
        }
        assert_eq!(lim.take_gr_db(), 0.0, "reset did not reopen the readout");
    }

    /// Near-ideal 4× reconstructor for TRUE-peak measurement (test apparatus
    /// only): 65-tap windowed sinc, Kaiser β=9, cutoff 0.95×Nyquist — much
    /// sharper than the runtime prototype, so Gibbs overshoot on sharp
    /// base-rate corners is visible instead of smeared.
    fn sharp_4x_peak(samples: &[f32]) -> f32 {
        fn i0(x: f64) -> f64 {
            let mut sum = 1.0;
            let mut term = 1.0;
            for k in 1..30 {
                term *= x * x / (4.0 * k as f64 * k as f64);
                sum += term;
            }
            sum
        }
        const HALF: i32 = 32;
        const FC: f64 = 0.95 * 0.5;
        let beta = 9.0;
        let i0_beta = i0(beta);
        // Kernel per quarter-phase: h_p[j] for j in -32..=32.
        let kernel = |f: f64, j: i32| {
            let x = j as f64 - f;
            let sinc = if x.abs() < 1e-9 {
                2.0 * FC
            } else {
                (2.0 * std::f64::consts::PI * FC * x).sin() / (std::f64::consts::PI * x)
            };
            let r = (x / HALF as f64).clamp(-1.0, 1.0);
            sinc * i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0_beta
        };
        let n = samples.len();
        let mut peak = 0.0f64;
        for i in 64..(n - 64) {
            for p in 0..4 {
                let f = p as f64 / 4.0;
                let mut acc = 0.0f64;
                for j in -HALF..=HALF {
                    acc += samples[(i as i32 + j) as usize] as f64 * kernel(f, j);
                }
                peak = peak.max(acc.abs());
            }
        }
        peak as f32
    }

    /// The oversampled corner must hold the TRUE peak (not just base samples):
    /// a steady ±0.864 square has single-sample edges pre-fix whose Gibbs
    /// overshoot the ceiling, and decimator-softened edges post-fix that do
    /// not. The apparatus self-checks first: a synthetic hard square must read
    /// ≥3% over (else the apparatus is blind and the test fails loudly
    /// instead of passing vacuously).
    #[test]
    fn oversampled_corner_holds_true_peak() {
        // Apparatus self-check on a synthetic hard square.
        let synth: Vec<f32> = (0..2000)
            .map(|i| {
                if (i / 120) % 2 == 0 {
                    0.86427
                } else {
                    -0.86427
                }
            })
            .collect();
        let synth_peak = sharp_4x_peak(&synth);
        assert!(
            synth_peak >= 0.86427 * 1.03,
            "apparatus blind to Gibbs overshoot: {synth_peak:.4}"
        );
        // Limiter output on a 200 Hz ±1.0 square (gain settles to ~0.727,
        // output ≈ the synthetic square, edges softened by decimation).
        let sr = 48_000.0;
        let mut lim = Limiter::new(sr);
        let mut out = Vec::with_capacity(4000);
        for i in 0..4000 {
            let x = if (i / 120) % 2 == 0 { 1.0 } else { -1.0 };
            let (l, _) = lim.process(x, x);
            out.push(l);
        }
        let lim_peak = sharp_4x_peak(&out);
        assert!(
            lim_peak <= CEILING + 0.01,
            "true peak {lim_peak:.4} escapes the ceiling {CEILING}"
        );
    }
}
