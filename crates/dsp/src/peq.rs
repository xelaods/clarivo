//! 最大 10 バンドのパラメトリックEQ。

use crate::biquad::{Biquad, Coefs, FilterKind};
use crate::smoother::LinearSmoother;
use crate::{db_to_gain, BLOCK, SAMPLE_RATE};
use serde::{Deserialize, Serialize};

pub const MAX_BANDS: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub kind: FilterKind,
    pub freq: f32,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default = "default_q")]
    pub q: f32,
}

fn default_true() -> bool {
    true
}
fn default_q() -> f32 {
    0.707
}

impl Band {
    pub fn new(kind: FilterKind, freq: f32, gain_db: f32, q: f32) -> Self {
        Self {
            enabled: true,
            kind,
            freq,
            gain_db,
            q,
        }
    }

    pub fn coefs(&self) -> Coefs {
        if !self.enabled {
            return Coefs::IDENTITY;
        }
        // ゲインが 0 のピーキング・シェルフは何もしないので恒等にする
        let gainless = matches!(
            self.kind,
            FilterKind::Peaking | FilterKind::LowShelf | FilterKind::HighShelf
        ) && self.gain_db.abs() < 1e-3;
        if gainless {
            return Coefs::IDENTITY;
        }
        Coefs::design(self.kind, self.freq, self.gain_db, self.q, SAMPLE_RATE)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EqSettings {
    #[serde(default)]
    pub enabled: bool,
    /// true のとき、最大ブースト量だけ全体を下げて音割れを防ぐ
    #[serde(default = "default_true")]
    pub auto_preamp: bool,
    /// 手動プリアンプ（dB）。auto_preamp が true でも加算される
    #[serde(default)]
    pub preamp_db: f32,
    #[serde(default)]
    pub bands: Vec<Band>,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_preamp: true,
            preamp_db: 0.0,
            bands: Vec::new(),
        }
    }
}

impl EqSettings {
    /// 全バンドを合わせた応答（dB）。
    pub fn response_db(&self, freq: f32) -> f32 {
        self.bands
            .iter()
            .take(MAX_BANDS)
            .map(|b| b.coefs().magnitude_db(freq, SAMPLE_RATE))
            .sum()
    }

    /// 20Hz〜20kHz の最大ブースト量（dB, 0 以上）。
    pub fn max_boost_db(&self) -> f32 {
        let mut max = 0.0f32;
        let steps = 240;
        for i in 0..=steps {
            let f = 20.0 * (1000.0f32).powf(i as f32 / steps as f32);
            max = max.max(self.response_db(f));
        }
        max
    }

    /// 実際にかけるプリアンプ（dB）。
    pub fn effective_preamp_db(&self) -> f32 {
        let auto = if self.auto_preamp {
            -self.max_boost_db()
        } else {
            0.0
        };
        auto + self.preamp_db
    }
}

/// 複数チャンネル用のEQ処理器。
pub struct Peq {
    channels: usize,
    filters: Vec<[Biquad; MAX_BANDS]>,
    preamp: LinearSmoother,
    /// 有効・無効の切り替えを滑らかにする（1 = EQ のみ）
    wet: LinearSmoother,
    settings: EqSettings,
    dry: Vec<f32>,
}

impl Peq {
    pub fn new(channels: usize) -> Self {
        Self {
            channels,
            filters: (0..channels)
                .map(|_| std::array::from_fn(|_| Biquad::identity()))
                .collect(),
            preamp: LinearSmoother::new(1.0, BLOCK as u32),
            wet: LinearSmoother::new(0.0, BLOCK as u32),
            settings: EqSettings::default(),
            dry: vec![0.0; BLOCK * 8],
        }
    }

    pub fn settings(&self) -> &EqSettings {
        &self.settings
    }

    /// 設定を反映する。音声スレッドから呼んでよい（係数計算のみ）。
    pub fn apply(&mut self, settings: &EqSettings) {
        for (i, band) in (0..MAX_BANDS).map(|i| (i, settings.bands.get(i))) {
            let coefs = band.map(|b| b.coefs()).unwrap_or(Coefs::IDENTITY);
            for ch in self.filters.iter_mut() {
                if ch[i].coefs() != coefs {
                    ch[i].set_coefs(coefs, BLOCK as u32);
                }
            }
        }
        self.preamp
            .set_target(db_to_gain(settings.effective_preamp_db()));
        self.wet
            .set_target(if settings.enabled { 1.0 } else { 0.0 });
        self.settings = settings.clone();
    }

    /// 平面形式（チャンネルごとのスライス）で処理する。
    pub fn process(&mut self, chans: &mut [&mut [f32]]) {
        debug_assert!(chans.len() <= self.channels);
        let n = chans.first().map(|c| c.len()).unwrap_or(0);
        if self.wet.current() == 0.0 && self.wet.is_settled() {
            return;
        }
        if self.dry.len() < n {
            // 通常は起きない（BLOCK*8 を超える長さで呼ばれた場合のみ）
            self.dry.resize(n, 0.0);
        }
        let preamp_start = self.preamp.clone();
        let wet_start = self.wet.clone();
        for (ci, buf) in chans.iter_mut().enumerate() {
            let mut pre = preamp_start.clone();
            let mut wet = wet_start.clone();
            let filters = &mut self.filters[ci];
            for s in buf.iter_mut() {
                let x = *s;
                let mut y = x * pre.next();
                for f in filters.iter_mut() {
                    y = f.process_sample(y);
                }
                let w = wet.next();
                *s = x + (y - x) * w;
            }
        }
        // 平滑器はチャンネル間で同じ進み方をさせ、最後に本体を進める
        for _ in 0..n {
            self.preamp.next();
            self.wet.next();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biquad::FilterKind;

    #[test]
    fn auto_preamp_offsets_boost() {
        let eq = EqSettings {
            enabled: true,
            auto_preamp: true,
            preamp_db: 0.0,
            bands: vec![Band::new(FilterKind::Peaking, 3000.0, 6.0, 1.0)],
        };
        assert!((eq.max_boost_db() - 6.0).abs() < 0.2);
        assert!((eq.effective_preamp_db() + 6.0).abs() < 0.2);
    }

    #[test]
    fn disabled_eq_is_transparent() {
        let mut peq = Peq::new(2);
        let mut l: Vec<f32> = (0..480).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut r = l.clone();
        let orig = l.clone();
        peq.process(&mut [&mut l, &mut r]);
        assert_eq!(l, orig);
    }

    #[test]
    fn enabled_eq_changes_signal() {
        let mut peq = Peq::new(1);
        peq.apply(&EqSettings {
            enabled: true,
            auto_preamp: false,
            preamp_db: 0.0,
            bands: vec![Band::new(FilterKind::Peaking, 1000.0, 12.0, 1.0)],
        });
        let mut energy_in = 0.0;
        let mut energy_out = 0.0;
        for blk in 0..20 {
            let mut buf: Vec<f32> = (0..480)
                .map(|i| {
                    let t = (blk * 480 + i) as f32 / 48_000.0;
                    (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.1
                })
                .collect();
            energy_in += buf.iter().map(|x| x * x).sum::<f32>();
            peq.process(&mut [&mut buf]);
            if blk > 5 {
                energy_out += buf.iter().map(|x| x * x).sum::<f32>();
            }
        }
        let _ = energy_in;
        // 12dB ブースト ≒ 振幅 4 倍 ≒ エネルギー 16 倍（14 ブロック分）
        let expected = 16.0 * 14.0 * 480.0 * 0.01 / 2.0;
        assert!((energy_out / expected - 1.0).abs() < 0.1, "{energy_out} vs {expected}");
    }
}
