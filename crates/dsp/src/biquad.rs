//! 双二次フィルター（RBJ Audio EQ Cookbook の式）。
//!
//! 係数を変えるときは、指定サンプル数かけて直線補間する。急に係数を切り替えると
//! プチノイズが出るため。

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
}

/// 正規化済み係数（a0 = 1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coefs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Coefs {
    pub const IDENTITY: Coefs = Coefs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// RBJ の式で係数を計算する。周波数はナイキストの少し手前に収める。
    pub fn design(kind: FilterKind, freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> Coefs {
        let freq = freq.clamp(10.0, sample_rate * 0.49);
        let q = q.clamp(0.05, 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a = 10f32.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterKind::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            FilterKind::LowShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + sq),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - sq),
                    (a + 1.0) + (a - 1.0) * cos + sq,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - sq,
                )
            }
            FilterKind::HighShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + sq),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - sq),
                    (a + 1.0) - (a - 1.0) * cos + sq,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - sq,
                )
            }
            FilterKind::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            FilterKind::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
        };
        Coefs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// 周波数 `freq` での振幅応答（dB）。画面のEQカーブ描画とプリアンプ計算に使う。
    pub fn magnitude_db(&self, freq: f32, sample_rate: f32) -> f32 {
        let w = 2.0 * PI * freq / sample_rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = den_re * den_re + den_im * den_im;
        10.0 * (num / den.max(1e-30)).max(1e-30).log10()
    }

    fn lerp(&self, other: &Coefs, t: f32) -> Coefs {
        Coefs {
            b0: self.b0 + (other.b0 - self.b0) * t,
            b1: self.b1 + (other.b1 - self.b1) * t,
            b2: self.b2 + (other.b2 - self.b2) * t,
            a1: self.a1 + (other.a1 - self.a1) * t,
            a2: self.a2 + (other.a2 - self.a2) * t,
        }
    }
}

/// 1 チャンネル分のフィルター（転置直接形 II）。
#[derive(Debug, Clone)]
pub struct Biquad {
    coefs: Coefs,
    from: Coefs,
    to: Coefs,
    ramp_pos: u32,
    ramp_len: u32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn new(coefs: Coefs) -> Self {
        Self {
            coefs,
            from: coefs,
            to: coefs,
            ramp_pos: 0,
            ramp_len: 0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn identity() -> Self {
        Self::new(Coefs::IDENTITY)
    }

    /// 係数を `ramp` サンプルかけて新しい値に変える。
    pub fn set_coefs(&mut self, coefs: Coefs, ramp: u32) {
        if ramp == 0 {
            self.coefs = coefs;
            self.from = coefs;
            self.to = coefs;
            self.ramp_len = 0;
            self.ramp_pos = 0;
            return;
        }
        self.from = self.coefs;
        self.to = coefs;
        self.ramp_len = ramp;
        self.ramp_pos = 0;
    }

    pub fn coefs(&self) -> Coefs {
        self.to
    }

    pub fn reset_state(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        if self.ramp_len > 0 {
            self.ramp_pos += 1;
            let t = self.ramp_pos as f32 / self.ramp_len as f32;
            self.coefs = self.from.lerp(&self.to, t.min(1.0));
            if self.ramp_pos >= self.ramp_len {
                self.coefs = self.to;
                self.ramp_len = 0;
            }
        }
        let c = &self.coefs;
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        // 非正規化数による速度低下を防ぐ
        if self.z1.abs() < 1e-20 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-20 {
            self.z2 = 0.0;
        }
        y
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine_gain(filter: &mut Biquad, freq: f32) -> f32 {
        let n = 48_000;
        let mut peak = 0.0f32;
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f32 / SR).sin();
            let y = filter.process_sample(x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn peaking_hits_gain_at_center() {
        let c = Coefs::design(FilterKind::Peaking, 3000.0, 6.0, 1.0, SR);
        assert!((c.magnitude_db(3000.0, SR) - 6.0).abs() < 0.05);
        assert!(c.magnitude_db(100.0, SR).abs() < 0.3);
        let mut f = Biquad::new(c);
        assert!((sine_gain(&mut f, 3000.0) - 6.0).abs() < 0.1);
    }

    #[test]
    fn shelves_reach_gain() {
        let ls = Coefs::design(FilterKind::LowShelf, 120.0, -6.0, 0.707, SR);
        assert!((ls.magnitude_db(20.0, SR) + 6.0).abs() < 0.3);
        assert!(ls.magnitude_db(5000.0, SR).abs() < 0.1);
        let hs = Coefs::design(FilterKind::HighShelf, 8000.0, 4.0, 0.707, SR);
        assert!((hs.magnitude_db(20000.0, SR) - 4.0).abs() < 0.4);
        assert!(hs.magnitude_db(100.0, SR).abs() < 0.1);
    }

    #[test]
    fn highpass_cuts_low() {
        let c = Coefs::design(FilterKind::HighPass, 80.0, 0.0, 0.707, SR);
        assert!((c.magnitude_db(80.0, SR) + 3.0).abs() < 0.2);
        assert!(c.magnitude_db(20.0, SR) < -20.0);
        assert!(c.magnitude_db(1000.0, SR).abs() < 0.1);
    }

    #[test]
    fn ramp_is_stable_and_ends_on_target() {
        let a = Coefs::design(FilterKind::Peaking, 200.0, -12.0, 2.0, SR);
        let b = Coefs::design(FilterKind::Peaking, 8000.0, 12.0, 0.5, SR);
        let mut f = Biquad::new(a);
        f.set_coefs(b, 480);
        for i in 0..2000 {
            let y = f.process_sample(((i * 7919) % 13) as f32 / 13.0 - 0.5);
            assert!(y.is_finite() && y.abs() < 20.0);
        }
        assert_eq!(f.coefs(), b);
    }
}
