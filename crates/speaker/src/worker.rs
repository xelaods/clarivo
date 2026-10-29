//! 話者ゲートの判定スレッド（段階A）。
//!
//! マイクスレッドから届くノイズ除去後の音声を 16kHz に落とし、直近 1 秒に声が
//! あれば 250ms ごとに声紋を計算して、登録した声紋と比べる。判定結果は
//! [`SpeakerLink`] に書き、マイクスレッドがそれを見て音量を下げる。

use crate::model::{cosine, EmbeddingModel};
use clarivo_denoise::Rnnoise;
use clarivo_dsp::resample::Decimator3;
use clarivo_engine::chains::{SpeakerLink, DECISION_OTHER, DECISION_SELF, DECISION_UNKNOWN};
use clarivo_engine::state::Shared;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct GateParams {
    pub threshold: f32,
    pub hold_ms: f32,
}

/// 判定の状態遷移（スレッドから切り離してテストできるようにしてある）。
#[derive(Debug, Clone)]
pub struct Decider {
    pub params: GateParams,
    state: u8,
    low_count: u32,
    last_voice: Option<Instant>,
}

impl Decider {
    pub fn new(params: GateParams) -> Self {
        Self {
            params,
            state: DECISION_UNKNOWN,
            low_count: 0,
            last_voice: None,
        }
    }

    pub fn state(&self) -> u8 {
        self.state
    }

    /// 声があるときの類似度を渡す。
    pub fn on_score(&mut self, score: f32, now: Instant) -> u8 {
        self.last_voice = Some(now);
        if score >= self.params.threshold {
            self.state = DECISION_SELF;
            self.low_count = 0;
        } else if score < self.params.threshold - 0.05 {
            // 1 回の低い値ではまだ閉じない（話し始めの揺れを避ける）
            self.low_count += 1;
            if self.low_count >= 2 {
                self.state = DECISION_OTHER;
            }
        }
        self.state
    }

    /// 声が無い状態が続いたら判定を解除する（マイクは通常のゲートに任せる）。
    pub fn on_silence(&mut self, now: Instant) -> u8 {
        let hold = Duration::from_secs_f32(self.params.hold_ms.max(0.0) / 1000.0);
        if self
            .last_voice
            .map(|t| now.duration_since(t) > hold)
            .unwrap_or(true)
        {
            self.state = DECISION_UNKNOWN;
            self.low_count = 0;
        }
        self.state
    }
}

pub struct SpeakerWorker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<rtrb::Consumer<f32>>>,
}

impl SpeakerWorker {
    pub fn spawn(
        mut tap: rtrb::Consumer<f32>,
        mut model: EmbeddingModel,
        reference: Vec<f32>,
        params: GateParams,
        link: Arc<SpeakerLink>,
        shared: Arc<Shared>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = std::thread::Builder::new()
            .name("clarivo-speaker".into())
            .spawn(move || {
                let mut vad = Rnnoise::new();
                let mut dec = Decimator3::new();
                let win = model.window_samples();
                let mut ring16: VecDeque<f32> = VecDeque::with_capacity(win * 2);
                let mut voiced_flags: VecDeque<bool> = VecDeque::with_capacity(128);
                let mut frame48 = Vec::with_capacity(480);
                let mut tmp16 = Vec::with_capacity(480);
                let mut window = vec![0.0f32; win];
                let mut decider = Decider::new(params);
                let mut last_eval = Instant::now();
                let meters = &shared.meters;
                while !stop2.load(Ordering::Relaxed) {
                    while let Ok(s) = tap.pop() {
                        frame48.push(s);
                        if frame48.len() == 480 {
                            let v = vad.vad_only(&frame48) > 0.6;
                            voiced_flags.push_back(v);
                            if voiced_flags.len() > 100 {
                                voiced_flags.pop_front();
                            }
                            tmp16.clear();
                            dec.process(&frame48, &mut tmp16);
                            ring16.extend(tmp16.iter().copied());
                            while ring16.len() > win {
                                ring16.pop_front();
                            }
                            frame48.clear();
                        }
                    }
                    let now = Instant::now();
                    if now.duration_since(last_eval) >= Duration::from_millis(250) {
                        last_eval = now;
                        let voiced = voiced_flags.iter().filter(|v| **v).count();
                        let enough = ring16.len() >= win && voiced_flags.len() >= 50;
                        // 直近 1 秒の 4 割以上に声があるときだけ判定する
                        if enough && voiced * 10 >= voiced_flags.len() * 4 {
                            for (d, s) in window.iter_mut().zip(ring16.iter()) {
                                *d = *s;
                            }
                            match model.embed(&window) {
                                Ok(e) => {
                                    let score = cosine(&e, &reference);
                                    meters.speaker_score.store(score);
                                    let st = decider.on_score(score, now);
                                    link.decision.store(st, Ordering::Relaxed);
                                    meters.speaker_state.store(
                                        if st == DECISION_OTHER { 2 } else if st == DECISION_SELF { 1 } else { 3 },
                                        Ordering::Relaxed,
                                    );
                                }
                                Err(e) => log::warn!("声紋の計算に失敗: {e:#}"),
                            }
                        } else {
                            let st = decider.on_silence(now);
                            link.decision.store(st, Ordering::Relaxed);
                            if st == DECISION_UNKNOWN {
                                meters.speaker_state.store(0, Ordering::Relaxed);
                                meters.speaker_score.store(f32::NAN);
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                link.decision.store(DECISION_UNKNOWN, Ordering::Relaxed);
                meters.speaker_state.store(0, Ordering::Relaxed);
                meters.speaker_score.store(f32::NAN);
                // 取り出し口は返して、設定を変えたときに使い回す
                tap
            })
            .expect("話者判定スレッドを開始できません");
        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// 止めて、マイク音の取り出し口を返す。
    pub fn stop(&mut self) -> Option<rtrb::Consumer<f32>> {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.take().and_then(|h| h.join().ok())
    }
}

impl Drop for SpeakerWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decider_needs_two_low_scores_and_reopens_quickly() {
        let mut d = Decider::new(GateParams {
            threshold: 0.5,
            hold_ms: 600.0,
        });
        let t = Instant::now();
        assert_eq!(d.on_score(0.2, t), DECISION_UNKNOWN);
        assert_eq!(d.on_score(0.2, t), DECISION_OTHER);
        assert_eq!(d.on_score(0.7, t), DECISION_SELF);
        // しきい値付近（0.45〜0.5）では状態を変えない
        assert_eq!(d.on_score(0.47, t), DECISION_SELF);
        // 声が途切れて hold を過ぎたら解除
        assert_eq!(d.on_silence(t + Duration::from_millis(100)), DECISION_SELF);
        assert_eq!(d.on_silence(t + Duration::from_millis(700)), DECISION_UNKNOWN);
    }
}
