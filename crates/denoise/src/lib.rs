//! AIノイズ除去。
//!
//! - 軽量モード: RNNoise（Rust 移植の `nnnoiseless`）
//! - 高品質モード: DeepFilterNet3（`deep_filter` の tract 実装、CPU で実時間処理できる）
//!
//! どちらも 48kHz・モノラル・480 サンプル（10ms）単位で処理する。

use serde::{Deserialize, Serialize};

pub const FRAME: usize = 480;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DenoiseMode {
    Off,
    #[default]
    Light,
    High,
}

impl DenoiseMode {
    pub fn label(&self) -> &'static str {
        match self {
            DenoiseMode::Off => "オフ",
            DenoiseMode::Light => "軽量（RNNoise）",
            DenoiseMode::High => "高品質（DeepFilterNet3）",
        }
    }
}

/// RNNoise による軽量ノイズ除去。音声らしさ（VAD 確率）も返す。
pub struct Rnnoise {
    state: Box<nnnoiseless::DenoiseState<'static>>,
    inbuf: [f32; FRAME],
    outbuf: [f32; FRAME],
}

impl Default for Rnnoise {
    fn default() -> Self {
        Self::new()
    }
}

impl Rnnoise {
    pub fn new() -> Self {
        Self {
            state: nnnoiseless::DenoiseState::new(),
            inbuf: [0.0; FRAME],
            outbuf: [0.0; FRAME],
        }
    }

    /// 1 フレームを処理する。`floor` は残す元音の割合（0 = 全部消す）。
    /// 戻り値は 0〜1 の音声らしさ。
    pub fn process(&mut self, frame: &mut [f32], floor: f32) -> f32 {
        debug_assert_eq!(frame.len(), FRAME);
        for (d, s) in self.inbuf.iter_mut().zip(frame.iter()) {
            *d = *s * 32768.0;
        }
        let vad = self.state.process_frame(&mut self.outbuf, &self.inbuf);
        for (o, (den, orig)) in frame
            .iter_mut()
            .zip(self.outbuf.iter().zip(self.inbuf.iter()))
        {
            let d = den / 32768.0;
            let x = orig / 32768.0;
            *o = d + (x - d) * floor;
        }
        vad
    }

    /// ノイズ除去はせず、音声らしさだけを求める（話者ゲートなどで使う）。
    pub fn vad_only(&mut self, frame: &[f32]) -> f32 {
        for (d, s) in self.inbuf.iter_mut().zip(frame.iter()) {
            *d = *s * 32768.0;
        }
        self.state.process_frame(&mut self.outbuf, &self.inbuf)
    }
}

#[cfg(feature = "deepfilter")]
mod dfn {
    use super::FRAME;
    use anyhow::{bail, Result};
    use df::tract::{DfParams, DfTract, RuntimeParams};
    use ndarray::{ArrayView2, ArrayViewMut2};

    /// DeepFilterNet3。モデルの読み込みに数百 ms かかるので、音声スレッドの外で作る。
    pub struct DeepFilter {
        model: DfTract,
        out: Vec<f32>,
        atten_db: f32,
    }

    // tract のモデルは別スレッドへ移せる
    unsafe impl Send for DeepFilter {}

    impl DeepFilter {
        pub fn new(atten_lim_db: f32) -> Result<Self> {
            let params = DfParams::default();
            let rp = RuntimeParams::default_with_ch(1)
                .with_atten_lim(atten_lim_db)
                .with_post_filter(0.0);
            let model = DfTract::new(params, &rp)?;
            if model.hop_size != FRAME || model.sr != 48_000 {
                bail!(
                    "DeepFilterNet のフレーム長が想定外です（{} / {}Hz）",
                    model.hop_size,
                    model.sr
                );
            }
            Ok(Self {
                model,
                out: vec![0.0; FRAME],
                atten_db: atten_lim_db,
            })
        }

        /// 抑制量の上限（dB）。100 でほぼ完全に消す。
        pub fn set_atten_lim(&mut self, db: f32) {
            if (db - self.atten_db).abs() > 0.01 {
                self.model.set_atten_lim(db);
                self.atten_db = db;
            }
        }

        /// モデルの先読みによる遅延（サンプル）。
        pub fn latency(&self) -> usize {
            self.model.lookahead * self.model.hop_size
        }

