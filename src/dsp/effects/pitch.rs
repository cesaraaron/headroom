use super::{OnePoleLp, SmoothedGain, param_changed};
use std::f32::consts::TAU;

/// Pitch shifter / Whammy — a time-domain, allocation-free pitch transposer.
///
/// Sits early in the mono chain (right after the gate, before the drives) so it
/// works like a real whammy pedal: the *shifted* note is what the amp's gain stage
/// then clips. That placement is the whole point — octave-down for a fake-baritone
/// heaviness, octave-up shrieks and dive-bombs, all feeding raw into the distortion.
///
/// Algorithm: the classic two-tap crossfaded varispeed delay line ("H910"-style
/// granular shifter). Input is written to a ring buffer; two read taps sweep through
/// it at a rate set by the pitch ratio, kept half a grain apart. A raised-cosine
/// window fades each tap to silence exactly at the point where it wraps past the
/// grain boundary, so the two taps crossfade seamlessly and the wrap discontinuity
/// is inaudible. It is not phase-vocoder clean — a little warble is inherent — but
/// that roughness is part of the whammy character, and it costs almost nothing.
///
/// Knob ranges (all normalised 0–1):
///   PITCH → transpose interval: 0 = −12 st (octave down), 0.5 = unison, 1 = +12 st.
///   MIX   → dry/wet blend; 0 = dry, 1 = fully wet (full dive-bomb).
///   TONE  → low-pass on the wet path, taming the granular fizz; 0 = dark, 1 = open.
pub struct Pitch {
    buf: Vec<f32>,
    write: usize,
    /// Grain length in samples, derived from the sample rate (see [`GRAIN_SECS`]).
    grain: f32,
    /// Grain phase in [0, 1). `phase * grain` is the delay of the first read tap;
    /// the second tap reads half a grain away. Advances by `(1 − ratio)/grain`.
    phase: f32,
    tone: OnePoleLp,
    last_tone: f32,
    /// Cached transpose ratio + the knob position it was computed from. The
    /// `exp2` maps a knob-rate value, so it is recomputed only on change.
    last_pitch: f32,
    ratio: f32,
    /// Smoothed ratio: a pitch sweep must glide, not step the delay tap.
    ratio_smooth: SmoothedGain,
    /// How much the wet is faded to dry at unison (1 = fully dry), smoothed.
    unison: f32,
    unison_coeff: f32,
    /// Wet/dry, smoothed. A MIDI CC sweep of the whammy's mix would otherwise be
    /// an audio-rate staircase.
    mix: SmoothedGain,
    sr: f32,
}

/// Grain length in **seconds**. ~43 ms — long enough to hide the wrap under the
/// window, short enough that transposed transients don't smear badly. It is a
/// time, not a sample count: 2048 samples would halve to 21 ms at 96 kHz.
const GRAIN_SECS: f32 = 0.0427;

impl Pitch {
    pub fn new(sr: f32) -> Self {
        let grain = (GRAIN_SECS * sr).round();
        // Power-of-two ring at least twice the grain, plus interpolation headroom.
        let buf_len = (((grain * 2.0) as usize) + 8).next_power_of_two();
        let mut p = Self {
            buf: vec![0.0; buf_len],
            write: 0,
            grain,
            phase: 0.0,
            tone: OnePoleLp::new(),
            last_tone: -1.0,  // force first update
            last_pitch: -1.0, // force first update
            ratio: 1.0,
            ratio_smooth: SmoothedGain::with_ms(1.0, 12.0, sr),
            unison: 1.0,
            unison_coeff: 1.0 - (-1.0 / (0.010 * sr)).exp(),
            // `DEFAULT_PITCH_MIX`.
            mix: SmoothedGain::new(0.50, sr),
            sr,
        };
        p.set_tone(0.7);
        p
    }

