//! ノイズゲート、コンプレッサー、小音持ち上げ、リミッター。
//!
//! どれもチャンネル間でゲインを共通にする（ステレオの定位が崩れないように）。

use crate::{db_to_gain, gain_to_db, time_coef, SAMPLE_RATE};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// ノイズゲート
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GateSettings {
    pub enabled: bool,
    /// これより小さい音を切る（dBFS）
    pub threshold_db: f32,
    /// 閉じたときの減衰量（dB, 負の値）
    pub range_db: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
}

impl Default for GateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_db: -50.0,
            range_db: -60.0,
            attack_ms: 2.0,
            hold_ms: 120.0,
            release_ms: 150.0,
        }
    }
}

pub struct Gate {
    s: GateSettings,
    env: f32,
    gain: f32,
    hold_left: u32,
    open: bool,
    att: f32,
    rel: f32,
    env_rel: f32,
}

impl Gate {
    pub fn new(s: GateSettings) -> Self {
        let mut g = Self {
            s: s.clone(),
            env: 0.0,
            gain: 1.0,
            hold_left: 0,
            open: true,
            att: 0.0,
            rel: 0.0,
            env_rel: 0.0,
        };
        g.apply(&s);
        g
    }

    pub fn apply(&mut self, s: &GateSettings) {
        self.att = time_coef(s.attack_ms, SAMPLE_RATE);
        self.rel = time_coef(s.release_ms, SAMPLE_RATE);
        self.env_rel = time_coef(20.0, SAMPLE_RATE);
        self.s = s.clone();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn process(&mut self, chans: &mut [&mut [f32]]) {
        if !self.s.enabled {
            self.open = true;
            return;
        }
        let n = chans.first().map(|c| c.len()).unwrap_or(0);
        let open_th = db_to_gain(self.s.threshold_db);
        // 3dB のヒステリシスでバタつきを抑える
        let close_th = db_to_gain(self.s.threshold_db - 3.0);
        let floor = db_to_gain(self.s.range_db);
        let hold = (self.s.hold_ms * 0.001 * SAMPLE_RATE) as u32;
        for i in 0..n {
            let peak = chans.iter().map(|c| c[i].abs()).fold(0.0, f32::max);
            self.env = if peak > self.env {
                peak
            } else {
                self.env * self.env_rel + peak * (1.0 - self.env_rel)
            };
            if self.env >= open_th {
                self.open = true;
                self.hold_left = hold;
            } else if self.env < close_th {
                if self.hold_left > 0 {
                    self.hold_left -= 1;
                } else {
                    self.open = false;
                }
            }
            let target = if self.open { 1.0 } else { floor };
            let coef = if target > self.gain { self.att } else { self.rel };
            self.gain = target + (self.gain - target) * coef;
            for c in chans.iter_mut() {
                c[i] *= self.gain;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// コンプレッサー
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompressorSettings {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub knee_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for CompressorSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_db: -24.0,
            ratio: 3.0,
            knee_db: 6.0,
            attack_ms: 5.0,
            release_ms: 120.0,
            makeup_db: 6.0,
        }
    }
}

pub struct Compressor {
    s: CompressorSettings,
    gr_db: f32,
    att: f32,
    rel: f32,
    /// 直近のゲインリダクション量（メーター表示用、正の dB）
    pub last_reduction_db: f32,
}

impl Compressor {
    pub fn new(s: CompressorSettings) -> Self {
        let mut c = Self {
            s: s.clone(),
            gr_db: 0.0,
            att: 0.0,
            rel: 0.0,
            last_reduction_db: 0.0,
        };
        c.apply(&s);
        c
    }

    pub fn apply(&mut self, s: &CompressorSettings) {
        self.att = time_coef(s.attack_ms, SAMPLE_RATE);
        self.rel = time_coef(s.release_ms, SAMPLE_RATE);
        self.s = s.clone();
    }

    /// 入力レベル（dB）に対する静的なゲイン変化（dB, 0 以下）。
    pub fn static_curve(s: &CompressorSettings, level_db: f32) -> f32 {
        let ratio = s.ratio.max(1.0);
        let over = level_db - s.threshold_db;
        let knee = s.knee_db.max(0.0);
        let slope = 1.0 / ratio - 1.0;
        if knee > 0.0 && over.abs() <= knee / 2.0 {
            slope * (over + knee / 2.0).powi(2) / (2.0 * knee)
        } else if over > knee / 2.0 {
            slope * over
        } else {
            0.0
        }
    }

    pub fn process(&mut self, chans: &mut [&mut [f32]]) {
        if !self.s.enabled {
            self.last_reduction_db = 0.0;
            return;
        }
        let n = chans.first().map(|c| c.len()).unwrap_or(0);
        let makeup = db_to_gain(self.s.makeup_db);
        let mut max_gr = 0.0f32;
        for i in 0..n {
            let peak = chans.iter().map(|c| c[i].abs()).fold(0.0, f32::max);
            let target = Self::static_curve(&self.s, gain_to_db(peak));
            let coef = if target < self.gr_db { self.att } else { self.rel };
            self.gr_db = target + (self.gr_db - target) * coef;
            max_gr = max_gr.max(-self.gr_db);
            let g = db_to_gain(self.gr_db) * makeup;
            for c in chans.iter_mut() {
                c[i] *= g;
            }
        }
        self.last_reduction_db = max_gr;
    }
}

// ---------------------------------------------------------------------------
// 小音持ち上げ（アップワードコンプレッサー）
// ---------------------------------------------------------------------------

/// 遠くの足音など小さい音を持ち上げる。無音（floor 以下）は持ち上げない。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelerSettings {
    pub enabled: bool,
    /// この音量より小さい音を持ち上げる（dBFS）
    pub threshold_db: f32,
    /// 持ち上げの強さ（1 = なし、2 = 差を半分にする）
    pub ratio: f32,
    /// 最大の持ち上げ量（dB）
    pub max_boost_db: f32,
    /// これ以下は無音として扱い持ち上げない（dBFS）
    pub floor_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

impl Default for LevelerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -30.0,
            ratio: 2.0,
            max_boost_db: 12.0,
            floor_db: -65.0,
            attack_ms: 10.0,
            release_ms: 250.0,
        }
    }
}

