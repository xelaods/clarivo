//! Clarivo の音声処理部品。
//!
//! ここにある処理は OS に依存しない。Windows の音声入出力は `clarivo-engine` が担い、
//! そこからこのクレートの部品を呼び出す。すべて 48kHz・32bit 浮動小数を前提にし、
//! 音声スレッドから呼ばれる `process` 系の関数ではメモリ確保をしない。

pub mod autoeq;
pub mod biquad;
pub mod drift;
pub mod dynamics;
pub mod meter;
pub mod mixer;
pub mod peq;
pub mod presets;
pub mod resample;
pub mod smoother;
pub mod surround;
pub mod wav;

/// エンジン全体で使うサンプルレート。
pub const SAMPLE_RATE: f32 = 48_000.0;
/// 処理単位（10ms）。DeepFilterNet と RNNoise の 1 フレームと同じ長さ。
pub const BLOCK: usize = 480;

/// dB を倍率に変換する。
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// 倍率を dB に変換する。0 は -120dB として扱う。
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 1e-6 {
        -120.0
    } else {
        20.0 * gain.log10()
    }
}

/// ミリ秒の時定数から、1 サンプルごとの一次平滑の係数を求める。
#[inline]
pub fn time_coef(ms: f32, sample_rate: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        (-1.0 / (ms * 0.001 * sample_rate)).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_round_trip() {
        for db in [-60.0f32, -6.0, 0.0, 3.0, 12.0] {
            assert!((gain_to_db(db_to_gain(db)) - db).abs() < 1e-4);
        }
        assert_eq!(gain_to_db(0.0), -120.0);
    }
}