    fn set_tone(&mut self, tone: f32) {
        // tone 0 → ~800 Hz (dark, artifacts rolled off), tone 1 → ~10 kHz (open)
        let freq = 800.0 * (10_000.0_f32 / 800.0).powf(tone.clamp(0.0, 1.0));
        self.tone.set_cutoff(freq, self.sr);
        self.last_tone = tone;
    }

    /// Interpolated read `delay` samples behind the write head (Catmull-Rom).
    #[inline]
    fn read(buf: &[f32], write: usize, delay: f32) -> f32 {
        super::read_cubic(buf, write, delay)
    }

    /// Raised-cosine grain windows for tap phase `phase` in [0, 1).
    ///
    /// The second tap sits half a grain away (`phase + 0.5 mod 1`), so
    /// `cos(θ+π) = −cosθ` and its window is exactly `1 − win_a`: one `cos`
    /// instead of two, with a unity-sum crossfade by construction.
    #[inline]
    fn grain_windows(phase: f32) -> (f32, f32) {
        let win_a = 0.5 - 0.5 * (phase * TAU).cos();
        (win_a, 1.0 - win_a)
    }

    /// `pitch` 0–1 (0.5 = unison), `mix` 0–1, `tone` 0–1.
    #[inline]
    pub fn process(&mut self, x: f32, pitch: f32, mix: f32, tone: f32) -> f32 {
        if param_changed(tone, self.last_tone) {
            self.set_tone(tone);
        }

        // Write the incoming sample first, so a zero-delay tap reads exactly `x`.
        self.buf[self.write] = x;

        // Map the knob to a transpose ratio: ±12 semitones around unison.
        // Cached: a knob-rate `exp2`, recomputed only when the knob moves.
        if param_changed(pitch, self.last_pitch) {
            let clamped = pitch.clamp(0.0, 1.0);
            let semis = (clamped - 0.5) * 24.0;
            self.ratio = (semis / 12.0).exp2();
            self.last_pitch = pitch;
        }
        self.ratio_smooth.set(self.ratio);
        let ratio = self.ratio_smooth.step();

        // At unison the tap stops drifting, so it would freeze into a static comb
        // of two half-grain-spaced copies of the input. Fade the wet to the dry
        // input there (a real whammy at unison is the dry signal).
        let target = if (ratio - 1.0).abs() < 1.0e-3 {
            1.0
        } else {
            0.0
        };
        self.unison += (target - self.unison) * self.unison_coeff;

        // The read taps must advance `ratio` samples per output sample while the
        // write head advances one, so the tap *delay* drifts by `(1 − ratio)` per
        // sample; dividing by the grain expresses that drift in phase units.
        if (ratio - 1.0).abs() < 1.0e-6 {
            // Nothing is moving the tap; walk it home so leaving unison starts
            // from zero delay.
            self.phase = (self.phase - 1.0 / self.grain).max(0.0);
        } else {
            self.phase += (1.0 - ratio) / self.grain;
            self.phase -= self.phase.floor(); // wrap into [0, 1)
        }
        let phase_b = {
            let p = self.phase + 0.5;
            p - p.floor()
        };

        let wet = {
            let tap_a = self.phase * self.grain;
            let tap_b = phase_b * self.grain;
            // Raised-cosine grain windows (single `cos`; the taps are half a
            // grain apart so their gains sum to unity — see `grain_windows`).
            let (win_a, win_b) = Self::grain_windows(self.phase);
            win_a * Self::read(&self.buf, self.write, tap_a)
                + win_b * Self::read(&self.buf, self.write, tap_b)
        };

        self.write = (self.write + 1) % self.buf.len();

        let wet = self.tone.process(wet) * (1.0 - self.unison) + x * self.unison;
        self.mix.set(mix.clamp(0.0, 1.0));
        let mix = self.mix.step();
        x * (1.0 - mix) + wet * mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: f32 = 48_000.0;

    /// Single-bin amplitude estimate via Goertzel (f64 accumulators: the recurrence
    /// sits on the unit circle and loses small bins in f32 over long windows).
    fn goertzel(samples: &[f32], f: f32, sr: f32) -> f32 {
        let w = 2.0 * std::f64::consts::PI * f as f64 / sr as f64;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &x in samples {
            let s0 = x as f64 + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let real = s1 - s2 * w.cos();
        let imag = s2 * w.sin();
        ((real * real + imag * imag).sqrt() / (samples.len() as f64 / 2.0)) as f32
    }

    /// Render a steady sine through the shifter and return the settled tail (past the
    /// initial buffer fill), asserting finiteness along the way.
    fn render(f0: f32, pitch: f32, mix: f32, tone: f32) -> Vec<f32> {
        let mut p = Pitch::new(SR);
        let n = SR as usize;
        let warmup = n / 4;
        let mut out = Vec::with_capacity(n - warmup);
        for i in 0..n {
            let x = (2.0 * PI * f0 * i as f32 / SR).sin() * 0.6;
            let y = p.process(x, pitch, mix, tone);
            assert!(y.is_finite(), "pitch non-finite at {i}");
            if i >= warmup {
                out.push(y);
            }
        }
        out
    }

    /// Center pitch does not matter for `mix = 0`: fully dry must pass through the
    /// input untouched, so the pedal is transparent when blended out.
    #[test]
    fn fully_dry_is_passthrough() {
        let mut p = Pitch::new(SR);
        for n in 0..4000 {
            let x = (n as f32 * 0.017).sin() * 0.5;
            let y = p.process(x, 1.0, 0.0, 0.5);
            assert!((y - x).abs() < 1e-6, "dry mix altered the signal at {n}");
        }
    }

    /// The grain is a time, not a sample count: the warp/window has to stay ~43 ms
    /// at every supported rate. A fixed 2048 samples would halve it at 96 kHz.
    #[test]
    fn grain_length_is_a_time_not_a_sample_count() {
        for sr in [44_100.0f32, 48_000.0, 96_000.0] {
            let p = Pitch::new(sr);
            let ms = p.grain / p.sr * 1000.0;
            assert!((ms - 42.7).abs() < 0.5, "grain {ms} ms at {sr} Hz");
        }
    }

    /// At unison the wet tap has nothing to track, so it used to freeze into a
    /// static half-grain comb. The pedal must pass the dry signal instead.
    #[test]
    fn unison_passes_the_dry_signal() {
        let mut p = Pitch::new(SR);
        let mut worst = 0.0f32;
        for n in 0..48_000 {
            let x = (2.0 * PI * 440.0 * n as f32 / SR).sin() * 0.5;
            let y = p.process(x, 0.5, 1.0, 1.0);
            if n > 24_000 {
                worst = worst.max((y - x).abs());
            }
        }
        assert!(worst < 0.01, "unison is not transparent: {worst}");
    }

    /// Extreme settings must stay finite and bounded — the unity-sum windows mean the
    /// wet tap can never exceed the buffered peak, so the blend is always well behaved.
    #[test]
    fn finite_and_bounded_under_extremes() {
        let mut p = Pitch::new(SR);
        let mut max_abs = 0.0f32;
        for n in 0..(SR as usize) {
            let x = (2.0 * PI * 110.0 * n as f32 / SR).sin() * 0.9;
            let y = p.process(x, 0.0, 1.0, 1.0);
            assert!(y.is_finite(), "non-finite at {n}");
            max_abs = max_abs.max(y.abs());
        }
        assert!(max_abs < 2.0, "pitch output out of bounds: {max_abs}");
    }

    /// Pitch fully up (+12 st) must transpose a note up an octave: the shifted
    /// fundamental at 2·f0 must dominate the original f0 in the wet output.
    #[test]
    fn octave_up_moves_the_fundamental_up() {
        let f0 = 500.0;
        let out = render(f0, 1.0, 1.0, 1.0);
        let shifted = goertzel(&out, 2.0 * f0, SR);
        let original = goertzel(&out, f0, SR);
        assert!(
            shifted > 2.0 * original,
            "octave-up weak: 2f0 {shifted:.4} vs f0 {original:.4}"
        );
    }

    /// Pitch fully down (−12 st) must transpose a note down an octave: the shifted
    /// fundamental at f0/2 must dominate the original f0.
    #[test]
    fn octave_down_moves_the_fundamental_down() {
        let f0 = 1000.0;
        let out = render(f0, 0.0, 1.0, 1.0);
        let shifted = goertzel(&out, f0 / 2.0, SR);
        let original = goertzel(&out, f0, SR);
        assert!(
            shifted > 2.0 * original,
            "octave-down weak: f0/2 {shifted:.4} vs f0 {original:.4}"
        );
    }

    /// The tone knob must roll off the wet path's brightness: shifting a bright note
    /// up an octave and opening the tone must pass more high-frequency energy than a
    /// dark setting does.
    #[test]
    fn tone_controls_wet_brightness() {
        let energy = |tone: f32| {
            let out = render(3000.0, 1.0, 1.0, tone);
            out.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>()
        };
        assert!(
            energy(0.9) > energy(0.1) * 1.5,
            "tone knob did not control wet brightness"
        );
    }

    /// The cached transpose ratio must track the knob: computed on first use and
    /// recomputed when the knob moves. A stale ratio plays the wrong interval.
    #[test]
    fn pitch_ratio_cache_tracks_the_knob() {
        let mut p = Pitch::new(SR);
        assert_eq!(p.last_pitch, -1.0, "cache must start invalid");
        for n in 0..100 {
            let x = (2.0 * PI * 440.0 * n as f32 / SR).sin() * 0.5;
            p.process(x, 0.5, 1.0, 0.7);
        }
        assert_eq!(p.last_pitch, 0.5, "first process must populate cache");
        assert!(
            (p.ratio - 1.0).abs() < 1e-6,
            "unison ratio must be 1, got {}",
            p.ratio
        );
        for n in 100..200 {
            let x = (2.0 * PI * 440.0 * n as f32 / SR).sin() * 0.5;
            p.process(x, 1.0, 1.0, 0.7);
        }
        assert_eq!(p.last_pitch, 1.0, "cache must follow the knob");
        assert!(
            (p.ratio - 2.0).abs() < 1e-6,
            "octave-up ratio must be 2, got {}",
            p.ratio
        );
    }

    /// The two grain windows must sum to unity at every phase (constant-gain
    /// crossfade) and match the direct two-cosine form — the identity the
    /// single-cosine fold relies on.
    #[test]
    fn grain_windows_sum_to_unity() {
        let (a0, b0) = Pitch::grain_windows(0.0);
        assert!(
            (a0 - 0.0).abs() < 1e-6,
            "phase 0: win_a must be 0, got {a0}"
        );
        assert!(
            (b0 - 1.0).abs() < 1e-6,
            "phase 0: win_b must be 1, got {b0}"
        );
        for i in 0..=64 {
            let ph = i as f32 / 64.0;
            let (wa, wb) = Pitch::grain_windows(ph);
            assert!(
                (wa + wb - 1.0).abs() < 1e-6,
                "windows not unity at phase {ph}: {wa} + {wb}"
            );
            assert!(wa >= -1e-6 && wb >= -1e-6, "window negative at {ph}");
            let direct_a = 0.5 - 0.5 * (ph * TAU).cos();
            let pb = {
                let q = ph + 0.5;
                q - q.floor()
            };
            let direct_b = 0.5 - 0.5 * (pb * TAU).cos();
            assert!(
                (wa - direct_a).abs() < 1e-6 && (wb - direct_b).abs() < 1e-6,
                "fold diverged from direct form at {ph}"
            );
        }
    }
}
