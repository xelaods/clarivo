//! レベルメーター。音声スレッドで書き、画面側スレッドで読む。

use crate::gain_to_db;
use std::sync::atomic::{AtomicU32, Ordering};

/// ロックなしで共有できる f32。
#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    #[inline]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    #[inline]
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
}

/// ピーク（dBFS）と RMS（dBFS）を保持する。
#[derive(Debug)]
pub struct LevelMeter {
    pub peak_db: AtomicF32,
    pub rms_db: AtomicF32,
}

impl Default for LevelMeter {
    fn default() -> Self {
        Self {
            peak_db: AtomicF32::new(-120.0),
            rms_db: AtomicF32::new(-120.0),
        }
    }
}

impl LevelMeter {
    /// 1 ブロック分の値で更新する。ピークは落ちる速さを緩やかにする。
    pub fn update(&self, chans: &[&[f32]]) {
        let mut peak = 0.0f32;
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for c in chans {
            for &s in c.iter() {
                peak = peak.max(s.abs());
                sum += s * s;
            }
            count += c.len();
        }
        let peak_db = gain_to_db(peak);
        let rms_db = if count > 0 {
            gain_to_db((sum / count as f32).sqrt())
        } else {
            -120.0
        };
        // 10ms ごとに約 1.5dB ずつ下がる
        let prev = self.peak_db.load();
        self.peak_db.store(peak_db.max(prev - 1.5));
        let prev_rms = self.rms_db.load();
        self.rms_db.store(if rms_db > prev_rms {
            rms_db
        } else {
            prev_rms * 0.7 + rms_db * 0.3
        });
    }

    pub fn snapshot(&self) -> (f32, f32) {
        (self.peak_db.load(), self.rms_db.load())
    }

    pub fn reset(&self) {
        self.peak_db.store(-120.0);
        self.rms_db.store(-120.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_tracks_peak_and_decays() {
        let m = LevelMeter::default();
        let loud = vec![0.5f32; 480];
        m.update(&[&loud]);
        let (p, _) = m.snapshot();
        assert!((p - gain_to_db(0.5)).abs() < 1e-3);
        let silent = vec![0.0f32; 480];
        m.update(&[&silent]);
        assert!(m.snapshot().0 < p);
    }
}
