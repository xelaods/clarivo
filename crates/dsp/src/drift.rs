//! クロックのずれ補正。
//!
//! 仮想ケーブルと実デバイスはそれぞれ別の時計で動くため、そのままつなぐと
//! バッファが少しずつ増えるか減っていく。ここではバッファの残量を一定に保つように
//! 再生速度をごくわずか（最大 ±0.5%）変えて吸収する。補間は 3 次エルミート。

use std::collections::VecDeque;

pub struct DriftResampler {
    channels: usize,
    fifo: Vec<VecDeque<f32>>,
    /// 補間位置（fifo 先頭からの小数位置）
    frac: f64,
    target: usize,
    max_fill: usize,
    integral: f64,
    ratio: f64,
    primed: bool,
    pub underruns: u64,
    pub overruns: u64,
}

const MAX_DEVIATION: f64 = 0.005;
const KP: f64 = 0.00002;
const KI: f64 = 0.0000000015;

impl DriftResampler {
    /// `target_frames` は保ちたいバッファ残量（フレーム数）。
    pub fn new(channels: usize, target_frames: usize) -> Self {
        let cap = target_frames * 8 + 4096;
        Self {
            channels,
            fifo: (0..channels).map(|_| VecDeque::with_capacity(cap)).collect(),
            frac: 0.0,
            target: target_frames.max(8),
            max_fill: target_frames * 6 + 960,
            integral: 0.0,
            ratio: 1.0,
            primed: false,
            underruns: 0,
            overruns: 0,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// 現在のバッファ残量（フレーム数）。
    pub fn fill(&self) -> usize {
        self.fifo.first().map(|f| f.len()).unwrap_or(0)
    }

    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    pub fn clear(&mut self) {
        for f in self.fifo.iter_mut() {
            f.clear();
        }
        self.frac = 0.0;
        self.integral = 0.0;
        self.ratio = 1.0;
        self.primed = false;
    }

    /// インターリーブされた入力を追加する。
    pub fn push_interleaved(&mut self, data: &[f32], in_channels: usize) {
        let frames = data.len() / in_channels.max(1);
        for i in 0..frames {
            for c in 0..self.channels {
                // 入力のチャンネルが足りない場合：モノラルなら全チャンネルに複製、
                // それ以外（例: 2ch を 8ch に入れる）は足りない分を無音にする
                let v = if c < in_channels {
                    data[i * in_channels + c]
                } else if in_channels == 1 {
                    data[i]
                } else {
                    0.0
                };
                self.fifo[c].push_back(v);
            }
        }
        self.trim_overflow();
    }

    /// 平面形式の入力を追加する。
    pub fn push_planar(&mut self, chans: &[&[f32]]) {
        for c in 0..self.channels {
            let src = chans[c.min(chans.len() - 1)];
            self.fifo[c].extend(src.iter().copied());
        }
        self.trim_overflow();
    }

    fn trim_overflow(&mut self) {
        let fill = self.fill();
        if fill > self.max_fill {
            let drop = fill - self.target;
            for f in self.fifo.iter_mut() {
                f.drain(..drop);
            }
            self.overruns += 1;
            self.integral = 0.0;
        }
    }

    /// 平面形式の出力を `out` に書く。データが足りなければ無音を出す。
    pub fn pull(&mut self, out: &mut [&mut [f32]]) {
        let n = out.first().map(|o| o.len()).unwrap_or(0);
        if !self.primed {
            if self.fill() >= self.target {
                self.primed = true;
            } else {
                for o in out.iter_mut() {
                    o.fill(0.0);
                }
                return;
            }
        }
        // バッファ残量の誤差から速度を調整（PI 制御）
        let err = self.fill() as f64 - self.target as f64;
        self.integral = (self.integral + err * n as f64).clamp(-2.0e6, 2.0e6);
        self.ratio = (1.0 + KP * err + KI * self.integral)
            .clamp(1.0 - MAX_DEVIATION, 1.0 + MAX_DEVIATION);

        for i in 0..n {
            // エルミート補間には位置の前後 2 サンプルずつが必要
            if self.fill() < 4 {
                self.underruns += 1;
                self.primed = false;
                self.integral = 0.0;
                for o in out.iter_mut() {
                    o[i..].fill(0.0);
                }
                return;
            }
            let t = self.frac as f32;
            for (c, o) in out.iter_mut().enumerate() {
                let f = &self.fifo[c.min(self.channels - 1)];
                let (xm1, x0, x1, x2) = (f[0], f[1], f[2], f[3]);
                o[i] = hermite(xm1, x0, x1, x2, t);
            }
            self.frac += self.ratio;
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                for f in self.fifo.iter_mut() {
                    f.pop_front();
                }
            }
        }
    }
}

#[inline]
fn hermite(xm1: f32, x0: f32, x1: f32, x2: f32, t: f32) -> f32 {
    let c0 = x0;
    let c1 = 0.5 * (x1 - xm1);
    let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + c0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 入力側が 0.2% 速く動いていても、バッファ残量が目標付近に保たれること。
    #[test]
    fn keeps_fill_near_target_with_clock_skew() {
        let mut r = DriftResampler::new(1, 960);
        let mut phase = 0.0f64;
        let in_rate = 48_096.0; // +0.2%
        let mut out = vec![0.0f32; 480];
        let mut carry = 0.0f64;
        let mut fills = Vec::new();
        for _ in 0..6000 {
            // 10ms ごとに入力側の時計で生成された分だけ追加
            carry += in_rate * 0.01;
            let frames = carry.floor() as usize;
            carry -= frames as f64;
            let block: Vec<f32> = (0..frames)
                .map(|_| {
                    phase += 2.0 * std::f64::consts::PI * 440.0 / in_rate;
                    phase.sin() as f32 * 0.5
                })
                .collect();
            r.push_interleaved(&block, 1);
            fills.push(r.fill() as i64);
            r.pull(&mut [&mut out]);
        }
        // 最後の 10 秒の平均（取り出す前の残量）が目標付近にあること
        let tail = &fills[fills.len() - 1000..];
        let fill = tail.iter().sum::<i64>() / tail.len() as i64;
        assert!((fill - 960).abs() < 120, "fill {fill}");
        assert_eq!(r.overruns, 0);
        assert!(r.ratio() > 1.0);
    }

    #[test]
    fn outputs_silence_until_primed() {
        let mut r = DriftResampler::new(2, 480);
        r.push_interleaved(&[0.5; 100], 2);
        let mut l = vec![1.0f32; 64];
        let mut rr = vec![1.0f32; 64];
        r.pull(&mut [&mut l, &mut rr]);
        assert!(l.iter().all(|&x| x == 0.0));
    }
}
