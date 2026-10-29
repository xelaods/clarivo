//! 音声スレッドと画面の間で共有する状態（メーター、動作状況）。

use clarivo_dsp::meter::{AtomicF32, LevelMeter};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[derive(Default)]
pub struct Meters {
    pub mic_in: LevelMeter,
    pub mic_out: LevelMeter,
    pub game: LevelMeter,
    pub chat: LevelMeter,
    pub master: LevelMeter,
    pub stream: LevelMeter,
    pub gate_open: AtomicBool,
    pub mic_reduction_db: AtomicF32,
    pub limiter_reduction_db: AtomicF32,
    /// 話者判定の類似度（-1〜1、判定していないときは NaN）
    pub speaker_score: AtomicF32,
    /// 0 = 無音, 1 = 本人, 2 = 他人, 3 = 判定中
    pub speaker_state: AtomicU8,
    /// 処理時間 ÷ 音声の長さ（1 を超えると処理が追いつかない）
    pub mic_load: AtomicF32,
    pub output_load: AtomicF32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelSnapshot {
    pub peak: f32,
    pub rms: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterSnapshot {
    pub mic_in: LevelSnapshot,
    pub mic_out: LevelSnapshot,
    pub game: LevelSnapshot,
    pub chat: LevelSnapshot,
    pub master: LevelSnapshot,
    pub stream: LevelSnapshot,
    pub gate_open: bool,
    pub mic_reduction_db: f32,
    pub limiter_reduction_db: f32,
    pub speaker_score: Option<f32>,
    pub speaker_state: &'static str,
    pub mic_load: f32,
    pub output_load: f32,
}

fn snap(m: &LevelMeter) -> LevelSnapshot {
    let (peak, rms) = m.snapshot();
    LevelSnapshot { peak, rms }
}

impl Meters {
    pub fn new() -> Self {
        let m = Self::default();
        m.speaker_score.store(f32::NAN);
        m
    }

    pub fn snapshot(&self) -> MeterSnapshot {
        let score = self.speaker_score.load();
        MeterSnapshot {
            mic_in: snap(&self.mic_in),
            mic_out: snap(&self.mic_out),
            game: snap(&self.game),
            chat: snap(&self.chat),
            master: snap(&self.master),
            stream: snap(&self.stream),
            gate_open: self.gate_open.load(Ordering::Relaxed),
            mic_reduction_db: self.mic_reduction_db.load(),
            limiter_reduction_db: self.limiter_reduction_db.load(),
            speaker_score: if score.is_nan() { None } else { Some(score) },
            speaker_state: match self.speaker_state.load(Ordering::Relaxed) {
                1 => "self",
                2 => "other",
                3 => "checking",
                _ => "silent",
            },
            mic_load: self.mic_load.load(),
            output_load: self.output_load.load(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStatus {
    pub running: bool,
    pub error: Option<String>,
    /// 使っているデバイス名
    pub devices: Vec<String>,
    /// 推定遅延（ms）
    pub latency_ms: f32,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub mic: PipelineStatus,
    pub output: PipelineStatus,
    /// 画面に一度だけ知らせたいお知らせ
    pub notices: Vec<String>,
    /// VB-CABLE が見つかったか
    pub cables: CableStatus,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CableStatus {
    pub mic: bool,
    pub game: bool,
    pub chat: bool,
}

pub struct Shared {
    pub meters: Meters,
    pub status: Mutex<EngineStatus>,
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

impl Shared {
    pub fn new() -> Self {
        Self {
            meters: Meters::new(),
            status: Mutex::new(EngineStatus::default()),
        }
    }

    pub fn notice(&self, msg: impl Into<String>) {
        let msg = msg.into();
        log::info!("{msg}");
        let mut st = self.status.lock();
        if !st.notices.contains(&msg) {
            st.notices.push(msg);
        }
    }

    /// お知らせを取り出す（取り出したものは消える）。
    pub fn take_notices(&self) -> Vec<String> {
        std::mem::take(&mut self.status.lock().notices)
    }
}
