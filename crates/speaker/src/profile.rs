//! 声紋プロファイルの登録と保存。録音そのものは保存せず、声紋ベクトルだけを残す。

use crate::model::{normalize, EmbeddingModel};
use anyhow::{bail, Result};
use clarivo_denoise::Rnnoise;
use clarivo_dsp::resample::Decimator3;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerProfile {
    pub id: String,
    pub name: String,
    /// 登録日時（UNIX 秒）
    pub created: u64,
    /// 登録に使えた音声の長さ（秒）
    pub voiced_seconds: f32,
    pub embedding: Vec<f32>,
}

/// 一覧表示用（声紋ベクトルを含めない）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub created: u64,
    pub voiced_seconds: f32,
}

impl From<&SpeakerProfile> for ProfileSummary {
    fn from(p: &SpeakerProfile) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            created: p.created,
            voiced_seconds: p.voiced_seconds,
        }
    }
}

pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, id: &str) -> PathBuf {
        // ID はこちらで作る英数字のみだが、念のためファイル名に使えない文字を除く
        let safe: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        self.dir.join(format!("{safe}.json"))
    }

    pub fn list(&self) -> Vec<SpeakerProfile> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut v: Vec<SpeakerProfile> = rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect();
        v.sort_by_key(|p| p.created);
        v
    }

    pub fn get(&self, id: &str) -> Option<SpeakerProfile> {
        let s = std::fs::read_to_string(self.path(id)).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save(&self, p: &SpeakerProfile) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.path(&p.id), serde_json::to_string_pretty(p)?)?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let p = self.path(id);
        if p.exists() {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// 48kHz の録音を、判定時と同じ前処理（軽量ノイズ除去 → 16kHz）にかけ、
/// 声のある部分だけを残す。戻り値は 16kHz の波形。
pub fn voiced_16k(recording48k: &[f32]) -> Vec<f32> {
    let mut rn = Rnnoise::new();
    let mut vad_rn = Rnnoise::new();
    let mut dec = Decimator3::new();
    let mut out = Vec::new();
    let mut frame = vec![0.0f32; 480];
    for chunk in recording48k.chunks_exact(480) {
        frame.copy_from_slice(chunk);
        let vad = vad_rn.vad_only(&frame);
        // 判定スレッドと同じく、ほぼ完全に消す強さで処理する
        rn.process(&mut frame, 0.001);
        if vad > 0.5 {
            dec.process(&frame, &mut out);
        }
    }
    out
}

/// 録音から声紋を作る。1 秒の窓を半分ずつずらしながら平均する。
pub fn enroll(
    model: &mut EmbeddingModel,
    recording48k: &[f32],
    id: String,
    name: String,
    now_unix: u64,
) -> Result<SpeakerProfile> {
    let voiced = voiced_16k(recording48k);
    let win = model.window_samples();
    let voiced_seconds = voiced.len() as f32 / 16_000.0;
    if voiced.len() < win * 4 {
        bail!(
            "声が聞き取れた部分が短すぎます（{voiced_seconds:.1} 秒）。もう少しはっきり、長めに読み上げてください"
        );
    }
    let mut sum: Vec<f32> = Vec::new();
    let mut count = 0usize;
    let mut start = 0;
    while start + win <= voiced.len() {
        let e = model.embed(&voiced[start..start + win])?;
        if sum.is_empty() {
            sum = vec![0.0; e.len()];
        }
        for (s, v) in sum.iter_mut().zip(e.iter()) {
            *s += v;
        }
        count += 1;
        start += win / 2;
    }
    if count == 0 {
        bail!("声紋を作れませんでした");
    }
    Ok(SpeakerProfile {
        id,
        name,
        created: now_unix,
        voiced_seconds,
        embedding: normalize(sum),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_round_trip() {
        let dir = std::env::temp_dir().join(format!("clarivo-speaker-test-{}", std::process::id()));
        let store = ProfileStore::new(&dir);
        let p = SpeakerProfile {
            id: "abc123".into(),
            name: "自分".into(),
            created: 1,
            voiced_seconds: 20.0,
            embedding: vec![0.6, 0.8],
        };
        store.save(&p).unwrap();
        assert_eq!(store.get("abc123").unwrap(), p);
        assert_eq!(store.list().len(), 1);
        store.delete("abc123").unwrap();
        assert!(store.list().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn silence_has_no_voiced_audio() {
        let v = voiced_16k(&vec![0.0; 48_000]);
        assert!(v.is_empty());
    }
}