        /// 1 フレーム処理し、推定した SN 比（dB）を返す。
        pub fn process(&mut self, frame: &mut [f32]) -> Result<f32> {
            let noisy = ArrayView2::from_shape((1, FRAME), &*frame)?;
            let enh = ArrayViewMut2::from_shape((1, FRAME), &mut self.out[..])?;
            let lsnr = self.model.process(noisy, enh)?;
            frame.copy_from_slice(&self.out);
            Ok(lsnr)
        }
    }
}

#[cfg(feature = "deepfilter")]
pub use dfn::DeepFilter;

/// モードに応じたノイズ除去器。
pub enum Denoiser {
    Off,
    Light(Rnnoise),
    #[cfg(feature = "deepfilter")]
    High(Box<DeepFilter>),
}

impl Denoiser {
    /// 指定モードで作る。高品質モードの読み込みに失敗したら軽量モードに戻す。
    pub fn create(mode: DenoiseMode, strength_db: f32) -> (Denoiser, Option<String>) {
        match mode {
            DenoiseMode::Off => (Denoiser::Off, None),
            DenoiseMode::Light => (Denoiser::Light(Rnnoise::new()), None),
            DenoiseMode::High => {
                #[cfg(feature = "deepfilter")]
                {
                    match DeepFilter::new(strength_db) {
                        Ok(d) => (Denoiser::High(Box::new(d)), None),
                        Err(e) => (
                            Denoiser::Light(Rnnoise::new()),
                            Some(format!("高品質モードを開始できませんでした: {e}")),
                        ),
                    }
                }
                #[cfg(not(feature = "deepfilter"))]
                {
                    let _ = strength_db;
                    (
                        Denoiser::Light(Rnnoise::new()),
                        Some("このビルドには高品質モードが含まれていません".into()),
                    )
                }
            }
        }
    }

    pub fn mode(&self) -> DenoiseMode {
        match self {
            Denoiser::Off => DenoiseMode::Off,
            Denoiser::Light(_) => DenoiseMode::Light,
            #[cfg(feature = "deepfilter")]
            Denoiser::High(_) => DenoiseMode::High,
        }
    }

    /// 1 フレーム処理する。`strength_db` は抑制量の上限（0〜100dB）。
    pub fn process(&mut self, frame: &mut [f32], strength_db: f32) {
        match self {
            Denoiser::Off => {}
            Denoiser::Light(r) => {
                let floor = 10f32.powf(-strength_db.clamp(0.0, 100.0) / 20.0);
                r.process(frame, floor);
            }
            #[cfg(feature = "deepfilter")]
            Denoiser::High(d) => {
                d.set_atten_lim(strength_db.clamp(0.0, 100.0));
                if d.process(frame).is_err() {
                    // 推論に失敗したフレームはそのまま通す
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u32) -> f32 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        (*seed as f32 / u32::MAX as f32) - 0.5
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|v| v * v).sum()
    }

    #[test]
    fn rnnoise_reduces_white_noise() {
        let mut r = Rnnoise::new();
        let mut seed = 1u32;
        let mut e_in = 0.0;
        let mut e_out = 0.0;
        for i in 0..200 {
            let mut frame: Vec<f32> = (0..FRAME).map(|_| noise(&mut seed) * 0.05).collect();
            if i > 50 {
                e_in += energy(&frame);
            }
            r.process(&mut frame, 0.0);
            if i > 50 {
                e_out += energy(&frame);
            }
        }
        assert!(e_out < e_in * 0.2, "in {e_in} out {e_out}");
    }

    #[cfg(feature = "deepfilter")]
    #[test]
    fn deepfilter_runs_in_real_time_and_reduces_noise() {
        let mut d = DeepFilter::new(100.0).expect("model");
        let mut seed = 7u32;
        let mut e_in = 0.0;
        let mut e_out = 0.0;
        let frames = 300; // 3 秒
        let t0 = std::time::Instant::now();
        for i in 0..frames {
            let mut frame: Vec<f32> = (0..FRAME).map(|_| noise(&mut seed) * 0.05).collect();
            if i > 50 {
                e_in += energy(&frame);
            }
            d.process(&mut frame).unwrap();
            if i > 50 {
                e_out += energy(&frame);
            }
        }
        let elapsed = t0.elapsed().as_secs_f32();
        eprintln!(
            "DeepFilterNet3: 3.0 秒の音声を {elapsed:.2} 秒で処理（実時間比 {:.2}）, 遅延 {} サンプル",
            elapsed / 3.0,
            d.latency()
        );
        assert!(e_out < e_in * 0.2, "in {e_in} out {e_out}");
    }
}
