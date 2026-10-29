//! 話者埋め込みモデル（WeSpeaker ResNet34、ONNX）を tract で動かす。
//!
//! モデルは初回に Hugging Face から取得する（約 26MB、CC BY 4.0）。
//! 入力は [1, フレーム数, 80] の特徴量、出力は [1, 256] の声紋ベクトル。

use crate::fbank::{Fbank, N_MELS};
use anyhow::{Context, Result};
use std::path::Path;
use tract_onnx::prelude::*;

pub const MODEL_FILE: &str = "voxceleb_resnet34_LM.onnx";
pub const MODEL_URL: &str =
    "https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/resolve/main/voxceleb_resnet34_LM.onnx";
pub const MODEL_CREDIT: &str =
    "WeSpeaker ResNet34-LM（VoxCeleb で学習, CC BY 4.0, https://github.com/wenet-e2e/wespeaker）";

/// 判定に使う窓の長さ（1 秒 = 98 フレーム）。
pub const WINDOW_SAMPLES: usize = 16_000;

type Plan = TypedSimplePlan<TypedModel>;

pub struct EmbeddingModel {
    plan: Plan,
    frames: usize,
    fbank: Fbank,
}

impl EmbeddingModel {
    /// `window_samples`（16kHz）の長さの音声を扱えるよう、入力の形を固定して最適化する。
    pub fn load(path: &Path, window_samples: usize) -> Result<Self> {
        let frames = Fbank::num_frames(window_samples);
        let plan = tract_onnx::onnx()
            .model_for_path(path)
            .with_context(|| format!("話者モデルを読めません: {}", path.display()))?
            .with_input_fact(0, f32::fact([1, frames, N_MELS]).into())?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self {
            plan,
            frames,
            fbank: Fbank::new(),
        })
    }

    pub fn window_samples(&self) -> usize {
        // num_frames の逆算
        (self.frames - 1) * crate::fbank::FRAME_SHIFT + crate::fbank::FRAME_LEN
    }

    /// 16kHz の波形 1 窓分から、長さ 1 に正規化した声紋ベクトルを返す。
    pub fn embed(&mut self, wave16k: &[f32]) -> Result<Vec<f32>> {
        let need = self.window_samples();
        anyhow::ensure!(wave16k.len() >= need, "音声が短すぎます");
        let feats = self.fbank.compute(&wave16k[wave16k.len() - need..]);
        debug_assert_eq!(feats.len(), self.frames * N_MELS);
        let input = tract_ndarray::Array3::from_shape_vec((1, self.frames, N_MELS), feats)?;
        let out = self.plan.run(tvec!(Tensor::from(input).into()))?;
        let emb: Vec<f32> = out[0].to_array_view::<f32>()?.iter().copied().collect();
        Ok(normalize(emb))
    }
}

pub fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
    v
}

/// コサイン類似度（どちらも正規化済みなら内積と同じ）。
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        let n = normalize(vec![3.0, 4.0]);
        assert!((n[0] - 0.6).abs() < 1e-6);
    }

    /// 実際のモデルで動くか（CI でモデルを取得したときだけ実行）。
    #[test]
    fn real_model_produces_stable_embeddings() {
        let Ok(path) = std::env::var("CLARIVO_SPEAKER_MODEL") else {
            eprintln!("CLARIVO_SPEAKER_MODEL が無いのでスキップ");
            return;
        };
        let mut m = EmbeddingModel::load(Path::new(&path), WINDOW_SAMPLES).unwrap();
        // 声の代わりに、基本周波数の違う 2 種類の「声らしい」合成音を使う
        let voice = |f0: f32, seed: u32| -> Vec<f32> {
            let mut s = seed;
            (0..WINDOW_SAMPLES)
                .map(|i| {
                    s ^= s << 13;
                    s ^= s >> 17;
                    s ^= s << 5;
                    let t = i as f32 / 16_000.0;
                    let mut v = 0.0;
                    for h in 1..20 {
                        v += (2.0 * std::f32::consts::PI * f0 * h as f32 * t).sin() / h as f32;
                    }
                    v * 0.1 + (s as f32 / u32::MAX as f32 - 0.5) * 0.002
                })
                .collect()
        };
        let t0 = std::time::Instant::now();
        let a1 = m.embed(&voice(120.0, 1)).unwrap();
        eprintln!("埋め込み 1 回: {:?}", t0.elapsed());
        let a2 = m.embed(&voice(120.0, 2)).unwrap();
        let b = m.embed(&voice(240.0, 3)).unwrap();
        assert!(a1.len() >= 128);
        let same = cosine(&a1, &a2);
        let diff = cosine(&a1, &b);
        eprintln!("同じ音源 {same:.3} / 違う音源 {diff:.3}");
        assert!(same > diff);
    }
}
