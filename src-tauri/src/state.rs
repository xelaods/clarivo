//! アプリ全体で共有する状態と、設定の保存。

use crate::router::RouterHandle;
use crate::speaker::SpeakerManager;
use clarivo_engine::settings::{ChannelSettings, Settings};
use clarivo_engine::Engine;
use clarivo_routing::RoutingReport;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub settings_file: PathBuf,
    pub speakers_dir: PathBuf,
    pub models_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Paths {
    pub fn new(config_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            settings_file: config_dir.join("settings.json"),
            speakers_dir: config_dir.join("speakers"),
            models_dir: config_dir.join("models"),
            cache_dir,
            config_dir,
        }
    }

    pub fn speaker_model(&self) -> PathBuf {
        self.models_dir.join(clarivo_speaker::MODEL_FILE)
    }
}

/// ホットキーで実行する操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleMicMute,
    NextPreset,
    ChatmixGame,
    ChatmixChat,
    ToggleSurround,
}

/// ゲームごとのプロファイルを当てる前の設定（戻すときに使う）。
#[derive(Default)]
pub struct ProfileRuntime {
    pub base: Option<(ChannelSettings, f32)>,
    pub active_pid: Option<u32>,
}

pub struct AppState {
    pub engine: Mutex<Engine>,
    pub settings: Mutex<Settings>,
    pub paths: Paths,
    pub router: RouterHandle,
    pub routing_report: Mutex<RoutingReport>,
    pub speaker: Mutex<SpeakerManager>,
    pub hotkeys: Mutex<HashMap<u32, HotkeyAction>>,
    pub profile: Mutex<ProfileRuntime>,
    pub autoeq_index: Mutex<Option<Vec<crate::autoeq::AutoEqEntry>>>,
    /// オーバーレイを隠すタイミングの管理（新しい表示が来たら古い予約を無効にする）
    pub overlay_generation: std::sync::atomic::AtomicU64,
}

pub fn load_settings(paths: &Paths) -> Settings {
    match std::fs::read_to_string(&paths.settings_file) {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("設定ファイルを読めませんでした（初期値で起動します）: {e}");
                // 壊れたファイルは残しておく
                let _ = std::fs::copy(
                    &paths.settings_file,
                    paths.config_dir.join("settings.broken.json"),
                );
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(paths: &Paths, s: &Settings) {
    if let Err(e) = std::fs::create_dir_all(&paths.config_dir) {
        log::warn!("設定フォルダを作れません: {e}");
        return;
    }
    let tmp = paths.config_dir.join("settings.json.tmp");
    match serde_json::to_string_pretty(s) {
        Ok(json) => {
            // 書きかけで壊れないよう、一時ファイルに書いてから置き換える
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &paths.settings_file);
            }
        }
        Err(e) => log::warn!("設定を保存できません: {e}"),
    }
}
