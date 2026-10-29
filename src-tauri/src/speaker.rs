//! 話者ゲート（段階A）と話者抽出（段階B）の起動・停止を、設定に合わせて行う。

use crate::state::Paths;
use clarivo_engine::settings::{Settings, SpeakerMode};
use clarivo_engine::Engine;
use clarivo_speaker::extract::Extractor;
use clarivo_speaker::{EmbeddingModel, GateParams, ProfileStore, SpeakerWorker, WINDOW_SAMPLES};

#[derive(Default)]
pub struct SpeakerManager {
    worker: Option<SpeakerWorker>,
    tap: Option<rtrb::Consumer<f32>>,
    worker_key: Option<String>,
    extractor_key: Option<String>,
}

impl SpeakerManager {
    /// エンジンを開始し直したときに、新しいマイク音の取り出し口を受け取る。
    pub fn reset_tap(&mut self, engine: &Engine) {
        if let Some(mut w) = self.worker.take() {
            w.stop();
        }
        self.worker_key = None;
        self.extractor_key = None;
        self.tap = engine.take_speaker_tap();
    }

    /// 設定に合わせて判定スレッドや抽出モデルを用意する。問題があれば説明を返す。
    pub fn sync(&mut self, engine: &Engine, settings: &Settings, paths: &Paths) -> Option<String> {
        let sp = &settings.mic.speaker;
        let store = ProfileStore::new(&paths.speakers_dir);
        let profile = sp.profile.as_deref().and_then(|id| store.get(id));

        // --- 段階A：話者ゲート ---
        let want_gate = sp.mode == SpeakerMode::Gate;
        let mut problem = None;
        if want_gate {
            match &profile {
                None => problem = Some("話者ゲートを使うには、声を登録して選んでください".to_string()),
                Some(p) => {
                    let key = format!("{}:{:.3}:{:.0}", p.id, sp.threshold, sp.hold_ms);
                    if self.worker_key.as_deref() != Some(key.as_str()) {
                        self.stop_worker();
                        let model_path = paths.speaker_model();
                        if !model_path.exists() {
                            problem = Some("話者モデルがまだダウンロードされていません".into());
                        } else {
                            match (EmbeddingModel::load(&model_path, WINDOW_SAMPLES), self.tap.take()) {
                                (Ok(model), Some(tap)) => {
                                    self.worker = Some(SpeakerWorker::spawn(
                                        tap,
                                        model,
                                        p.embedding.clone(),
                                        GateParams {
                                            threshold: sp.threshold,
                                            hold_ms: sp.hold_ms,
                                        },
                                        engine.speaker_link(),
                                        engine.shared(),
                                    ));
                                    self.worker_key = Some(key);
                                }
                                (Err(e), tap) => {
                                    self.tap = tap;
                                    problem = Some(format!("話者モデルを読み込めません: {e:#}"));
                                }
                                (Ok(_), None) => {
                                    problem = Some("マイクが動いていないため、話者ゲートを開始できません".into());
                                }
                            }
                        }
                    }
                }
            }
        } else {
            self.stop_worker();
        }

        // --- 段階B：話者抽出（試験機能） ---
        let want_extract = sp.mode == SpeakerMode::Extract;
        if want_extract {
            match (&profile, sp.extract_model.as_deref()) {
                (Some(p), Some(model)) => {
                    let key = format!("{}:{}", p.id, model);
                    if self.extractor_key.as_deref() != Some(key.as_str()) {
                        match Extractor::load(std::path::Path::new(model), p.embedding.clone()) {
                            Ok(ex) => {
                                engine.set_extractor(Some(Box::new(ex)));
                                self.extractor_key = Some(key);
                            }
                            Err(e) => {
                                engine.set_extractor(None);
                                self.extractor_key = None;
                                problem = Some(format!("話者抽出モデルを読み込めません: {e:#}"));
                            }
                        }
                    }
                }
                (None, _) => problem = Some("話者抽出を使うには、声を登録して選んでください".into()),
                (_, None) => problem = Some("話者抽出モデル（ONNX）のファイルを選んでください".into()),
            }
        } else if self.extractor_key.take().is_some() {
            engine.set_extractor(None);
        }
        problem
    }

    fn stop_worker(&mut self) {
        if let Some(mut w) = self.worker.take() {
            if let Some(tap) = w.stop() {
                self.tap = Some(tap);
            }
        }
        self.worker_key = None;
    }

    pub fn shutdown(&mut self) {
        self.stop_worker();
    }
}
