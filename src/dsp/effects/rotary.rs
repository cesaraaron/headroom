//! Yamaha RA-200-family rotary speaker.
//!
//! The RA-200 is not a Leslie: the **speakers themselves spin**, which gives a
//! light, chorus-like swirl rather than a horn/drum wobble. This models one
//! rotating source per band with two virtual mics on opposite sides; the delay
//! is derived from the rotor angle, so the doppler pitch bend falls out of the
//! delay slope instead of being a separate LFO.

use super::{SmoothedGain, read_cubic};
use crate::dsp::biquad::Biquad;
use std::f32::consts::TAU;

/// Ring length. Base 4 ms + up to 6 ms of sweep + the cubic guard, with room.
const RING_MS: f32 = 16.0;
/// Rotor centre delay: the distance from the rotor to either mic.
const BASE_DELAY_MS: f32 = 4.0;
/// Maximum sweep either side of the centre, at DEPTH = 1.
const MAX_SWEEP_MS: f32 = 6.0;
/// Slow/fast rotor rates in Hz. The drum turns a little slower than the horn.
const SLOW_HORN_HZ: f32 = 0.80;
const FAST_HORN_HZ: f32 = 6.80;
const SLOW_DRUM_HZ: f32 = 0.70;
const FAST_DRUM_HZ: f32 = 5.70;
/// The drum is a large, smeared source: less doppler, more amplitude swing.
const DRUM_SWEEP_SCALE: f32 = 0.45;

pub struct Rotary {
    sr: f32,
    horn: Vec<f32>,
    drum: Vec<f32>,
    write: usize,
    /// Band split: the horn takes the top, the drum the bottom.
    cross_lo: Biquad,
    cross_hi: Biquad,
    /// Rotor angles (radians) and the current, ramped speeds (Hz). The angle is
    /// integrated from the speed, so a speed ramp bends the pitch through the
    /// delay slope.
    angle_h: f32,
    angle_d: f32,
    speed_h: f32,
    speed_d: f32,
    #[cfg(test)]
    pub(crate) clear_count: usize,
    last_horn_delay: f32,
    mix: SmoothedGain,
}

impl Rotary {
    pub fn new(sr: f32) -> Self {
        let ring = ((RING_MS * 0.001 * sr).ceil() as usize).max(8);
        Self {
            sr,
            horn: vec![0.0; ring],
            drum: vec![0.0; ring],
            write: 0,
            cross_lo: Biquad::lowpass(sr, 800.0, 0.707),
            cross_hi: Biquad::highpass(sr, 800.0, 0.707),
            angle_h: 0.0,
            angle_d: std::f32::consts::FRAC_PI_2,
            speed_h: SLOW_HORN_HZ,
            speed_d: SLOW_DRUM_HZ,
            #[cfg(test)]
            clear_count: 0,
            last_horn_delay: 0.0,
            mix: SmoothedGain::with_ms(1.0, 8.0, sr),
        }
    }

    /// Zero the two rotor rings after a chain reorder. The rotor angles and the
    /// mix smoother are kept: the cabinet should not jump when it is re-patched.
    pub fn clear_buffers(&mut self) {
        self.horn.fill(0.0);
        self.drum.fill(0.0);
        // The crossover biquads carry the last samples' tail too; `on_reorder`
        // means "cut the tail", so reset them as well as the rings.
        self.cross_lo.clear();
        self.cross_hi.clear();
        self.write = 0;
        #[cfg(test)]
        {
            self.clear_count += 1;
        }
    }

    /// The left-mic horn delay, in samples. Test-only: proves the delay is
    /// derived from the rotor angle (the source of the doppler bend).
    #[cfg(test)]
    fn horn_delay_now(&self) -> f32 {
        self.last_horn_delay
    }

    /// How many times the rings have been cleared. Test-only: the chain-level
    /// reorder test asserts this increments, which is unambiguous (the rotor
    /// ring is immediately refilled by the cab's own tail after a clear).
    #[cfg(test)]
    pub(crate) fn clear_count(&self) -> usize {
        self.clear_count
    }

