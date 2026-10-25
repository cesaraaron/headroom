//! Offline windowed-sinc resampler, shared by the external-IR loader and the
//! practice/jam-along track decoder. Both run once per load, off the audio
//! thread, when a file's sample rate differs from the engine's.
//!
//! For downsampling the cutoff and kernel width scale with `ratio` to suppress
//! aliasing. The kernel is small and the workload is one-shot, so this trades a
//! little quality for simplicity over an FFT/polyphase resampler.

use std::f32::consts::PI;

/// Resample `x` by `ratio` (output/input) with a windowed-sinc kernel. A ratio of
/// 1.0 (or an empty input) returns the input unchanged.
pub fn resample(x: &[f32], ratio: f32) -> Vec<f32> {
    if (ratio - 1.0).abs() < 1e-6 || x.is_empty() {
        return x.to_vec();
    }
    const LOBES: f32 = 16.0; // sinc lobes each side at unity ratio
    let down = ratio < 1.0;
    let cutoff = ratio.min(1.0); // < 1 when downsampling: lowers the anti-alias band
    let half = if down { LOBES / ratio } else { LOBES };

    let out_len = ((x.len() as f32) * ratio).round() as usize;
    let mut out = vec![0.0f32; out_len.max(1)];
    // Positions in f64: at ~2^23 input samples an f32 loses its fractional bits,
    // collapsing the 32-tap sinc to nearest-neighbour and then to a 2-sample
    // staircase. Backing tracks are minutes long, so this is not hypothetical.
    let ratio64 = f64::from(ratio);
    for (m, o) in out.iter_mut().enumerate() {
        let center = source_pos(m, ratio64);
        let i0 = (center - f64::from(half)).floor() as isize;
        let i1 = (center + f64::from(half)).ceil() as isize;
        let (mut acc, mut wsum) = (0.0f32, 0.0f32);
        for i in i0..=i1 {
            if i < 0 || i as usize >= x.len() {
                continue;
            }
            let t = (center - i as f64) as f32;
            let w = sinc(t * cutoff) * cutoff * lanczos(t, half);
            acc += w * x[i as usize];
            wsum += w;
        }
        *o = if wsum.abs() > 1e-9 { acc / wsum } else { 0.0 };
    }
    out
}

/// Position of output sample `m` in the input, in f64 so the fraction survives
/// past the f32 mantissa limit.
#[inline]
fn source_pos(m: usize, ratio: f64) -> f64 {
    m as f64 / ratio
}

#[inline]
fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        let px = PI * x;
        px.sin() / px
    }
}

#[inline]
fn lanczos(t: f32, half: f32) -> f32 {
    if t.abs() >= half { 0.0 } else { sinc(t / half) }
}

#[cfg(test)]
mod tests {
    /// Past ~2^23 samples the old all-f32 position lost its fraction entirely
    /// (ULP 1.0), so the sinc interpolator degenerated to nearest-neighbour.
    #[test]
    fn source_positions_keep_fractional_precision_past_f32_range() {
        let ratio = 48_000.0f64 / 44_100.0;
        let m = 10_000_001usize; // center ≈ 9.19M > 2^23
        let p = source_pos(m, ratio);
        assert!(p.fract() > 1e-4, "f64 position lost its fraction: {p}");
        // The old computation, for comparison.
        let old = (m as f32 / (48_000.0f32 / 44_100.0)).fract();
        assert_eq!(old, 0.0, "expected the old f32 collapse at this m");
    }

    use super::*;

    #[test]
    fn unity_ratio_is_a_copy() {
        let x = [0.1f32, -0.2, 0.3];
        assert_eq!(resample(&x, 1.0), x.to_vec());
    }

    #[test]
    fn upsample_grows_length() {
        let x = vec![0.0f32; 100];
        assert_eq!(resample(&x, 2.0).len(), 200);
    }

    #[test]
    fn resampled_dc_stays_flat_and_finite() {
        // A constant signal must resample to the same constant (no ringing drift).
        let x = vec![0.5f32; 256];
        let y = resample(&x, 48_000.0 / 44_100.0);
        assert!(y.iter().all(|v| v.is_finite()));
        // Interior samples settle to the input value.
        let mid = y[y.len() / 2];
        assert!((mid - 0.5).abs() < 1e-2, "DC drifted: {mid}");
    }
}
