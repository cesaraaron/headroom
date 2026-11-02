use super::{SmoothedGain, OnePoleLp, param_changed};
use crate::dsp::biquad::Biquad;
use crate::dsp::oversample::Oversampler4;

/// ProCo RAT (LM308 op-amp + silicon clippers + the reverse "Filter" control).
///
/// Signal path:
///   input coupling → 60 Hz HP → gain stage → [4× OS: op-amp/silicon clip] →
///   fixed 6.5 kHz low-pass → reverse tone low-pass → level
///
/// The RAT sits between the DS-1 and a Big Muff: more sustain and low-end than a
/// DS-1, less mid scoop than a Muff. The **Filter** knob is wired backwards on
/// the real pedal — turning it up *cuts* the highs — so `filter = 1` is the
/// darkest setting here too. That reverse mapping is the pedal's defining
/// control behaviour, not a UI quirk.
pub struct Rat {
    sr: f32,
    dc_block: Biquad,
    input_hp: Biquad,
    os: Oversampler4,
    /// Fixed output-stage low-pass (the RAT's 6.5 kHz roll-off).
    fixed_lp: OnePoleLp,
    /// The reverse Filter control.
    filter: OnePoleLp,
    last_filter: f32,
    drive: SmoothedGain,
    level: SmoothedGain,
}

impl Rat {
    pub fn new(sr: f32) -> Self {
        let mut r = Self {
            sr,
            dc_block: Biquad::highpass(sr, 10.0, 0.707),
            input_hp: Biquad::highpass(sr, 60.0, 0.707),
            os: Oversampler4::new(sr),
            fixed_lp: OnePoleLp::new(),
            filter: OnePoleLp::new(),
            last_filter: -1.0,
            drive: SmoothedGain::with_ms(1.0, 8.0, sr),
            level: SmoothedGain::with_ms(1.0, 8.0, sr),
        };
        r.fixed_lp.set_cutoff(6500.0, sr);
        r.set_filter(0.0);
        r
    }

    /// The real Filter knob is reverse-wired: 0 = open (6.5 kHz), 1 = dark
    /// (~520 Hz).
    fn set_filter(&mut self, filter: f32) {
        let f = filter.clamp(0.0, 1.0);
        let corner = 6500.0 * (0.08_f32).powf(f);
        self.filter.set_cutoff(corner, self.sr);
        self.last_filter = f;
    }

    #[inline]
    pub fn process(&mut self, sample: f32, dist: f32, filter: f32, level: f32) -> f32 {
        if param_changed(filter, self.last_filter) {
            self.set_filter(filter);
        }

        let x = self.dc_block.process(sample);
        let x = self.input_hp.process(x);

        self.drive.set(1.0 + dist.clamp(0.0, 1.0) * 60.0);
        let gain = self.drive.step();

        let x = self.os.process(x, |u| rat_clip(u * gain));

        let x = self.fixed_lp.process(x);
        let x = self.filter.process(x);

        self.level.set(level.clamp(0.0, 1.0));
        x * 0.32 * self.level.step()
    }
}

/// Op-amp into silicon diodes: a hard-ish clip with a tanh shoulder so the
/// corner is not a perfect square (which would alias badly even at 4×).
#[inline]
fn rat_clip(u: f32) -> f32 {
    let x = u * 2.0;
    let hard = x.clamp(-1.0, 1.0);
    (0.75 * hard + 0.25 * x.tanh()) * 0.8
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: f32 = 48_000.0;

    fn render(dist: f32, filter: f32) -> Vec<f32> {
        let mut r = Rat::new(SR);
        (0..4800)
            .map(|n| {
                let t = n as f32 / SR;
                // A 4 kHz probe on top of the fundamental, so the Filter knob has
                // high-frequency content to remove.
                let x = (2.0 * PI * 220.0 * t).sin() * 0.4
                    + (2.0 * PI * 4000.0 * t).sin() * 0.2;
                r.process(x, dist, filter, 0.6)
            })
            .collect()
    }

    #[test]
    fn output_is_finite_and_bounded() {
        for &d in &[0.0, 0.5, 1.0] {
            let out = render(d, 0.0);
            assert!(
                out.iter().all(|x| x.is_finite() && x.abs() < 4.0),
                "RAT blew up at dist {d}"
            );
        }
    }

    /// The Filter knob is reverse-wired: turning it up must remove high
    /// frequencies, not add them.
    #[test]
    fn filter_darkens_as_it_turns_up() {
        let hf = |filter: f32| {
            let out = render(0.8, filter);
            // First-difference energy is a cheap proxy for the top end.
            out.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / out.len() as f32
        };
        let open = hf(0.0);
        let dark = hf(1.0);
        assert!(
            dark < open * 0.6,
            "filter did not darken: open {open:.4}, dark {dark:.4}"
        );
    }
}