    #[inline]
    // Positional knobs match the `stereo_stage!` macro convention; bundling them
    // into a struct would need a second dispatch path for one effect.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        l: f32,
        r: f32,
        speed: f32,
        ramp: f32,
        depth: f32,
        mix: f32,
        balance: f32,
    ) -> (f32, f32) {
        let x = 0.5 * (l + r);
        let low = self.cross_lo.process(x);
        let high = self.cross_hi.process(x);

        // Write both bands at the same cursor, then read behind it.
        self.horn[self.write] = high;
        self.drum[self.write] = low;

        // Two-speed switch, like the real cabinet — but ramped so the rotor
        // accelerates instead of jumping.
        let ramp_ms = 200.0 + ramp.clamp(0.0, 1.0) * 2800.0;
        let k = 1.0 - (-1.0 / (ramp_ms * 0.001 * self.sr)).exp();
        let (tgt_h, tgt_d) = if speed < 0.5 {
            (SLOW_HORN_HZ, SLOW_DRUM_HZ)
        } else {
            (FAST_HORN_HZ, FAST_DRUM_HZ)
        };
        self.speed_h += (tgt_h - self.speed_h) * k;
        self.speed_d += (tgt_d - self.speed_d) * k;
        self.angle_h = (self.angle_h + TAU * self.speed_h / self.sr) % TAU;
        self.angle_d = (self.angle_d + TAU * self.speed_d / self.sr) % TAU;

        let depth = depth.clamp(0.0, 1.0);
        let sweep = depth * MAX_SWEEP_MS * 0.001 * self.sr;
        let base = BASE_DELAY_MS * 0.001 * self.sr;
        let am = depth * 0.8;

        // Mic L sits at angle 0, mic R opposite. When the source faces a mic the
        // path is shortest (minimum delay) and the amplitude is highest.
        let ch = self.angle_h.cos();
        let cd = self.angle_d.cos();
        let horn_dl = base + sweep * (0.5 - 0.5 * ch);
        let horn_dr = base + sweep * (0.5 + 0.5 * ch);
        let drum_sweep = sweep * DRUM_SWEEP_SCALE;
        let drum_dl = base + drum_sweep * (0.5 - 0.5 * cd);
        let drum_dr = base + drum_sweep * (0.5 + 0.5 * cd);
        self.last_horn_delay = horn_dl;

        let hl = read_cubic(&self.horn, self.write, horn_dl);
        let hr = read_cubic(&self.horn, self.write, horn_dr);
        let dl = read_cubic(&self.drum, self.write, drum_dl);
        let dr = read_cubic(&self.drum, self.write, drum_dr);

        let horn_aml = 1.0 - am * (0.5 - 0.5 * ch);
        let horn_amr = 1.0 - am * (0.5 + 0.5 * ch);
        let drum_aml = 1.0 - am * (0.5 - 0.5 * cd);
        let drum_amr = 1.0 - am * (0.5 + 0.5 * cd);

        // Equal-power band balance, renormalised so the centre is unity.
        let b = balance.clamp(0.0, 1.0);
        let hg = b.sqrt();
        let dg = (1.0 - b).sqrt();
        let wet_l = (hl * horn_aml * hg + dl * drum_aml * dg) * std::f32::consts::SQRT_2;
        let wet_r = (hr * horn_amr * hg + dr * drum_amr * dg) * std::f32::consts::SQRT_2;

        self.write = (self.write + 1) % self.horn.len();

        self.mix.set(mix.clamp(0.0, 1.0));
        let m = self.mix.step();
        (l * (1.0 - m) + wet_l * m, r * (1.0 - m) + wet_r * m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: f32 = 48_000.0;

    /// Render a steady tone through the rotary at full wet.
    fn render(speed: f32, depth: f32, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let mut r = Rotary::new(SR);
        let n = (SR * seconds) as usize;
        let mut l = Vec::with_capacity(n);
        let mut rr = Vec::with_capacity(n);
        for i in 0..n {
            let x = (2.0 * PI * 220.0 * i as f32 / SR).sin() * 0.4;
            let (a, b) = r.process(x, x, speed, 0.5, depth, 1.0, 0.5);
            l.push(a);
            rr.push(b);
        }
        (l, rr)
    }

    #[test]
    fn fully_dry_is_passthrough() {
        let mut r = Rotary::new(SR);
        for i in 0..4800 {
            let l = (2.0 * PI * 220.0 * i as f32 / SR).sin() * 0.4;
            let rr = -l;
            let (a, b) = r.process(l, rr, 1.0, 0.5, 1.0, 0.0, 0.5);
            assert_eq!(a, l, "left dry path changed at {i}");
            assert_eq!(b, rr, "right dry path changed at {i}");
        }
    }

    #[test]
    fn output_is_finite_and_bounded() {
        let (l, r) = render(1.0, 1.0, 2.0);
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 4.0));
    }

    /// The two mics sit on opposite sides of the rotor: as one side gets louder,
    /// the other gets quieter, so the L/R envelopes must move out of phase.
    #[test]
    fn the_two_mics_swing_out_of_phase() {
        let (l, r) = render(1.0, 1.0, 2.0);
        let env = |v: &[f32]| {
            v.chunks(1024)
                .map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())
                .collect::<Vec<_>>()
        };
        let (el, er) = (env(&l), env(&r));
        let n = el.len().min(er.len());
        let ml = el[..n].iter().sum::<f32>() / n as f32;
        let mr = er[..n].iter().sum::<f32>() / n as f32;
        let mut num = 0.0;
        let (mut dl, mut dr) = (0.0, 0.0);
        for i in 0..n {
            let (a, b) = (el[i] - ml, er[i] - mr);
            num += a * b;
            dl += a * a;
            dr += b * b;
        }
        let corr = num / (dl.sqrt() * dr.sqrt()).max(1e-9);
        assert!(
            corr < -0.2,
            "mic envelopes are not out of phase (corr {corr:.3})"
        );
    }

    /// The rotor delay must move while the speed ramps. This is what makes the
    /// pitch bend real; an independent "doppler LFO" would leave it fixed.
    #[test]
    fn the_rotor_delay_moves_during_a_speed_ramp() {
        let mut r = Rotary::new(SR);
        // Settle at slow.
        for i in 0..(SR as usize / 2) {
            let x = (2.0 * PI * 220.0 * i as f32 / SR).sin() * 0.4;
            r.process(x, x, 0.0, 1.0, 1.0, 1.0, 0.5);
        }
        let slow = r.horn_delay_now();
        // Ramp to fast.
        for i in 0..(SR as usize / 2) {
            let x = (2.0 * PI * 220.0 * i as f32 / SR).sin() * 0.4;
            r.process(x, x, 1.0, 1.0, 1.0, 1.0, 0.5);
        }
        let fast = r.horn_delay_now();
        assert!(
            (fast - slow).abs() > 1.0,
            "delay did not move with the rotor: {slow:.2} -> {fast:.2} samples"
        );
    }

    #[test]
    fn clear_buffers_silences_the_wet_path() {
        let mut r = Rotary::new(SR);
        for i in 0..4800 {
            let x = (2.0 * PI * 220.0 * i as f32 / SR).sin() * 0.4;
            r.process(x, x, 1.0, 0.5, 1.0, 1.0, 0.5);
        }
        r.clear_buffers();
        let mut peak = 0.0_f32;
        for _ in 0..2048 {
            let (a, b) = r.process(0.0, 0.0, 1.0, 0.5, 1.0, 1.0, 0.5);
            peak = peak.max(a.abs()).max(b.abs());
        }
        assert!(peak < 1e-6, "wet tail survived clear_buffers: {peak}");
    }
}
