//! 登録した話者の声だけを通す機能（フェーズ4）。
//!
//! - [`profile`]: 声の登録と保存
//! - [`worker`]: 段階A「話者ゲート」の判定スレッド
//! - [`extract`]: 段階B「話者抽出モデル」の差し込み口（試験機能）

pub mod extract;
pub mod fbank;
pub mod model;
pub mod profile;
pub mod worker;

pub use model::{EmbeddingModel, MODEL_CREDIT, MODEL_FILE, MODEL_URL, WINDOW_SAMPLES};
pub use profile::{ProfileStore, ProfileSummary, SpeakerProfile};
pub use worker::{GateParams, SpeakerWorker};
