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

/// A stereo-linked lookahead limiter. Real-time safe: all buffers are
/// preallocated by [`Limiter::new`], and `process` neither allocates nor blocks.
pub struct Limiter {
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    pos: usize,
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
    /// How many samples the safety clamp had to bite. Should stay 0 on ordinary
    /// material — if it moves, the limiter is base-rate-clipping transients.
    clamp_hits: u64,
}

impl Limiter {
    pub fn new(sr: f32) -> Self {
        let look = ((sr * LOOKAHEAD_MS / 1000.0).round() as usize).max(1);
        let sr = sr.max(1.0);
        let release = 1.0 - (-1.0 / (RELEASE_MS / 1000.0 * sr)).exp();
        Self {
            delay_l: vec![0.0; look],
            delay_r: vec![0.0; look],
            pos: 0,
            gain: 1.0,
            env: 0.0,
            attack: 1.0 - (-1.0 / (ATTACK_MS / 1000.0 * sr)).exp(),
            release,
            env_release: (-1.0 / (ENV_RELEASE_MS / 1000.0 * sr)).exp(),
            clamp_hits: 0,
        }
    }

    /// Latency the lookahead adds, in samples.
    pub fn latency(&self) -> usize {
        self.delay_l.len()
    }

    /// Clear the lookahead and open the gain. Used when the limiter is
    /// re-seated (not on a normal parameter change).
    pub fn reset(&mut self) {
        self.delay_l.fill(0.0);
        self.delay_r.fill(0.0);
        self.pos = 0;
        self.gain = 1.0;
        self.env = 0.0;
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let li = l * MAKEUP;
        let ri = r * MAKEUP;
        let peak = li.abs().max(ri.abs());
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
        let target = if self.env > CEILING {
            CEILING * MARGIN / self.env
        } else {
            1.0
        };
        let coeff = if target < self.gain {
            self.attack
        } else {
            self.release
        };
        self.gain += (target - self.gain) * coeff;

        let (yl, yr) = (
            self.delay_l[self.pos] * self.gain,
            self.delay_r[self.pos] * self.gain,
        );
        if yl.abs() > CEILING || yr.abs() > CEILING {
            self.clamp_hits += 1;
        }
        // Safety clamp: with the envelope follower the gain covers every peak, so
        // this should never bite — it exists only to make the ceiling an exact
        // guarantee. If `clamp_hits` ever moves, the detector has regressed.
        let dl = yl.clamp(-CEILING, CEILING);
        let dr = yr.clamp(-CEILING, CEILING);
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
        // input delayed by `latency`, scaled only by the makeup gain.
        let n = lim.latency();
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
        // 1.5 ms at 48 kHz.
        assert_eq!(lim.latency(), 72);
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
}
