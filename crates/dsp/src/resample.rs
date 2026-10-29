//! 48kHz と 16kHz の変換（話者認識モデルは 16kHz で動くため）。

use std::f32::consts::PI;

const TAPS: usize = 63;

fn lowpass_taps(cutoff_norm: f32) -> Vec<f32> {
    // 窓付き sinc（ブラックマン窓）。cutoff_norm はサンプルレートに対する比。
    let m = (TAPS - 1) as f32;
    let mut taps: Vec<f32> = (0..TAPS)
        .map(|i| {
            let n = i as f32 - m / 2.0;
            let sinc = if n == 0.0 {
                2.0 * cutoff_norm
            } else {
                (2.0 * PI * cutoff_norm * n).sin() / (PI * n)
            };
            let w = 0.42 - 0.5 * (2.0 * PI * i as f32 / m).cos()
                + 0.08 * (4.0 * PI * i as f32 / m).cos();
            sinc * w
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    for t in taps.iter_mut() {
        *t /= sum;
    }
    taps
}

/// 48kHz → 16kHz（1/3 間引き）。
pub struct Decimator3 {
    taps: Vec<f32>,
    hist: Vec<f32>,
    phase: usize,
}

impl Default for Decimator3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Decimator3 {
    pub fn new() -> Self {
        Self {
            // 16kHz のナイキスト 8kHz より少し下で切る
            taps: lowpass_taps(7_200.0 / 48_000.0),
            hist: vec![0.0; TAPS],
            phase: 0,
        }
    }

    /// 入力を処理し、出力を `out` に追加する。
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &x in input {
            self.hist.rotate_left(1);
            self.hist[TAPS - 1] = x;
            self.phase += 1;
            if self.phase == 3 {
                self.phase = 0;
                let y: f32 = self
                    .hist
                    .iter()
                    .zip(self.taps.iter().rev())
                    .map(|(h, t)| h * t)
                    .sum();
                out.push(y);
            }
        }
    }
}

/// 16kHz → 48kHz（3 倍補間）。
pub struct Interpolator3 {
    taps: Vec<f32>,
    hist: Vec<f32>,
}

impl Default for Interpolator3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpolator3 {
    pub fn new() -> Self {
        let mut taps = lowpass_taps(7_200.0 / 48_000.0);
        for t in taps.iter_mut() {
            *t *= 3.0;
        }
        Self {
            taps,
            hist: vec![0.0; TAPS],
        }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &x in input {
            for k in 0..3 {
                self.hist.rotate_left(1);
                self.hist[TAPS - 1] = if k == 0 { x } else { 0.0 };
                let y: f32 = self
                    .hist
                    .iter()
                    .zip(self.taps.iter().rev())
                    .map(|(h, t)| h * t)
                    .sum();
                out.push(y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn decimator_passes_voice_band_and_blocks_high() {
        let tone = |f: f32| -> Vec<f32> {
            (0..48_000)
                .map(|i| (2.0 * PI * f * i as f32 / 48_000.0).sin())
                .collect()
        };
        let mut d = Decimator3::new();
        let mut out = Vec::new();
        d.process(&tone(1000.0), &mut out);
        assert_eq!(out.len(), 16_000);
        assert!((rms(&out[1000..]) - 0.707).abs() < 0.03);

        let mut d = Decimator3::new();
        let mut out = Vec::new();
        d.process(&tone(15_000.0), &mut out);
        assert!(rms(&out[1000..]) < 0.01);
    }

    #[test]
    fn interpolator_round_trip_keeps_level() {
        let input: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * PI * 500.0 * i as f32 / 16_000.0).sin())
            .collect();
        let mut up = Interpolator3::new();
        let mut out = Vec::new();
        up.process(&input, &mut out);
        assert_eq!(out.len(), 48_000);
        assert!((rms(&out[3000..]) - 0.707).abs() < 0.03);
    }
}
