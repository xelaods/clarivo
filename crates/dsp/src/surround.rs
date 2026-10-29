//! バーチャルサラウンド（7.1ch → ヘッドホン用 2ch）。
//!
//! 球形の頭のモデル（Brown & Duda, 1998）で、各スピーカー方向からの音が
//! 左右の耳に届くまでの時間差（ITD）と頭の陰による高域の減衰を再現する。
//! さらに短い初期反射を少し足し、頭の中ではなく外から聞こえるようにする。
//! 測定データを使わないため軽く、外部ライブラリも不要。

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

const HEAD_RADIUS: f32 = 0.0875; // m
const SPEED_OF_SOUND: f32 = 343.0; // m/s

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SurroundSettings {
    pub enabled: bool,
    /// 部屋の響きの量（0〜1）
    pub room: f32,
    /// 2ch 入力もスピーカー位置に置き直すか
    pub virtualize_stereo: bool,
}

impl Default for SurroundSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            room: 0.25,
            virtualize_stereo: true,
        }
    }
}

/// 7.1ch のスピーカー方位（度、正が右）。WAVEFORMATEXTENSIBLE の並び順。
pub const SPEAKER_AZIMUTHS: [f32; 8] = [-30.0, 30.0, 0.0, 0.0, -150.0, 150.0, -90.0, 90.0];
const LFE_INDEX: usize = 3;

/// 一次の頭部陰影フィルター。
#[derive(Clone)]
struct ShadowFilter {
    b0: f32,
    b1: f32,
    a1: f32,
    z: f32,
}

impl ShadowFilter {
    /// `theta` は音源と耳の軸がなす角（ラジアン、0 = 耳の正面）。
    fn new(theta: f32, sample_rate: f32) -> Self {
        let alpha_min = 0.1f32;
        let theta_min = 150.0f32.to_radians();
        let alpha = (1.0 + alpha_min / 2.0) + (1.0 - alpha_min / 2.0) * (theta / theta_min * PI).cos();
        let w0 = SPEED_OF_SOUND / HEAD_RADIUS;
        let k = 2.0 * sample_rate;
        // H(s) = (2w0 + alpha s) / (2w0 + s) を双一次変換
        let n0 = 2.0 * w0 + alpha * k;
        let n1 = 2.0 * w0 - alpha * k;
        let d0 = 2.0 * w0 + k;
        let d1 = 2.0 * w0 - k;
        Self {
            b0: n0 / d0,
            b1: n1 / d0,
            a1: d1 / d0,
            z: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z;
        self.z = self.b1 * x - self.a1 * y;
        y
    }
}

/// 固定長の遅延線（小数遅延は線形補間）。
#[derive(Clone)]
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(max: usize) -> Self {
        Self {
            buf: vec![0.0; max.next_power_of_two().max(4)],
            pos: 0,
        }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) & (self.buf.len() - 1);
        self.buf[self.pos] = x;
    }

    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let mask = self.buf.len() - 1;
        let d = delay.max(0.0);
        let i = d.floor() as usize;
        let f = d - i as f32;
        let a = self.buf[(self.pos + self.buf.len() - i) & mask];
        let b = self.buf[(self.pos + self.buf.len() - i - 1) & mask];
        a + (b - a) * f
    }
}

/// ある方位の音源から片耳への経路。
#[derive(Clone)]
struct EarPath {
    delay: f32,
    shadow: ShadowFilter,
}

#[derive(Clone)]
struct SpeakerPaths {
    line: Delay,
    left: EarPath,
    right: EarPath,
}

/// 耳の軸からの角度による時間差（Brown & Duda の式）。秒。
fn ear_delay(theta: f32) -> f32 {
    let a_c = HEAD_RADIUS / SPEED_OF_SOUND;
    if theta < PI / 2.0 {
        a_c * (1.0 - theta.cos())
    } else {
        a_c * (1.0 + theta - PI / 2.0)
    }
}

fn speaker_paths(azimuth_deg: f32, sample_rate: f32) -> SpeakerPaths {
    let az = azimuth_deg.to_radians();
    // 右耳は +90°、左耳は -90° にあるとして、それぞれの耳からの角度を求める
    let angle_to = |ear: f32| -> f32 {
        let mut d = (az - ear).abs() % (2.0 * PI);
        if d > PI {
            d = 2.0 * PI - d;
        }
        d
    };
    let tl = angle_to(-PI / 2.0);
    let tr = angle_to(PI / 2.0);
    SpeakerPaths {
        line: Delay::new(256),
        left: EarPath {
            delay: ear_delay(tl) * sample_rate,
            shadow: ShadowFilter::new(tl, sample_rate),
        },
        right: EarPath {
            delay: ear_delay(tr) * sample_rate,
            shadow: ShadowFilter::new(tr, sample_rate),
        },
    }
}

