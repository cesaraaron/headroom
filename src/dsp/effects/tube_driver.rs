use super::{SmoothedGain, param_changed};
use crate::dsp::biquad::Biquad;
use crate::dsp::oversample::Oversampler4;

/// B.K. Butler / Chandler Tube Driver — the starved-plate 12AX7 overdrive that
/// defined Gilmour's 1993–2016 lead sound.
///
/// Signal path:
///   input coupling → 40 Hz HP → gain stage → [4× OS: asymmetric tube clip] →
///   DC block → low shelf (120 Hz) + high shelf (2.5 kHz) → level
///
/// Unlike the TS-808 the clip is **asymmetric**: the two halves of the tube
/// saturate toward different rails, which is where the pedal's second-harmonic
/// warmth comes from. The DC that asymmetry injects is removed after the
/// clipper so it does not offset the amp's input. It is smoother and thicker
/// than a Tube Screamer and is usually run *after* a Big Muff, not before.
pub struct TubeDriver {
    sr: f32,
    dc_block: Biquad,
    input_hp: Biquad,
    os: Oversampler4,
    post_dc: Biquad,
    low_shelf: Biquad,
    high_shelf: Biquad,
    last_low: f32,
    last_high: f32,
    drive: SmoothedGain,
    level: SmoothedGain,
}

impl TubeDriver {
    pub fn new(sr: f32) -> Self {
        let mut td = Self {
            sr,
            dc_block: Biquad::highpass(sr, 10.0, 0.707),
            input_hp: Biquad::highpass(sr, 40.0, 0.707),
            os: Oversampler4::new(sr),
            post_dc: Biquad::highpass(sr, 20.0, 0.707),
            low_shelf: Biquad::low_shelf(sr, 120.0, 0.0),
            high_shelf: Biquad::high_shelf(sr, 2500.0, 0.0),
            last_low: -1.0,
            last_high: -1.0,
            drive: SmoothedGain::with_ms(1.0, 8.0, sr),
            level: SmoothedGain::with_ms(1.0, 8.0, sr),
        };
        td.set_eq(0.5, 0.5);
        td
    }

    /// The pedal's two tone controls: ±12 dB around their shelf corners, flat at
    /// 0.5.
    fn set_eq(&mut self, low: f32, high: f32) {
        self.low_shelf
            .set_low_shelf(self.sr, 120.0, (low.clamp(0.0, 1.0) - 0.5) * 24.0);
        self.high_shelf
            .set_high_shelf(self.sr, 2500.0, (high.clamp(0.0, 1.0) - 0.5) * 24.0);
        self.last_low = low;
        self.last_high = high;
    }

    #[inline]
    pub fn process(&mut self, sample: f32, drive: f32, high: f32, low: f32, level: f32) -> f32 {
        if param_changed(low, self.last_low) || param_changed(high, self.last_high) {
            self.set_eq(low, high);
        }

        let x = self.dc_block.process(sample);
        let x = self.input_hp.process(x);

        self.drive.set(1.0 + drive.clamp(0.0, 1.0) * 36.0);
        let gain = self.drive.step();

        let x = self.os.process(x, |u| tube_clip(u * gain));
        let x = self.post_dc.process(x);
        let x = self.low_shelf.process(x);
        let x = self.high_shelf.process(x);

        self.level.set(level.clamp(0.0, 1.0));
        x * 0.38 * self.level.step()
    }
}

/// Starved-plate 12AX7: a soft, asymmetric clip. The negative rail is deeper
/// than the positive, which is the pedal's even-harmonic warmth.
#[inline]
fn tube_clip(u: f32) -> f32 {
    if u >= 0.0 {
        (u * 1.35).tanh()
    } else {
        (u * 1.05).tanh() * 1.22
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: f32 = 48_000.0;

    fn render(drive: f32) -> Vec<f32> {
        let mut td = TubeDriver::new(SR);
        (0..4800)
            .map(|n| {
                let x = (2.0 * PI * 220.0 * n as f32 / SR).sin() * 0.4;
                td.process(x, drive, 0.5, 0.5, 0.6)
            })
            .collect()
    }

    #[test]
    fn output_is_finite_and_bounded() {
        for &d in &[0.0, 0.5, 1.0] {
            let out = render(d);
            assert!(
                out.iter().all(|x| x.is_finite() && x.abs() < 3.0),
                "Tube Driver blew up at drive {d}"
            );
        }
    }

    /// The starved-plate stage clips asymmetrically (unequal positive/negative
    /// peaks) but the post-clip DC block keeps the average near zero.
    #[test]
    fn clips_asymmetrically_without_dc_offset() {
        let out = render(0.9);
        let max_pos = out.iter().copied().fold(0.0_f32, f32::max);
        let max_neg = out.iter().copied().fold(0.0_f32, f32::min).abs();
        let mean = out.iter().sum::<f32>() / out.len() as f32;
        assert!(
            (max_pos - max_neg).abs() > max_pos.max(max_neg) * 0.02,
            "clip is symmetric: +{max_pos:.4} vs -{max_neg:.4}"
        );
        assert!(mean.abs() < 0.01, "DC offset survived: {mean:.4}");
    }
}