pub struct Leveler {
    s: LevelerSettings,
    env: f32,
    boost_db: f32,
    env_att: f32,
    env_rel: f32,
    att: f32,
    rel: f32,
}

impl Leveler {
    pub fn new(s: LevelerSettings) -> Self {
        let mut l = Self {
            s: s.clone(),
            env: 0.0,
            boost_db: 0.0,
            env_att: 0.0,
            env_rel: 0.0,
            att: 0.0,
            rel: 0.0,
        };
        l.apply(&s);
        l
    }

    pub fn apply(&mut self, s: &LevelerSettings) {
        self.env_att = time_coef(5.0, SAMPLE_RATE);
        self.env_rel = time_coef(80.0, SAMPLE_RATE);
        self.att = time_coef(s.attack_ms, SAMPLE_RATE);
        self.rel = time_coef(s.release_ms, SAMPLE_RATE);
        self.s = s.clone();
    }

    pub fn boost_for(s: &LevelerSettings, level_db: f32) -> f32 {
        if level_db <= s.floor_db || level_db >= s.threshold_db {
            return 0.0;
        }
        let ratio = s.ratio.max(1.0);
        let under = s.threshold_db - level_db;
        (under * (1.0 - 1.0 / ratio)).min(s.max_boost_db)
    }

    pub fn process(&mut self, chans: &mut [&mut [f32]]) {
        if !self.s.enabled {
            return;
        }
        let n = chans.first().map(|c| c.len()).unwrap_or(0);
        for i in 0..n {
            let peak = chans.iter().map(|c| c[i].abs()).fold(0.0, f32::max);
            let c = if peak > self.env { self.env_att } else { self.env_rel };
            self.env = peak + (self.env - peak) * c;
            let target = Self::boost_for(&self.s, gain_to_db(self.env));
            // 大きい音が来たら素早く持ち上げをやめる
            let coef = if target < self.boost_db { self.att } else { self.rel };
            self.boost_db = target + (self.boost_db - target) * coef;
            let g = db_to_gain(self.boost_db);
            for ch in chans.iter_mut() {
                ch[i] *= g;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 先読みリミッター
// ---------------------------------------------------------------------------

pub struct Limiter {
    ceiling: f32,
    lookahead: usize,
    delay: Vec<VecDeque<f32>>,
    /// 先読み区間の必要ゲインの最小値を求めるための単調キュー（位置, ゲイン）
    mins: VecDeque<(u64, f32)>,
    pos: u64,
    gain: f32,
    att: f32,
    rel: f32,
    pub last_reduction_db: f32,
}

impl Limiter {
    /// `ceiling_db` は出力の上限（dBFS）。
    pub fn new(channels: usize, ceiling_db: f32) -> Self {
        let lookahead = (0.0015 * SAMPLE_RATE) as usize; // 1.5ms
        Self {
            ceiling: db_to_gain(ceiling_db),
            lookahead,
            delay: (0..channels)
                .map(|_| {
                    let mut d = VecDeque::with_capacity(lookahead + 1);
                    d.extend(std::iter::repeat(0.0).take(lookahead));
                    d
                })
                .collect(),
            mins: VecDeque::with_capacity(lookahead * 2 + 2),
            pos: 0,
            gain: 1.0,
            att: time_coef(0.3, SAMPLE_RATE),
            rel: time_coef(60.0, SAMPLE_RATE),
            last_reduction_db: 0.0,
        }
    }

    pub fn set_ceiling_db(&mut self, db: f32) {
        self.ceiling = db_to_gain(db);
    }

    /// 先読みによる遅延（サンプル）。
    pub fn latency(&self) -> usize {
        self.lookahead
    }

    pub fn process(&mut self, chans: &mut [&mut [f32]]) {
        let n = chans.first().map(|c| c.len()).unwrap_or(0);
        let mut max_red = 0.0f32;
        for i in 0..n {
            let peak = chans.iter().map(|c| c[i].abs()).fold(0.0, f32::max);
            let need = if peak > self.ceiling {
                self.ceiling / peak
            } else {
                1.0
            };
            while let Some(&(_, g)) = self.mins.back() {
                if g >= need {
                    self.mins.pop_back();
                } else {
                    break;
                }
            }
            self.mins.push_back((self.pos, need));
            let window_start = self.pos.saturating_sub(self.lookahead as u64);
            while let Some(&(p, _)) = self.mins.front() {
                if p < window_start {
                    self.mins.pop_front();
                } else {
                    break;
                }
            }
            let target = self.mins.front().map(|m| m.1).unwrap_or(1.0);
            let coef = if target < self.gain { self.att } else { self.rel };
            self.gain = target + (self.gain - target) * coef;
            max_red = max_red.max(-gain_to_db(self.gain));
            for (ci, c) in chans.iter_mut().enumerate() {
                let d = &mut self.delay[ci];
                d.push_back(c[i]);
                let out = d.pop_front().unwrap_or(0.0) * self.gain;
                // 立ち上がりの平滑で取りこぼした分は最後に確実に止める
                c[i] = out.clamp(-self.ceiling, self.ceiling);
            }
            self.pos += 1;
        }
        self.last_reduction_db = max_red;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_closes_on_silence_and_opens_on_signal() {
        let mut g = Gate::new(GateSettings::default());
        let mut quiet = vec![0.0005f32; 48_000];
        g.process(&mut [&mut quiet]);
        assert!(!g.is_open());
        assert!(quiet[47_999].abs() < 0.0005 * 0.01);
        let mut loud = vec![0.3f32; 4_800];
        g.process(&mut [&mut loud]);
        assert!(g.is_open());
        assert!((loud[4_799] - 0.3).abs() < 0.01);
    }

    #[test]
    fn compressor_curve() {
        let s = CompressorSettings {
            knee_db: 0.0,
            ratio: 4.0,
            threshold_db: -20.0,
            ..Default::default()
        };
        assert_eq!(Compressor::static_curve(&s, -30.0), 0.0);
        assert!((Compressor::static_curve(&s, -10.0) + 7.5).abs() < 1e-4);
    }

    #[test]
    fn leveler_only_lifts_quiet_parts() {
        let s = LevelerSettings {
            enabled: true,
            ..Default::default()
        };
        assert_eq!(Leveler::boost_for(&s, -10.0), 0.0);
        assert_eq!(Leveler::boost_for(&s, -80.0), 0.0);
        assert!((Leveler::boost_for(&s, -40.0) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn limiter_never_exceeds_ceiling() {
        let mut l = Limiter::new(2, -1.0);
        let ceiling = db_to_gain(-1.0);
        let mut a: Vec<f32> = (0..9600)
            .map(|i| (i as f32 * 0.05).sin() * if i % 2000 < 300 { 3.0 } else { 0.3 })
            .collect();
        let mut b = a.clone();
        l.process(&mut [&mut a, &mut b]);
        assert!(a.iter().chain(b.iter()).all(|x| x.abs() <= ceiling + 1e-6));
    }
}