/// 短い初期反射（左右で時間をずらした 3 本のタップ、少し高域を落とす）。
struct Room {
    delay_l: Delay,
    delay_r: Delay,
    taps_l: [(f32, f32); 3],
    taps_r: [(f32, f32); 3],
    lp_l: f32,
    lp_r: f32,
}

impl Room {
    fn new(sample_rate: f32) -> Self {
        let ms = |v: f32| v * 0.001 * sample_rate;
        Self {
            delay_l: Delay::new(ms(40.0) as usize + 4),
            delay_r: Delay::new(ms(40.0) as usize + 4),
            taps_l: [(ms(7.1), 0.35), (ms(13.3), 0.25), (ms(23.7), 0.15)],
            taps_r: [(ms(8.3), 0.35), (ms(15.1), 0.25), (ms(26.9), 0.15)],
            lp_l: 0.0,
            lp_r: 0.0,
        }
    }
}

pub struct Virtualizer {
    sample_rate: f32,
    speakers: Vec<SpeakerPaths>,
    room: Room,
    settings: SurroundSettings,
}

impl Virtualizer {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            speakers: SPEAKER_AZIMUTHS
                .iter()
                .map(|&a| speaker_paths(a, sample_rate))
                .collect(),
            room: Room::new(sample_rate),
            settings: SurroundSettings::default(),
        }
    }

    pub fn apply(&mut self, s: &SurroundSettings) {
        self.settings = s.clone();
    }

    pub fn settings(&self) -> &SurroundSettings {
        &self.settings
    }

    /// 8ch 入力を左右に変換する。`active` で使われているチャンネルを示す
    /// （2ch の音源なら FL と FR だけ）。
    pub fn process(&mut self, input: &[&[f32]; 8], left: &mut [f32], right: &mut [f32]) {
        let n = left.len();
        let room_amt = self.settings.room.clamp(0.0, 1.0);
        let lp = (-2.0 * PI * 6000.0 / self.sample_rate).exp();
        for i in 0..n {
            let mut l = 0.0f32;
            let mut r = 0.0f32;
            for (ch, sp) in self.speakers.iter_mut().enumerate() {
                let x = input[ch][i];
                if ch == LFE_INDEX {
                    // 低音は方向感がないので左右に同じだけ送る
                    l += x * 0.5;
                    r += x * 0.5;
                    continue;
                }
                sp.line.push(x);
                let xl = sp.line.read(sp.left.delay);
                let xr = sp.line.read(sp.right.delay);
                l += sp.left.shadow.process(xl);
                r += sp.right.shadow.process(xr);
            }
            // 両耳に届く分を足すと音量が上がるので少し抑える
            l *= 0.7;
            r *= 0.7;
            if room_amt > 0.0 {
                let room = &mut self.room;
                room.delay_l.push(l);
                room.delay_r.push(r);
                let mut el = 0.0;
                let mut er = 0.0;
                for &(d, g) in room.taps_l.iter() {
                    // 反射は反対側の耳からも少し届く
                    el += room.delay_l.read(d) * g;
                    er += room.delay_l.read(d) * g * 0.4;
                }
                for &(d, g) in room.taps_r.iter() {
                    er += room.delay_r.read(d) * g;
                    el += room.delay_r.read(d) * g * 0.4;
                }
                room.lp_l = el * (1.0 - lp) + room.lp_l * lp;
                room.lp_r = er * (1.0 - lp) + room.lp_r * lp;
                l += room.lp_l * room_amt;
                r += room.lp_r * room_amt;
            }
            left[i] = l;
            right[i] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|v| v * v).sum()
    }

    fn render(channel: usize) -> (Vec<f32>, Vec<f32>) {
        let mut v = Virtualizer::new(48_000.0);
        v.apply(&SurroundSettings {
            enabled: true,
            room: 0.0,
            virtualize_stereo: true,
        });
        let n = 4800;
        let sig: Vec<f32> = (0..n)
            .map(|i| ((i * 2654435761usize) % 1000) as f32 / 1000.0 - 0.5)
            .collect();
        let zero = vec![0.0f32; n];
        let mut chans: [&[f32]; 8] = [&zero; 8];
        chans[channel] = &sig;
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        v.process(&chans, &mut l, &mut r);
        (l, r)
    }

    #[test]
    fn side_left_is_louder_in_left_ear() {
        let (l, r) = render(6); // SL (-90°)
        assert!(energy(&l) > energy(&r) * 1.5);
        let (l, r) = render(7); // SR (+90°)
        assert!(energy(&r) > energy(&l) * 1.5);
    }

    #[test]
    fn center_is_balanced() {
        let (l, r) = render(2);
        let ratio = energy(&l) / energy(&r);
        assert!((ratio - 1.0).abs() < 0.01);
    }

    #[test]
    fn itd_is_within_physical_range() {
        let max = ear_delay(PI);
        // 頭の大きさから、両耳の時間差はおおむね 0.7ms 程度
        assert!(max * 1000.0 < 1.0 && max * 1000.0 > 0.5);
    }
}
